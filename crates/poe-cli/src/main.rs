use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use poe_ggpk::{
    default_cache_root, fetch_latest_patch_versions, parse_bundle_header, parse_index_bundle,
    read_dat_table, root_directories, table_name_from_path, unpack_path_reps, CacheMode,
    DatSchemaClient, DiskCache, PatchCdnSource, PoeGame, DEFAULT_DAT_SCHEMA_URL,
};

const DEFAULT_DAT_SCHEMA_PATH: &str = "schema/dat/_Core.gql";
const DEFAULT_DAT_SCHEMA_MANIFEST_PATH: &str = "schema/dat/schema-manifest.json";
const REQUIRED_DAT_SCHEMA_TYPES: &[&str] = &["WorldAreas", "Topologies"];

#[derive(Debug, Parser)]
#[command(author, version, about = "Path of Exile layout pipeline tools")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the latest known `PoE` patch versions.
    LatestVersions,
    /// Fetch and summarize the patch CDN bundle index.
    SnapshotIndex {
        /// Patch CDN version such as x.y.z.w. Defaults to the live `PoE1` version endpoint.
        #[arg(long)]
        patch_version: Option<String>,
        /// Cache root. Defaults to .poe-layouts/cache under the current directory.
        #[arg(long)]
        cache_root: Option<PathBuf>,
        /// Read from cache only and fail on cache miss.
        #[arg(long)]
        offline: bool,
    },
    /// Fetch `_.index.bin` and print compressed bundle header metadata.
    InspectIndexHeader {
        /// Patch CDN version such as x.y.z.w. Defaults to the live `PoE1` version endpoint.
        #[arg(long)]
        patch_version: Option<String>,
        /// Cache root. Defaults to .poe-layouts/cache under the current directory.
        #[arg(long)]
        cache_root: Option<PathBuf>,
        /// Read from cache only and fail on cache miss.
        #[arg(long)]
        offline: bool,
    },
    /// Fetch, decompress, and parse the live `_.index.bin` bundle.
    InspectIndex {
        /// Patch CDN version such as x.y.z.w. Defaults to the live `PoE1` version endpoint.
        #[arg(long)]
        patch_version: Option<String>,
        /// Cache root. Defaults to .poe-layouts/cache under the current directory.
        #[arg(long)]
        cache_root: Option<PathBuf>,
        /// Read from cache only and fail on cache miss.
        #[arg(long)]
        offline: bool,
        /// Optional logical path to resolve inside the index.
        #[arg(long)]
        logical_path: Option<String>,
        /// Optional prefix for sampling unpacked logical paths.
        #[arg(long)]
        prefix: Option<String>,
        /// Maximum number of prefix sample paths to print.
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Node.js executable used for the temporary `ooz-wasm` bridge.
        #[arg(long, default_value = "node")]
        node: PathBuf,
        /// Bridge script used to decompress Oodle bundle files.
        #[arg(long)]
        ooz_script: Option<PathBuf>,
    },
    /// Parse an already-decompressed `_.index.bin` payload.
    InspectDecompressedIndex {
        /// Path to decompressed index bytes.
        #[arg(long)]
        input: PathBuf,
        /// Optional logical path to resolve inside the index.
        #[arg(long)]
        logical_path: Option<String>,
    },
    /// Extract one logical file from patch CDN bundles.
    ExtractFile {
        /// Logical path from the unpacked index, for example data/worldareas.datc64.
        #[arg(long)]
        logical_path: String,
        /// Output file path.
        #[arg(long)]
        out: PathBuf,
        /// Patch CDN version such as x.y.z.w. Defaults to the live `PoE1` version endpoint.
        #[arg(long)]
        patch_version: Option<String>,
        /// Cache root. Defaults to .poe-layouts/cache under the current directory.
        #[arg(long)]
        cache_root: Option<PathBuf>,
        /// Read from cache only and fail on cache miss.
        #[arg(long)]
        offline: bool,
        /// Node.js executable used for the temporary `ooz-wasm` bridge.
        #[arg(long, default_value = "node")]
        node: PathBuf,
        /// Bridge script used to decompress Oodle bundle files.
        #[arg(long)]
        ooz_script: Option<PathBuf>,
    },
    /// Prefetch named bundle files and write an offline cache manifest.
    PrefetchBundles {
        /// Patch CDN version such as x.y.z.w. Defaults to the live `PoE1` version endpoint.
        #[arg(long)]
        patch_version: Option<String>,
        /// Cache root. Defaults to .poe-layouts/cache under the current directory.
        #[arg(long)]
        cache_root: Option<PathBuf>,
        /// Bundle name under Bundles2, for example _.index.bin.
        #[arg(long = "bundle")]
        bundles: Vec<String>,
        /// Newline-delimited bundle names to prefetch.
        #[arg(long)]
        bundle_list: Option<PathBuf>,
        /// Manifest output path.
        #[arg(long, default_value = ".poe-layouts/cache-manifest.json")]
        manifest: PathBuf,
        /// Cache/network behavior.
        #[arg(long, value_enum, default_value_t = NetworkMode::Online)]
        mode: NetworkMode,
    },
    /// Verify an offline cache manifest without network access.
    VerifyCache {
        /// Cache root. Defaults to .poe-layouts/cache under the current directory.
        #[arg(long)]
        cache_root: Option<PathBuf>,
        /// Manifest path to verify.
        #[arg(long, default_value = ".poe-layouts/cache-manifest.json")]
        manifest: PathBuf,
    },
    /// Clear cached `PoE1` bundle files by release line or exact patch version.
    ClearCache {
        /// Cache root. Defaults to .poe-layouts/cache under the current directory.
        #[arg(long)]
        cache_root: Option<PathBuf>,
        /// Release line to clear. Defaults to the latest `PoE1` release line.
        #[arg(long)]
        release_line: Option<String>,
        /// Clear only one exact patch version under its release line.
        #[arg(long)]
        patch_version: Option<String>,
        /// Print what would be removed without deleting it.
        #[arg(long)]
        dry_run: bool,
    },
    /// Fetch the GraphQL table schema used to parse `.dat`/`.datc64` files.
    UpdateDatSchema {
        /// Source URL for the schema snapshot.
        #[arg(long, default_value = DEFAULT_DAT_SCHEMA_URL)]
        url: String,
        /// Cache root. Defaults to .poe-layouts/cache under the current directory.
        #[arg(long)]
        cache_root: Option<PathBuf>,
        /// Read the schema from cache only and fail on cache miss.
        #[arg(long)]
        offline: bool,
        /// Output path for the fetched GraphQL schema.
        #[arg(long, default_value = DEFAULT_DAT_SCHEMA_PATH)]
        out: PathBuf,
        /// Output path for the schema snapshot manifest.
        #[arg(long, default_value = DEFAULT_DAT_SCHEMA_MANIFEST_PATH)]
        manifest: PathBuf,
    },
    /// Read projected columns from a local `.datc64` table file.
    InspectDatTable {
        /// Input `.datc64` file.
        #[arg(long)]
        input: PathBuf,
        /// GraphQL dat schema snapshot.
        #[arg(long, default_value = DEFAULT_DAT_SCHEMA_PATH)]
        schema: PathBuf,
        /// Table name. Defaults to the input file stem.
        #[arg(long)]
        table: Option<String>,
        /// Column to read. Repeat this flag to project multiple columns.
        #[arg(long = "column")]
        columns: Vec<String>,
        /// Maximum number of rows to print.
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum NetworkMode {
    Online,
    Offline,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    match args.command {
        Command::LatestVersions => latest_versions(),
        Command::SnapshotIndex {
            patch_version,
            cache_root,
            offline,
        } => snapshot_index(patch_version, cache_root, offline),
        Command::InspectIndexHeader {
            patch_version,
            cache_root,
            offline,
        } => inspect_index_header(patch_version, cache_root, offline),
        Command::InspectIndex {
            patch_version,
            cache_root,
            offline,
            logical_path,
            prefix,
            limit,
            node,
            ooz_script,
        } => inspect_index(InspectIndexOptions {
            patch_version,
            cache_root,
            offline,
            logical_path: logical_path.as_deref(),
            prefix: prefix.as_deref(),
            limit,
            node: &node,
            ooz_script: ooz_script.as_deref(),
        }),
        Command::InspectDecompressedIndex {
            input,
            logical_path,
        } => inspect_decompressed_index(&input, logical_path.as_deref()),
        Command::ExtractFile {
            logical_path,
            out,
            patch_version,
            cache_root,
            offline,
            node,
            ooz_script,
        } => extract_file(ExtractFileOptions {
            logical_path: &logical_path,
            out: &out,
            patch_version,
            cache_root,
            offline,
            node: &node,
            ooz_script: ooz_script.as_deref(),
        }),
        Command::PrefetchBundles {
            patch_version,
            cache_root,
            bundles,
            bundle_list,
            manifest,
            mode,
        } => prefetch_bundles(
            patch_version,
            cache_root,
            bundles,
            bundle_list,
            &manifest,
            mode,
        ),
        Command::VerifyCache {
            cache_root,
            manifest,
        } => verify_cache(cache_root, &manifest),
        Command::ClearCache {
            cache_root,
            release_line,
            patch_version,
            dry_run,
        } => clear_cache(cache_root, release_line, patch_version, dry_run),
        Command::UpdateDatSchema {
            url,
            cache_root,
            offline,
            out,
            manifest,
        } => update_dat_schema(&url, cache_root, offline, &out, &manifest),
        Command::InspectDatTable {
            input,
            schema,
            table,
            columns,
            limit,
        } => inspect_dat_table(&input, &schema, table, columns, limit),
    }
}

fn latest_versions() -> anyhow::Result<()> {
    let versions = fetch_latest_patch_versions().context("fetch latest PoE patch versions")?;
    let poe1 = versions.poe1_source();
    let poe2 = versions.poe2_source();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "poe1": {
                "patch_version": versions.poe,
                "release_line": poe1.release_line(),
                "cache_namespace": poe1.cache_namespace(),
            },
            "poe2": {
                "patch_version": versions.poe2,
                "release_line": poe2.release_line(),
                "cache_namespace": poe2.cache_namespace(),
            },
        }))?
    );
    Ok(())
}

fn cache_from_arg(cache_root: Option<PathBuf>) -> DiskCache {
    cache_root.map_or_else(
        || default_cache_root(&std::env::current_dir().expect("current dir")),
        DiskCache::new,
    )
}

fn resolve_poe1_patch_version(patch_version: Option<String>) -> anyhow::Result<String> {
    Ok(match patch_version {
        Some(version) => version,
        None => {
            fetch_latest_patch_versions()
                .context("fetch latest PoE patch versions")?
                .poe
        }
    })
}

fn snapshot_index(
    patch_version: Option<String>,
    cache_root: Option<PathBuf>,
    offline: bool,
) -> anyhow::Result<()> {
    let patch_version = resolve_poe1_patch_version(patch_version)?;
    let cache = cache_from_arg(cache_root);
    let source = PatchCdnSource::poe1(patch_version);
    let snapshot = source
        .snapshot_index(&cache, cache_mode(offline))
        .context("fetch patch CDN index")?;
    println!("{}", serde_json::to_string_pretty(&snapshot)?);
    Ok(())
}

fn inspect_index_header(
    patch_version: Option<String>,
    cache_root: Option<PathBuf>,
    offline: bool,
) -> anyhow::Result<()> {
    let patch_version = resolve_poe1_patch_version(patch_version)?;
    let cache = cache_from_arg(cache_root);
    let source = PatchCdnSource::poe1(patch_version);
    let fetch = source
        .fetch_index(&cache, cache_mode(offline))
        .context("fetch patch CDN index")?;
    let bytes =
        std::fs::read(&fetch.path).with_context(|| format!("read {}", fetch.path.display()))?;
    let header = parse_bundle_header(&bytes).context("parse bundle header")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "key": fetch.key,
            "path": fetch.path,
            "source": fetch.source,
            "byte_len": fetch.byte_len,
            "blake3": fetch.blake3,
            "header": {
                "decompressed_data_size": header.decompressed_data_size,
                "chunk_count": header.chunk_count,
                "compression_granularity": header.compression_granularity,
                "payload_offset": header.payload_offset,
                "compressed_payload_size": header.chunk_sizes.iter().map(|size| u64::from(*size)).sum::<u64>(),
            },
        }))?
    );
    Ok(())
}

struct InspectIndexOptions<'a> {
    patch_version: Option<String>,
    cache_root: Option<PathBuf>,
    offline: bool,
    logical_path: Option<&'a str>,
    prefix: Option<&'a str>,
    limit: usize,
    node: &'a Path,
    ooz_script: Option<&'a Path>,
}

fn inspect_index(options: InspectIndexOptions<'_>) -> anyhow::Result<()> {
    let patch_version = resolve_poe1_patch_version(options.patch_version)?;
    let cache = cache_from_arg(options.cache_root);
    let source = PatchCdnSource::poe1(patch_version);
    let fetch = source
        .fetch_index(&cache, cache_mode(options.offline))
        .context("fetch patch CDN index")?;

    let temp = tempfile::tempdir().context("create temporary index decompression directory")?;
    let decompressed_index_path = temp.path().join("index.bin");
    let script = options
        .ooz_script
        .map_or_else(default_ooz_script, Path::to_path_buf);
    run_ooz_bridge(OozBridgeInvocation {
        node: options.node,
        script: &script,
        input: &fetch.path,
        output: &decompressed_index_path,
        slice: None,
    })
    .context("decompress patch CDN index bundle")?;

    let index_bytes = std::fs::read(&decompressed_index_path)
        .with_context(|| format!("read {}", decompressed_index_path.display()))?;
    let index = parse_index_bundle(&index_bytes).context("parse decompressed index bundle")?;

    let path_reps_bundle_path = temp.path().join("path-reps.bundle.bin");
    let path_reps_path = temp.path().join("path-reps.bin");
    std::fs::write(&path_reps_bundle_path, &index.path_reps_bundle)
        .with_context(|| format!("write {}", path_reps_bundle_path.display()))?;
    run_ooz_bridge(OozBridgeInvocation {
        node: options.node,
        script: &script,
        input: &path_reps_bundle_path,
        output: &path_reps_path,
        slice: None,
    })
    .context("decompress nested path reps bundle")?;
    let path_reps = std::fs::read(&path_reps_path)
        .with_context(|| format!("read {}", path_reps_path.display()))?;
    let logical_paths = unpack_path_reps(&path_reps).context("unpack path reps")?;
    let root_dirs = root_directories(&logical_paths);
    let prefix_matches = options.prefix.map(|prefix| {
        logical_paths
            .iter()
            .filter(|path| path.starts_with(prefix))
            .take(options.limit)
            .cloned()
            .collect::<Vec<_>>()
    });

    let location = options
        .logical_path
        .map(|path| index.file_location(path).map(|location| (path, location)))
        .transpose()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "patch_version": source.patch_version,
            "release_line": source.release_line(),
            "cache_namespace": source.cache_namespace(),
            "index": {
                "key": fetch.key,
                "path": fetch.path,
                "source": fetch.source,
                "byte_len": fetch.byte_len,
                "blake3": fetch.blake3,
            },
            "summary": index.summary(),
            "path_reps": {
                "byte_len": path_reps.len(),
                "logical_path_count": logical_paths.len(),
                "root_directories": root_dirs,
            },
            "prefix_sample": prefix_matches.map(|paths| serde_json::json!({
                "prefix": options.prefix,
                "limit": options.limit,
                "paths": paths,
            })),
            "lookup": location.map(|(path, location)| serde_json::json!({
                "logical_path": path,
                "location": location,
            })),
        }))?
    );
    Ok(())
}

fn inspect_decompressed_index(input: &Path, logical_path: Option<&str>) -> anyhow::Result<()> {
    let bytes = std::fs::read(input).with_context(|| format!("read {}", input.display()))?;
    let index = parse_index_bundle(&bytes).context("parse decompressed index bundle")?;
    let location = logical_path
        .map(|path| index.file_location(path).map(|location| (path, location)))
        .transpose()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "input": input,
            "summary": index.summary(),
            "lookup": location.map(|(path, location)| serde_json::json!({
                "logical_path": path,
                "location": location,
            })),
        }))?
    );
    Ok(())
}

struct ExtractFileOptions<'a> {
    logical_path: &'a str,
    out: &'a Path,
    patch_version: Option<String>,
    cache_root: Option<PathBuf>,
    offline: bool,
    node: &'a Path,
    ooz_script: Option<&'a Path>,
}

fn extract_file(options: ExtractFileOptions<'_>) -> anyhow::Result<()> {
    let patch_version = resolve_poe1_patch_version(options.patch_version)?;
    let cache = cache_from_arg(options.cache_root);
    let source = PatchCdnSource::poe1(patch_version);
    let script = options
        .ooz_script
        .map_or_else(default_ooz_script, Path::to_path_buf);

    let temp = tempfile::tempdir().context("create temporary extraction directory")?;
    let decompressed_index_path = temp.path().join("index.bin");
    let index_fetch = source
        .fetch_index(&cache, cache_mode(options.offline))
        .context("fetch patch CDN index")?;
    run_ooz_bridge(OozBridgeInvocation {
        node: options.node,
        script: &script,
        input: &index_fetch.path,
        output: &decompressed_index_path,
        slice: None,
    })
    .context("decompress patch CDN index bundle")?;

    let index_bytes = std::fs::read(&decompressed_index_path)
        .with_context(|| format!("read {}", decompressed_index_path.display()))?;
    let index = parse_index_bundle(&index_bytes).context("parse decompressed index bundle")?;
    let location = index
        .file_location(options.logical_path)?
        .with_context(|| format!("logical path not found in index: {}", options.logical_path))?;

    let bundle_fetch = source
        .fetch_bundle(&cache, &location.bundle, cache_mode(options.offline))
        .with_context(|| format!("fetch bundle {}", location.bundle))?;
    if let Some(parent) = options.out.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    run_ooz_bridge(OozBridgeInvocation {
        node: options.node,
        script: &script,
        input: &bundle_fetch.path,
        output: options.out,
        slice: Some((
            usize::try_from(location.offset).context("file offset does not fit usize")?,
            usize::try_from(location.size).context("file size does not fit usize")?,
        )),
    })
    .with_context(|| format!("extract {}", options.logical_path))?;

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "logical_path": options.logical_path,
            "out": options.out,
            "patch_version": source.patch_version,
            "release_line": source.release_line(),
            "bundle": {
                "name": location.bundle,
                "offset": location.offset,
                "size": location.size,
                "cache_key": bundle_fetch.key,
                "path": bundle_fetch.path,
                "source": bundle_fetch.source,
            },
        }))?
    );
    Ok(())
}

fn default_ooz_script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("scripts")
        .join("ooz-decompress-bundle.mjs")
}

#[derive(Debug, Clone, Copy)]
struct OozBridgeInvocation<'a> {
    node: &'a Path,
    script: &'a Path,
    input: &'a Path,
    output: &'a Path,
    slice: Option<(usize, usize)>,
}

fn run_ooz_bridge(invocation: OozBridgeInvocation<'_>) -> anyhow::Result<()> {
    let mut command = ProcessCommand::new(invocation.node);
    command
        .arg(invocation.script)
        .arg(invocation.input)
        .arg(invocation.output);
    if let Some((offset, size)) = invocation.slice {
        command.arg(offset.to_string()).arg(size.to_string());
    }
    let output_status = command.output().with_context(|| {
        format!(
            "run {} {}",
            invocation.node.display(),
            invocation.script.display()
        )
    })?;
    if !output_status.status.success() {
        anyhow::bail!(
            "ooz bridge failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output_status.status,
            String::from_utf8_lossy(&output_status.stdout),
            String::from_utf8_lossy(&output_status.stderr)
        );
    }
    Ok(())
}

fn prefetch_bundles(
    patch_version: Option<String>,
    cache_root: Option<PathBuf>,
    mut bundles: Vec<String>,
    bundle_list: Option<PathBuf>,
    manifest_path: &Path,
    mode: NetworkMode,
) -> anyhow::Result<()> {
    let patch_version = resolve_poe1_patch_version(patch_version)?;
    if let Some(bundle_list) = bundle_list {
        let contents = std::fs::read_to_string(&bundle_list)
            .with_context(|| format!("read bundle list {}", bundle_list.display()))?;
        bundles.extend(
            contents
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(str::to_owned),
        );
    }
    if bundles.is_empty() {
        bundles.push("_.index.bin".to_owned());
    }
    bundles.sort();
    bundles.dedup();

    let cache = cache_from_arg(cache_root);
    let source = PatchCdnSource::poe1(patch_version);
    let manifest = source
        .prefetch_bundles(&cache, &bundles, mode.into())
        .context("prefetch patch CDN bundles")?;
    cache
        .write_manifest(manifest_path, &manifest)
        .with_context(|| format!("write manifest {}", manifest_path.display()))?;
    println!("{}", serde_json::to_string_pretty(&manifest)?);
    Ok(())
}

fn verify_cache(cache_root: Option<PathBuf>, manifest_path: &Path) -> anyhow::Result<()> {
    let cache = cache_from_arg(cache_root);
    let manifest = cache
        .read_manifest(manifest_path)
        .with_context(|| format!("read manifest {}", manifest_path.display()))?;
    let verification = cache.verify_manifest(&manifest)?;
    println!("{}", serde_json::to_string_pretty(&verification)?);
    if !verification.missing.is_empty() || !verification.mismatched.is_empty() {
        anyhow::bail!("cache verification failed");
    }
    Ok(())
}

fn clear_cache(
    cache_root: Option<PathBuf>,
    release_line: Option<String>,
    patch_version: Option<String>,
    dry_run: bool,
) -> anyhow::Result<()> {
    let cache = cache_from_arg(cache_root);
    let key = if let Some(patch_version) = patch_version {
        let source = PatchCdnSource::poe1(patch_version);
        format!(
            "{}/patches/{}",
            source.cache_namespace(),
            source.patch_version
        )
    } else {
        let release_line = match release_line {
            Some(release_line) => release_line,
            None => fetch_latest_patch_versions()
                .context("fetch latest PoE patch versions")?
                .poe1_source()
                .release_line(),
        };
        PatchCdnSource::release_cache_namespace(PoeGame::Poe1, &release_line)
    };
    let report = if dry_run {
        cache.preview_clear_key(&key)?
    } else {
        cache.clear_key(&key)?
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn update_dat_schema(
    url: &str,
    cache_root: Option<PathBuf>,
    offline: bool,
    out: &Path,
    manifest_path: &Path,
) -> anyhow::Result<()> {
    let required_types = REQUIRED_DAT_SCHEMA_TYPES
        .iter()
        .map(|type_name| (*type_name).to_owned())
        .collect::<Vec<_>>();
    let mode = if offline {
        CacheMode::Offline
    } else {
        CacheMode::Refresh
    };
    let manifest = DatSchemaClient::new(cache_from_arg(cache_root))
        .with_url(url)
        .update_snapshot(out, manifest_path, &required_types, mode)
        .context("update dat schema snapshot")?;
    println!("{}", serde_json::to_string_pretty(&manifest)?);
    Ok(())
}

fn inspect_dat_table(
    input: &Path,
    schema: &Path,
    table: Option<String>,
    columns: Vec<String>,
    limit: usize,
) -> anyhow::Result<()> {
    let table_name = table
        .or_else(|| table_name_from_path(input))
        .with_context(|| format!("derive table name from {}", input.display()))?;
    let bytes = std::fs::read(input).with_context(|| format!("read {}", input.display()))?;
    let schema =
        std::fs::read_to_string(schema).with_context(|| format!("read {}", schema.display()))?;
    let columns = if columns.is_empty() {
        default_columns_for_table(&table_name)
    } else {
        columns
    };
    let rows = read_dat_table(&bytes, &schema, &table_name, &columns, Some(limit))
        .with_context(|| format!("read dat table {table_name}"))?;
    println!("{}", serde_json::to_string_pretty(&rows)?);
    Ok(())
}

fn default_columns_for_table(table_name: &str) -> Vec<String> {
    match table_name {
        "WorldAreas" => [
            "Id",
            "Name",
            "Act",
            "IsTown",
            "AreaLevel",
            "TopologiesKeys",
            "TSIFile",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        "Topologies" => ["Id", "DGRFile"].into_iter().map(str::to_owned).collect(),
        _ => ["Id"].into_iter().map(str::to_owned).collect(),
    }
}

fn cache_mode(offline: bool) -> CacheMode {
    if offline {
        CacheMode::Offline
    } else {
        CacheMode::Online
    }
}

impl From<NetworkMode> for CacheMode {
    fn from(mode: NetworkMode) -> Self {
        match mode {
            NetworkMode::Online => Self::Online,
            NetworkMode::Offline => Self::Offline,
        }
    }
}
