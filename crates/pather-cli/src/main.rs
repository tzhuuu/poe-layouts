use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use pather_core::{
    inspect_layout_database, scrape_campaign_acts_one_to_five as scrape_campaign_core,
    CampaignScrapeRequest,
};
use poe_content::{
    fetch_latest_patch_versions, parse_bundle_header, parse_index_bundle, root_directories,
    unpack_path_reps, BundleDecompressor, BundleSlice, BundleSliceOutput, CacheMode, DiskCache,
    PatchCdnSource, PatchClient, PoeGame,
};
use poe_dat::{table_name_from_path, DatSchemaClient, GraphqlDatSchema, DEFAULT_DAT_SCHEMA_URL};

const DEFAULT_DAT_SCHEMA_PATH: &str = "data/cache/dat-schema/_Core.gql";
const DEFAULT_DAT_SCHEMA_MANIFEST_PATH: &str = "data/cache/dat-schema/schema-manifest.json";
const REQUIRED_DAT_SCHEMA_TYPES: &[&str] = &["WorldAreas", "Topologies"];
const OOZ_BRIDGE_TIMEOUT: Duration = Duration::from_secs(120);

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
    /// Browse logical files and child folders in the patch CDN index.
    BrowseFiles {
        /// Logical folder prefix, for example Metadata/Terrain/Act1/Area1.
        #[arg(long, default_value = "")]
        prefix: String,
        /// Optional file extension filter, for example dgr or .dgr.
        #[arg(long)]
        extension: Option<String>,
        /// Return every matching descendant instead of only immediate children.
        #[arg(long)]
        recursive: bool,
        /// Maximum number of file paths to print.
        #[arg(long, default_value_t = 200)]
        limit: usize,
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
    /// Extract every logical file under a patch CDN folder prefix.
    ExtractFolder {
        /// Logical folder prefix, for example Metadata/Terrain/Act1/Area1.
        #[arg(long)]
        prefix: String,
        /// Optional file extension filter, for example dgr or .dgr.
        #[arg(long)]
        extension: Option<String>,
        /// Root output directory. Logical paths are preserved under this root.
        #[arg(long, default_value = ".poe-layouts/raw/files")]
        out_dir: PathBuf,
        /// Write a JSON manifest for the extraction.
        #[arg(long, default_value = ".poe-layouts/raw/extract-folder-manifest.json")]
        manifest: PathBuf,
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
        /// Read every schema column using generated names for anonymous fields.
        #[arg(long)]
        all_columns: bool,
        /// Maximum number of rows to print.
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Build the raw Acts 1-5 campaign layout scrape cache and manifest.
    #[command(name = "scrape-campaign-acts-1-5")]
    ScrapeCampaignActsOneToFive {
        /// Patch CDN version such as x.y.z.w. Defaults to the live `PoE1` version endpoint.
        #[arg(long)]
        patch_version: Option<String>,
        /// Cache root. Defaults to .poe-layouts/cache under the current directory.
        #[arg(long)]
        cache_root: Option<PathBuf>,
        /// Read from cache only and fail on cache miss.
        #[arg(long)]
        offline: bool,
        /// GraphQL dat schema snapshot.
        #[arg(long, default_value = DEFAULT_DAT_SCHEMA_PATH)]
        schema: PathBuf,
        /// Output directory for raw files and manifest.
        #[arg(long, default_value = ".poe-layouts/raw/campaign-acts-1-5")]
        out_dir: PathBuf,
        /// Output path for the app-facing `FlatBuffers` layout database.
        #[arg(long, default_value = "app/public/data/layouts.bin")]
        layout_db_out: PathBuf,
        /// Extra logical folder to extract into the raw corpus. Repeatable.
        #[arg(long = "raw-folder")]
        raw_folder_prefixes: Vec<String>,
        /// Node.js executable used for the temporary `ooz-wasm` bridge.
        #[arg(long, default_value = "node")]
        node: PathBuf,
        /// Bridge script used to decompress Oodle bundle files.
        #[arg(long)]
        ooz_script: Option<PathBuf>,
    },
    /// Read and summarize an app-facing `FlatBuffers` layout database.
    InspectLayoutDb {
        /// Input `layouts.bin` file.
        #[arg(long, default_value = "app/public/data/layouts.bin")]
        input: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum NetworkMode {
    Online,
    Offline,
}

struct ScrapeCampaignOptions<'a> {
    patch_version: Option<String>,
    cache_root: Option<PathBuf>,
    offline: bool,
    schema: &'a Path,
    out_dir: &'a Path,
    layout_db_out: &'a Path,
    raw_folder_prefixes: Vec<String>,
    node: &'a Path,
    ooz_script: Option<&'a Path>,
}

#[allow(clippy::too_many_lines)]
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
        Command::BrowseFiles {
            prefix,
            extension,
            recursive,
            limit,
            patch_version,
            cache_root,
            offline,
            node,
            ooz_script,
        } => browse_files(BrowseFilesOptions {
            prefix: &prefix,
            extension: extension.as_deref(),
            recursive,
            limit,
            patch_version,
            cache_root,
            offline,
            node: &node,
            ooz_script: ooz_script.as_deref(),
        }),
        Command::ExtractFolder {
            prefix,
            extension,
            out_dir,
            manifest,
            patch_version,
            cache_root,
            offline,
            node,
            ooz_script,
        } => {
            let out_dir = workspace_path(&out_dir);
            let manifest = workspace_path(&manifest);
            extract_folder(ExtractFolderOptions {
                prefix: &prefix,
                extension: extension.as_deref(),
                out_dir: &out_dir,
                manifest: &manifest,
                patch_version,
                cache_root,
                offline,
                node: &node,
                ooz_script: ooz_script.as_deref(),
            })
        }
        Command::PrefetchBundles {
            patch_version,
            cache_root,
            bundles,
            bundle_list,
            manifest,
            mode,
        } => {
            let manifest = workspace_path(&manifest);
            prefetch_bundles(
                patch_version,
                cache_root,
                bundles,
                bundle_list,
                &manifest,
                mode,
            )
        }
        Command::VerifyCache {
            cache_root,
            manifest,
        } => {
            let manifest = workspace_path(&manifest);
            verify_cache(cache_root, &manifest)
        }
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
        } => {
            let out = workspace_path(&out);
            let manifest = workspace_path(&manifest);
            update_dat_schema(&url, cache_root, offline, &out, &manifest)
        }
        Command::InspectDatTable {
            input,
            schema,
            table,
            columns,
            all_columns,
            limit,
        } => {
            let schema = workspace_path(&schema);
            inspect_dat_table(&input, &schema, table, columns, all_columns, limit)
        }
        Command::ScrapeCampaignActsOneToFive {
            patch_version,
            cache_root,
            offline,
            schema,
            out_dir,
            layout_db_out,
            raw_folder_prefixes,
            node,
            ooz_script,
        } => {
            let schema = workspace_path(&schema);
            let out_dir = workspace_path(&out_dir);
            let layout_db_out = workspace_path(&layout_db_out);
            scrape_campaign_acts_one_to_five(ScrapeCampaignOptions {
                patch_version,
                cache_root,
                offline,
                schema: &schema,
                out_dir: &out_dir,
                layout_db_out: &layout_db_out,
                raw_folder_prefixes,
                node: &node,
                ooz_script: ooz_script.as_deref(),
            })
        }
        Command::InspectLayoutDb { input } => {
            let input = workspace_path(&input);
            inspect_layout_db(&input)
        }
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
        || DiskCache::new(workspace_dir().join(".poe-layouts").join("cache")),
        DiskCache::new,
    )
}

fn resolve_poe1_patch_version(patch_version: Option<String>) -> anyhow::Result<String> {
    Ok(match patch_version {
        Some(patch_version) if !patch_version.trim().is_empty() => patch_version,
        _ => {
            fetch_latest_patch_versions()
                .context("fetch latest PoE patch versions")?
                .poe1_source()
                .patch_version
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
    run_ooz_bridge(
        options.node,
        &script,
        &fetch.path,
        &decompressed_index_path,
        None,
    )
    .map_err(anyhow::Error::msg)
    .context("decompress patch CDN index bundle")?;

    let index_bytes = std::fs::read(&decompressed_index_path)
        .with_context(|| format!("read {}", decompressed_index_path.display()))?;
    let index = parse_index_bundle(&index_bytes).context("parse decompressed index bundle")?;

    let path_reps_bundle_path = temp.path().join("path-reps.bundle.bin");
    let path_reps_path = temp.path().join("path-reps.bin");
    std::fs::write(&path_reps_bundle_path, &index.path_reps_bundle)
        .with_context(|| format!("write {}", path_reps_bundle_path.display()))?;
    run_ooz_bridge(
        options.node,
        &script,
        &path_reps_bundle_path,
        &path_reps_path,
        None,
    )
    .map_err(anyhow::Error::msg)
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

struct BrowseFilesOptions<'a> {
    prefix: &'a str,
    extension: Option<&'a str>,
    recursive: bool,
    limit: usize,
    patch_version: Option<String>,
    cache_root: Option<PathBuf>,
    offline: bool,
    node: &'a Path,
    ooz_script: Option<&'a Path>,
}

struct ExtractFolderOptions<'a> {
    prefix: &'a str,
    extension: Option<&'a str>,
    out_dir: &'a Path,
    manifest: &'a Path,
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
    let mut decompressor = NodeOozBridge {
        node: options.node,
        script: &script,
    };
    let client = PatchClient::new(source.clone(), cache, cache_mode(options.offline));
    let client_index = client
        .load_index(&mut decompressor)
        .context("load patch CDN index")?;
    let extracted = client
        .extract_logical_file(
            &client_index.index,
            options.logical_path,
            options.out,
            &mut decompressor,
        )
        .with_context(|| format!("extract {}", options.logical_path))?;

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "logical_path": options.logical_path,
            "out": options.out,
            "patch_version": source.patch_version,
            "release_line": source.release_line(),
            "bundle": {
                "name": extracted.location.bundle,
                "offset": extracted.location.offset,
                "size": extracted.location.size,
                "cache_key": extracted.cache_key,
                "path": extracted.cache_path,
                "source": extracted.cache_source,
            },
        }))?
    );
    Ok(())
}

fn browse_files(options: BrowseFilesOptions<'_>) -> anyhow::Result<()> {
    let script = options
        .ooz_script
        .map_or_else(default_ooz_script, Path::to_path_buf);
    let patch_version = resolve_poe1_patch_version(options.patch_version)?;
    let cache = cache_from_arg(options.cache_root);
    let source = PatchCdnSource::poe1(patch_version);
    let mut decompressor = NodeOozBridge {
        node: options.node,
        script: &script,
    };
    let client = PatchClient::new(source.clone(), cache, cache_mode(options.offline));
    let client_index = client
        .load_index(&mut decompressor)
        .context("load patch CDN index")?;
    let view = browse_logical_paths(
        &client_index.logical_paths,
        options.prefix,
        options.extension,
        options.recursive,
        options.limit,
    );

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "patch_version": source.patch_version,
            "release_line": source.release_line(),
            "prefix": normalize_logical_prefix(options.prefix),
            "extension": options.extension,
            "recursive": options.recursive,
            "total_files": view.total_files,
            "returned_files": view.files.len(),
            "folders": view.folders,
            "files": view.files,
        }))?
    );
    Ok(())
}

fn extract_folder(options: ExtractFolderOptions<'_>) -> anyhow::Result<()> {
    let script = options
        .ooz_script
        .map_or_else(default_ooz_script, Path::to_path_buf);
    let patch_version = resolve_poe1_patch_version(options.patch_version)?;
    let cache = cache_from_arg(options.cache_root);
    let source = PatchCdnSource::poe1(patch_version);
    let mut decompressor = NodeOozBridge {
        node: options.node,
        script: &script,
    };
    let client = PatchClient::new(source.clone(), cache, cache_mode(options.offline));
    let client_index = client
        .load_index(&mut decompressor)
        .context("load patch CDN index")?;
    let mut paths = recursive_logical_path_matches(
        &client_index.logical_paths,
        options.prefix,
        options.extension,
    );
    paths.sort();

    let mut extracted = Vec::new();
    let mut failed = Vec::new();
    for logical_path in &paths {
        let output_path = logical_output_path(options.out_dir, logical_path);
        match client.extract_logical_file(
            &client_index.index,
            logical_path,
            &output_path,
            &mut decompressor,
        ) {
            Ok(file) => extracted.push(file),
            Err(error) => failed.push(ExtractFolderFailure {
                logical_path: logical_path.clone(),
                error: error.to_string(),
            }),
        }
    }

    let manifest = ExtractFolderManifest {
        patch_version: source.patch_version.clone(),
        release_line: source.release_line(),
        prefix: normalize_logical_prefix(options.prefix),
        extension: options.extension.map(str::to_owned),
        out_dir: options.out_dir.to_path_buf(),
        matched_files: paths.len(),
        extracted,
        failed,
    };
    write_json(options.manifest, &manifest)?;
    println!("{}", serde_json::to_string_pretty(&manifest)?);
    if !manifest.failed.is_empty() {
        anyhow::bail!("failed to extract {} files", manifest.failed.len());
    }
    Ok(())
}

#[derive(Debug, serde::Serialize)]
struct BrowseFilesView {
    total_files: usize,
    folders: Vec<String>,
    files: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
struct ExtractFolderManifest {
    patch_version: String,
    release_line: String,
    prefix: String,
    extension: Option<String>,
    out_dir: PathBuf,
    matched_files: usize,
    extracted: Vec<poe_content::ExtractedLogicalFile>,
    failed: Vec<ExtractFolderFailure>,
}

#[derive(Debug, serde::Serialize)]
struct ExtractFolderFailure {
    logical_path: String,
    error: String,
}

fn browse_logical_paths(
    logical_paths: &[String],
    prefix: &str,
    extension: Option<&str>,
    recursive: bool,
    limit: usize,
) -> BrowseFilesView {
    let mut folders = BTreeSet::new();
    let mut files = Vec::new();
    let mut total_files = 0;

    for logical_path in logical_paths {
        let Some(remainder) = path_under_prefix(logical_path, prefix) else {
            continue;
        };
        if remainder.is_empty() {
            continue;
        }
        if recursive {
            if extension_matches(logical_path, extension) {
                total_files += 1;
                if files.len() < limit {
                    files.push(logical_path.clone());
                }
            }
            continue;
        }
        if let Some((folder, _)) = remainder.split_once('/') {
            folders.insert(folder.to_owned());
        } else if extension_matches(logical_path, extension) {
            total_files += 1;
            if files.len() < limit {
                files.push(logical_path.clone());
            }
        }
    }

    files.sort();
    BrowseFilesView {
        total_files,
        folders: folders.into_iter().collect(),
        files,
    }
}

fn recursive_logical_path_matches(
    logical_paths: &[String],
    prefix: &str,
    extension: Option<&str>,
) -> Vec<String> {
    logical_paths
        .iter()
        .filter(|path| {
            path_under_prefix(path, prefix).is_some_and(|remainder| !remainder.is_empty())
                && extension_matches(path, extension)
        })
        .cloned()
        .collect()
}

fn path_under_prefix<'a>(logical_path: &'a str, prefix: &str) -> Option<&'a str> {
    let prefix = normalize_logical_prefix(prefix);
    if prefix.is_empty() {
        return Some(logical_path.trim_start_matches('/'));
    }

    let path = logical_path.trim_matches('/');
    let path_lower = path.to_ascii_lowercase();
    let prefix_lower = prefix.to_ascii_lowercase();
    if path_lower == prefix_lower {
        return Some("");
    }

    let folder_prefix = format!("{prefix_lower}/");
    if path_lower.starts_with(&folder_prefix) {
        return path.get(prefix.len() + 1..);
    }
    None
}

fn extension_matches(logical_path: &str, extension: Option<&str>) -> bool {
    let Some(extension) = extension else {
        return true;
    };
    let extension = extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    if extension.is_empty() {
        return true;
    }
    logical_path
        .rsplit_once('.')
        .is_some_and(|(_, path_extension)| path_extension.eq_ignore_ascii_case(&extension))
}

fn normalize_logical_prefix(prefix: &str) -> String {
    prefix
        .replace('\\', "/")
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/")
}

fn logical_output_path(root: &Path, logical_path: &str) -> PathBuf {
    let mut output = root.to_path_buf();
    for part in logical_path.replace('\\', "/").split('/') {
        if part.is_empty() || part == "." || part == ".." {
            continue;
        }
        output.push(part);
    }
    output
}

fn write_json<T>(path: &Path, value: &T) -> anyhow::Result<()>
where
    T: serde::Serialize,
{
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(value).context("encode json")?;
    std::fs::write(path, bytes).with_context(|| format!("write {}", path.display()))
}

fn prefetch_bundles(
    patch_version: Option<String>,
    cache_root: Option<PathBuf>,
    mut bundles: Vec<String>,
    bundle_list: Option<PathBuf>,
    manifest_path: &Path,
    mode: NetworkMode,
) -> anyhow::Result<()> {
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

    let patch_version = resolve_poe1_patch_version(patch_version)?;
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
    let key = release_cache_key(release_line, patch_version)?;
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
    all_columns: bool,
    limit: usize,
) -> anyhow::Result<()> {
    let table_name = table
        .or_else(|| table_name_from_path(input))
        .with_context(|| format!("derive table name from {}", input.display()))?;
    let bytes = std::fs::read(input).with_context(|| format!("read {}", input.display()))?;
    let schema =
        std::fs::read_to_string(schema).with_context(|| format!("read {}", schema.display()))?;
    let reader = GraphqlDatSchema::parse(&schema).context("parse dat schema")?;
    let columns = if all_columns {
        if !columns.is_empty() {
            anyhow::bail!("--all-columns cannot be combined with --column");
        }
        Vec::new()
    } else if columns.is_empty() {
        default_columns_for_table(&table_name)
    } else {
        columns
    };
    let rows = reader
        .read_table(&bytes, &table_name, &columns, Some(limit))
        .with_context(|| format!("read dat table {table_name}"))?;
    println!("{}", serde_json::to_string_pretty(&rows)?);
    Ok(())
}

fn scrape_campaign_acts_one_to_five(options: ScrapeCampaignOptions<'_>) -> anyhow::Result<()> {
    let script = options
        .ooz_script
        .map_or_else(default_ooz_script, Path::to_path_buf);
    let patch_version = resolve_poe1_patch_version(options.patch_version)?;
    let source = PatchCdnSource::poe1(patch_version);
    let mut decompressor = NodeOozBridge {
        node: options.node,
        script: &script,
    };
    let output = scrape_campaign_core(
        &CampaignScrapeRequest {
            source,
            cache: cache_from_arg(options.cache_root),
            mode: cache_mode(options.offline),
            schema_path: options.schema.to_path_buf(),
            out_dir: options.out_dir.to_path_buf(),
            layout_db_out: Some(options.layout_db_out.to_path_buf()),
            raw_folder_prefixes: options.raw_folder_prefixes,
        },
        &mut decompressor,
    )?;
    let manifest = output.manifest;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "scope": manifest.scope,
            "patch_version": manifest.patch_version,
            "release_line": manifest.release_line,
            "manifest_path": output.manifest_path,
            "layout_db_path": output.layout_db_path,
            "counts": {
                "selected_areas": manifest.selected_areas.len(),
                "candidate_files": manifest.candidate_files.len(),
                "terrain_folders": manifest.terrain_folders.len(),
                "extracted_files": manifest.extracted_files.len(),
                "folder_files": manifest.folder_files.len(),
                "missing_files": manifest.missing_files.len(),
            },
            "warnings": manifest.warnings,
        }))?
    );
    Ok(())
}

fn inspect_layout_db(input: &Path) -> anyhow::Result<()> {
    let summary = inspect_layout_database(input)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "input": input,
            "schema_version": summary.schema_version,
            "game_version": summary.game_version,
            "release_line": summary.release_line,
            "scope": summary.scope,
            "counts": {
                "zones": summary.counts.zones,
                "terrain_files": summary.counts.terrain_files,
                "source_files": summary.counts.source_files,
                "warnings": summary.counts.warnings,
            },
            "sample_zones": summary.sample_zones,
        }))?
    );
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

fn release_cache_key(
    release_line: Option<String>,
    patch_version: Option<String>,
) -> anyhow::Result<String> {
    if let Some(patch_version) = patch_version.filter(|value| !value.trim().is_empty()) {
        let source = PatchCdnSource::poe1(patch_version);
        return Ok(format!(
            "{}/patches/{}",
            source.cache_namespace(),
            source.patch_version
        ));
    }

    let release_line = match release_line {
        Some(release_line) if !release_line.trim().is_empty() => release_line,
        _ => fetch_latest_patch_versions()
            .context("fetch latest PoE patch versions")?
            .poe1_source()
            .release_line(),
    };
    Ok(PatchCdnSource::release_cache_namespace(
        PoeGame::Poe1,
        release_line.trim(),
    ))
}

fn workspace_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cwd.ends_with("app") {
        return cwd.parent().map_or(cwd.clone(), PathBuf::from);
    }
    cwd
}

fn workspace_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_dir().join(path)
    }
}

fn default_ooz_script() -> PathBuf {
    workspace_dir()
        .join("scripts")
        .join("ooz-decompress-bundle.mjs")
}

#[derive(Debug, Clone, Copy)]
struct NodeOozBridge<'a> {
    node: &'a Path,
    script: &'a Path,
}

#[derive(Debug)]
struct NodeOozBridgeError(String);

impl std::fmt::Display for NodeOozBridgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for NodeOozBridgeError {}

impl BundleDecompressor for NodeOozBridge<'_> {
    type Error = NodeOozBridgeError;

    fn decompress_bundle(
        &mut self,
        input: &Path,
        output: &Path,
        slice: Option<BundleSlice>,
    ) -> Result<(), Self::Error> {
        run_ooz_bridge(self.node, self.script, input, output, slice).map_err(NodeOozBridgeError)
    }

    fn decompress_bundle_slices(
        &mut self,
        input: &Path,
        slices: &[BundleSliceOutput],
    ) -> Result<(), Self::Error> {
        run_ooz_bridge_batch(self.node, self.script, input, slices).map_err(NodeOozBridgeError)
    }
}

fn run_ooz_bridge_batch(
    node: &Path,
    script: &Path,
    input: &Path,
    slices: &[BundleSliceOutput],
) -> Result<(), String> {
    let mut manifest = tempfile::NamedTempFile::new()
        .map_err(|source| format!("create temporary ooz batch manifest: {source}"))?;
    serde_json::to_writer(
        &mut manifest,
        &serde_json::json!({
            "slices": slices.iter().map(|slice| serde_json::json!({
                "offset": slice.slice.offset,
                "size": slice.slice.size,
                "outputPath": slice.output_path,
            })).collect::<Vec<_>>(),
        }),
    )
    .map_err(|source| format!("write temporary ooz batch manifest: {source}"))?;
    manifest
        .flush()
        .map_err(|source| format!("flush temporary ooz batch manifest: {source}"))?;
    let mut command = ProcessCommand::new(node);
    command
        .arg(script)
        .arg("--batch")
        .arg(input)
        .arg(manifest.path());
    run_ooz_command(node, script, command)
}

fn run_ooz_bridge(
    node: &Path,
    script: &Path,
    input: &Path,
    output: &Path,
    slice: Option<BundleSlice>,
) -> Result<(), String> {
    let mut command = ProcessCommand::new(node);
    command.arg(script).arg(input).arg(output);
    if let Some(slice) = slice {
        command
            .arg(slice.offset.to_string())
            .arg(slice.size.to_string());
    }
    run_ooz_command(node, script, command)
}

fn run_ooz_command(node: &Path, script: &Path, mut command: ProcessCommand) -> Result<(), String> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|source| format!("run {} {}: {source}", node.display(), script.display()))?;
    let started = Instant::now();
    loop {
        let status = child.try_wait().map_err(|source| {
            format!("wait for {} {}: {source}", node.display(), script.display())
        })?;
        if status.is_some() {
            break;
        }
        if started.elapsed() > OOZ_BRIDGE_TIMEOUT {
            let _ = child.kill();
            let output = child.wait_with_output().map_err(|source| {
                format!(
                    "collect timed-out {} {} output: {source}",
                    node.display(),
                    script.display()
                )
            })?;
            return Err(format!(
                "timed out after {}s\nstdout:\n{}\nstderr:\n{}",
                OOZ_BRIDGE_TIMEOUT.as_secs(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }
    let output_status = child.wait_with_output().map_err(|source| {
        format!(
            "collect {} {} output: {source}",
            node.display(),
            script.display()
        )
    })?;
    if !output_status.status.success() {
        return Err(format!(
            "status {}\nstdout:\n{}\nstderr:\n{}",
            output_status.status,
            String::from_utf8_lossy(&output_status.stdout),
            String::from_utf8_lossy(&output_status.stderr)
        ));
    }
    Ok(())
}

impl From<NetworkMode> for CacheMode {
    fn from(mode: NetworkMode) -> Self {
        match mode {
            NetworkMode::Online => Self::Online,
            NetworkMode::Offline => Self::Offline,
        }
    }
}
