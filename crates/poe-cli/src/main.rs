use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use poe_ggpk::{
    default_cache_root, fetch_latest_patch_versions, parse_bundle_header, parse_index_bundle,
    root_directories, table_name_from_path, unpack_path_reps, CacheMode, DatSchemaClient, DatValue,
    DiskCache, ExtractedLogicalFile, GraphqlDatRows, GraphqlDatSchema, PatchCdnSource, PatchClient,
    PoeGame, DEFAULT_DAT_SCHEMA_URL,
};
use serde::Serialize;

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
        /// Node.js executable used for the temporary `ooz-wasm` bridge.
        #[arg(long, default_value = "node")]
        node: PathBuf,
        /// Bridge script used to decompress Oodle bundle files.
        #[arg(long)]
        ooz_script: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum NetworkMode {
    Online,
    Offline,
}

#[derive(Debug, Serialize)]
struct CampaignScrapeManifest {
    scope: String,
    patch_version: String,
    release_line: String,
    schema: PathBuf,
    out_dir: PathBuf,
    tables: Vec<ExtractedLogicalFile>,
    selected_areas: Vec<CampaignAreaSummary>,
    candidate_files: Vec<TerrainCandidate>,
    extracted_files: Vec<ExtractedLogicalFile>,
    missing_files: Vec<MissingTerrainCandidate>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct CampaignAreaSummary {
    row_index: usize,
    id: String,
    name: String,
    act: i64,
    is_town: bool,
    area_level: i64,
    topology_indices: Vec<usize>,
    tsi_file: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct TopologySummary {
    row_index: usize,
    id: String,
    graph_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct TerrainCandidate {
    logical_path: String,
    source: String,
    kind: TerrainCandidateKind,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum TerrainCandidateKind {
    Graph,
    Tsi,
    DgrVariant,
    ArmVariant,
}

#[derive(Debug, Clone, Serialize)]
struct MissingTerrainCandidate {
    logical_path: String,
    source: String,
    kind: TerrainCandidateKind,
    reason: String,
}

struct ScrapeCampaignOptions<'a> {
    patch_version: Option<String>,
    cache_root: Option<PathBuf>,
    offline: bool,
    schema: &'a Path,
    out_dir: &'a Path,
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
            all_columns,
            limit,
        } => inspect_dat_table(&input, &schema, table, columns, all_columns, limit),
        Command::ScrapeCampaignActsOneToFive {
            patch_version,
            cache_root,
            offline,
            schema,
            out_dir,
            node,
            ooz_script,
        } => scrape_campaign_acts_one_to_five(ScrapeCampaignOptions {
            patch_version,
            cache_root,
            offline,
            schema: &schema,
            out_dir: &out_dir,
            node: &node,
            ooz_script: ooz_script.as_deref(),
        }),
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

#[derive(Debug, Clone, Copy)]
struct NodeOozBridge<'a> {
    node: &'a Path,
    script: &'a Path,
}

#[derive(Debug)]
struct NodeOozBridgeError(anyhow::Error);

impl std::fmt::Display for NodeOozBridgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl std::error::Error for NodeOozBridgeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl poe_ggpk::BundleDecompressor for NodeOozBridge<'_> {
    type Error = NodeOozBridgeError;

    fn decompress_bundle(
        &mut self,
        input: &Path,
        output: &Path,
        slice: Option<poe_ggpk::BundleSlice>,
    ) -> Result<(), Self::Error> {
        run_ooz_bridge(OozBridgeInvocation {
            node: self.node,
            script: self.script,
            input,
            output,
            slice: slice.map(|slice| (slice.offset, slice.size)),
        })
        .map_err(NodeOozBridgeError)
    }
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

#[allow(clippy::too_many_lines)]
fn scrape_campaign_acts_one_to_five(options: ScrapeCampaignOptions<'_>) -> anyhow::Result<()> {
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
    let logical_paths = client_index
        .logical_paths
        .iter()
        .cloned()
        .collect::<HashSet<_>>();

    let files_dir = options.out_dir.join("files");
    std::fs::create_dir_all(&files_dir)
        .with_context(|| format!("create {}", files_dir.display()))?;

    let table_paths = ["data/worldareas.datc64", "data/topologies.datc64"];
    let mut extracted_tables = Vec::new();
    for logical_path in table_paths {
        let output_path = raw_output_path(&files_dir, logical_path);
        let extracted = client
            .extract_logical_file(
                &client_index.index,
                logical_path,
                &output_path,
                &mut decompressor,
            )
            .with_context(|| format!("extract table {logical_path}"))?;
        extracted_tables.push(extracted);
    }

    let schema_text = std::fs::read_to_string(options.schema)
        .with_context(|| format!("read {}", options.schema.display()))?;
    let schema = GraphqlDatSchema::parse(&schema_text).context("parse dat schema")?;
    let world_areas_bytes = std::fs::read(raw_output_path(&files_dir, "data/worldareas.datc64"))
        .context("read extracted WorldAreas")?;
    let topologies_bytes = std::fs::read(raw_output_path(&files_dir, "data/topologies.datc64"))
        .context("read extracted Topologies")?;

    let world_areas = schema
        .read_table(
            &world_areas_bytes,
            "WorldAreas",
            &[
                "Id".to_owned(),
                "Name".to_owned(),
                "Act".to_owned(),
                "IsTown".to_owned(),
                "AreaLevel".to_owned(),
                "IsMapArea".to_owned(),
                "TopologiesKeys".to_owned(),
                "TSIFile".to_owned(),
            ],
            None,
        )
        .context("read WorldAreas")?;
    let topologies = schema
        .read_table(
            &topologies_bytes,
            "Topologies",
            &["Id".to_owned(), "DGRFile".to_owned()],
            None,
        )
        .context("read Topologies")?;

    let topology_by_index = topology_summaries(&topologies)?;
    let selected_areas = campaign_area_summaries(&world_areas)?;
    let mut candidates = terrain_candidates(&selected_areas, &topology_by_index);
    candidates.sort();
    candidates.dedup();

    let mut extracted_files = Vec::new();
    let mut missing_files = Vec::new();
    for candidate in &candidates {
        if logical_paths.contains(&candidate.logical_path) {
            let output_path = raw_output_path(&files_dir, &candidate.logical_path);
            match client.extract_logical_file(
                &client_index.index,
                &candidate.logical_path,
                &output_path,
                &mut decompressor,
            ) {
                Ok(extracted) => extracted_files.push(extracted),
                Err(error) => missing_files.push(MissingTerrainCandidate {
                    logical_path: candidate.logical_path.clone(),
                    source: candidate.source.clone(),
                    kind: candidate.kind,
                    reason: error.to_string(),
                }),
            }
        } else {
            missing_files.push(MissingTerrainCandidate {
                logical_path: candidate.logical_path.clone(),
                source: candidate.source.clone(),
                kind: candidate.kind,
                reason: "not found in patch index".to_owned(),
            });
        }
    }

    let manifest = CampaignScrapeManifest {
        scope: "campaign-acts-1-5".to_owned(),
        patch_version: source.patch_version.clone(),
        release_line: source.release_line(),
        schema: options.schema.to_path_buf(),
        out_dir: options.out_dir.to_path_buf(),
        tables: extracted_tables,
        selected_areas,
        candidate_files: candidates,
        extracted_files,
        missing_files,
        warnings: vec![
            "This milestone only resolves table-declared graph and TSI paths plus simple .dgr/.arm path variants; deeper terrain dependencies require parsing the extracted graph files.".to_owned(),
            "Campaign selection currently uses WorldAreas Act 1-5, excludes map areas, and skips the NULL sentinel row.".to_owned(),
        ],
    };

    let manifest_path = options.out_dir.join("manifest.json");
    std::fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)
        .with_context(|| format!("write {}", manifest_path.display()))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "scope": manifest.scope,
            "patch_version": manifest.patch_version,
            "release_line": manifest.release_line,
            "manifest_path": manifest_path,
            "counts": {
                "tables": manifest.tables.len(),
                "selected_areas": manifest.selected_areas.len(),
                "candidate_files": manifest.candidate_files.len(),
                "extracted_files": manifest.extracted_files.len(),
                "missing_files": manifest.missing_files.len(),
            },
            "warnings": manifest.warnings,
        }))?
    );
    Ok(())
}

fn topology_summaries(rows: &GraphqlDatRows) -> anyhow::Result<HashMap<usize, TopologySummary>> {
    rows.rows
        .iter()
        .map(|row| {
            let row_index = row_unsigned(row, "_index")
                .context("topology row missing _index")?
                .try_into()
                .context("topology row index does not fit usize")?;
            Ok((
                row_index,
                TopologySummary {
                    row_index,
                    id: row_string(row, "Id").unwrap_or_default(),
                    graph_file: non_empty_string(row, "DGRFile"),
                },
            ))
        })
        .collect()
}

fn campaign_area_summaries(rows: &GraphqlDatRows) -> anyhow::Result<Vec<CampaignAreaSummary>> {
    let mut areas = Vec::new();
    for row in &rows.rows {
        let id = row_string(row, "Id").unwrap_or_default();
        if id == "NULL" || id.is_empty() {
            continue;
        }
        let act = row_integer(row, "Act").unwrap_or_default();
        if !(1..=5).contains(&act) {
            continue;
        }
        if !id.starts_with(&format!("1_{act}")) {
            continue;
        }
        if row_bool(row, "IsMapArea").unwrap_or(false) {
            continue;
        }
        let row_index = row_unsigned(row, "_index")
            .context("WorldAreas row missing _index")?
            .try_into()
            .context("WorldAreas row index does not fit usize")?;
        areas.push(CampaignAreaSummary {
            row_index,
            id,
            name: row_string(row, "Name").unwrap_or_default(),
            act,
            is_town: row_bool(row, "IsTown").unwrap_or(false),
            area_level: row_integer(row, "AreaLevel").unwrap_or_default(),
            topology_indices: row_array_unsigned(row, "TopologiesKeys"),
            tsi_file: non_empty_string(row, "TSIFile").map(normalize_logical_path),
        });
    }
    Ok(areas)
}

fn terrain_candidates(
    areas: &[CampaignAreaSummary],
    topology_by_index: &HashMap<usize, TopologySummary>,
) -> Vec<TerrainCandidate> {
    let mut candidates = Vec::new();
    for area in areas {
        if let Some(tsi_file) = &area.tsi_file {
            candidates.push(TerrainCandidate {
                logical_path: tsi_file.clone(),
                source: format!("WorldAreas[{}].TSIFile {}", area.row_index, area.id),
                kind: TerrainCandidateKind::Tsi,
            });
        }
        for topology_index in &area.topology_indices {
            let Some(topology) = topology_by_index.get(topology_index) else {
                continue;
            };
            let Some(graph_file) = &topology.graph_file else {
                continue;
            };
            let normalized_graph = normalize_logical_path(graph_file);
            let source = format!(
                "WorldAreas[{}].TopologiesKeys -> Topologies[{}] {}",
                area.row_index, topology.row_index, topology.id
            );
            candidates.push(TerrainCandidate {
                logical_path: normalized_graph.clone(),
                source: source.clone(),
                kind: TerrainCandidateKind::Graph,
            });
            if let Some(dgr_path) =
                replace_extension(&normalized_graph, "dgr").filter(|path| path != &normalized_graph)
            {
                candidates.push(TerrainCandidate {
                    logical_path: dgr_path,
                    source: source.clone(),
                    kind: TerrainCandidateKind::DgrVariant,
                });
            }
            if let Some(arm_path) = replace_extension(&normalized_graph, "arm") {
                candidates.push(TerrainCandidate {
                    logical_path: arm_path,
                    source,
                    kind: TerrainCandidateKind::ArmVariant,
                });
            }
        }
    }
    candidates
}

fn raw_output_path(root: &Path, logical_path: &str) -> PathBuf {
    logical_path
        .split('/')
        .filter(|part| !part.is_empty() && *part != "." && *part != "..")
        .fold(root.to_path_buf(), |path, part| path.join(part))
}

fn normalize_logical_path(path: impl AsRef<str>) -> String {
    path.as_ref().replace('\\', "/").to_lowercase()
}

fn replace_extension(path: &str, extension: &str) -> Option<String> {
    let (base, _) = path.rsplit_once('.')?;
    Some(format!("{base}.{extension}"))
}

fn row_value<'a>(row: &'a poe_ggpk::DatRow, name: &str) -> Option<&'a DatValue> {
    row.0
        .iter()
        .find_map(|(key, value)| (key == name).then_some(value))
}

fn row_string(row: &poe_ggpk::DatRow, name: &str) -> Option<String> {
    match row_value(row, name)? {
        DatValue::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn non_empty_string(row: &poe_ggpk::DatRow, name: &str) -> Option<String> {
    row_string(row, name).filter(|value| !value.is_empty())
}

fn row_integer(row: &poe_ggpk::DatRow, name: &str) -> Option<i64> {
    match row_value(row, name)? {
        DatValue::Integer(value) => Some(*value),
        DatValue::Unsigned(value) => i64::try_from(*value).ok(),
        _ => None,
    }
}

fn row_unsigned(row: &poe_ggpk::DatRow, name: &str) -> Option<u64> {
    match row_value(row, name)? {
        DatValue::Unsigned(value) => Some(*value),
        DatValue::Integer(value) => u64::try_from(*value).ok(),
        _ => None,
    }
}

fn row_bool(row: &poe_ggpk::DatRow, name: &str) -> Option<bool> {
    match row_value(row, name)? {
        DatValue::Bool(value) => Some(*value),
        _ => None,
    }
}

fn row_array_unsigned(row: &poe_ggpk::DatRow, name: &str) -> Vec<usize> {
    match row_value(row, name) {
        Some(DatValue::Array(values)) => values
            .iter()
            .filter_map(|value| match value {
                DatValue::Unsigned(value) => usize::try_from(*value).ok(),
                DatValue::Integer(value) => usize::try_from(*value).ok(),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
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
