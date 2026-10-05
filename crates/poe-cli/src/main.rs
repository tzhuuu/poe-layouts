use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use poe_ggpk::{
    default_cache_root, fetch_latest_patch_versions, CacheMode, DiskCache, PatchCdnSource,
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

fn snapshot_index(
    patch_version: Option<String>,
    cache_root: Option<PathBuf>,
    offline: bool,
) -> anyhow::Result<()> {
    let patch_version = match patch_version {
        Some(version) => version,
        None => {
            fetch_latest_patch_versions()
                .context("fetch latest PoE patch versions")?
                .poe
        }
    };
    let cache = cache_root.map_or_else(
        || default_cache_root(&std::env::current_dir().expect("current dir")),
        DiskCache::new,
    );
    let source = PatchCdnSource::poe1(patch_version);
    let snapshot = source
        .snapshot_index(&cache, cache_mode(offline))
        .context("fetch patch CDN index")?;
    println!("{}", serde_json::to_string_pretty(&snapshot)?);
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
    let patch_version = match patch_version {
        Some(version) => version,
        None => {
            fetch_latest_patch_versions()
                .context("fetch latest PoE patch versions")?
                .poe
        }
    };
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

    let cache = cache_root.map_or_else(
        || default_cache_root(&std::env::current_dir().expect("current dir")),
        DiskCache::new,
    );
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
    let cache = cache_root.map_or_else(
        || default_cache_root(&std::env::current_dir().expect("current dir")),
        DiskCache::new,
    );
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
