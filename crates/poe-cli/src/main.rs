use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use poe_ggpk::{
    default_cache_root, fetch_latest_patch_versions, parse_bundle_header, parse_index_bundle,
    CacheMode, DiskCache, PatchCdnSource, PoeGame, SUPPORTED_POE1_RELEASE_LINE,
};

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
        /// Patch CDN version such as 3.29.3.3. Defaults to the live `PoE1` version endpoint.
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
        /// Patch CDN version such as 3.29.3.3. Defaults to the live `PoE1` version endpoint.
        #[arg(long)]
        patch_version: Option<String>,
        /// Cache root. Defaults to .poe-layouts/cache under the current directory.
        #[arg(long)]
        cache_root: Option<PathBuf>,
        /// Read from cache only and fail on cache miss.
        #[arg(long)]
        offline: bool,
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
    /// Prefetch named bundle files and write an offline cache manifest.
    PrefetchBundles {
        /// Patch CDN version such as 3.29.3.3. Defaults to the live `PoE1` version endpoint.
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
        /// Release line to clear. Defaults to the currently supported `PoE1` line.
        #[arg(long, default_value = SUPPORTED_POE1_RELEASE_LINE)]
        release_line: String,
        /// Clear only one exact patch version under its release line.
        #[arg(long)]
        patch_version: Option<String>,
        /// Print what would be removed without deleting it.
        #[arg(long)]
        dry_run: bool,
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
        Command::InspectDecompressedIndex {
            input,
            logical_path,
        } => inspect_decompressed_index(&input, logical_path.as_deref()),
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
        } => clear_cache(cache_root, &release_line, patch_version, dry_run),
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
                "supported": poe1.release_line() == SUPPORTED_POE1_RELEASE_LINE,
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
    let patch_version = match patch_version {
        Some(version) => version,
        None => {
            fetch_latest_patch_versions()
                .context("fetch latest PoE patch versions")?
                .poe
        }
    };
    ensure_supported_poe1_release(&PatchCdnSource::poe1(&patch_version))?;
    Ok(patch_version)
}

fn ensure_supported_poe1_release(source: &PatchCdnSource) -> anyhow::Result<()> {
    let release_line = source.release_line();
    if release_line != SUPPORTED_POE1_RELEASE_LINE {
        anyhow::bail!(
            "unsupported PoE1 release line {release_line}; this pipeline currently supports only {SUPPORTED_POE1_RELEASE_LINE}"
        );
    }
    Ok(())
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
    release_line: &str,
    patch_version: Option<String>,
    dry_run: bool,
) -> anyhow::Result<()> {
    if release_line != SUPPORTED_POE1_RELEASE_LINE {
        anyhow::bail!(
            "unsupported PoE1 release line {release_line}; this pipeline currently supports only {SUPPORTED_POE1_RELEASE_LINE}"
        );
    }
    let cache = cache_from_arg(cache_root);
    let key = if let Some(patch_version) = patch_version {
        let source = PatchCdnSource::poe1(patch_version);
        ensure_supported_poe1_release(&source)?;
        format!(
            "{}/patches/{}",
            source.cache_namespace(),
            source.patch_version
        )
    } else {
        PatchCdnSource::release_cache_namespace(PoeGame::Poe1, release_line)
    };
    let report = if dry_run {
        cache.preview_clear_key(&key)?
    } else {
        cache.clear_key(&key)?
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
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
