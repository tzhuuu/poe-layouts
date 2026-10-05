use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use poe_ggpk::{
    default_cache_root, fetch_latest_patch_versions, BundleDecompressor, BundleSlice,
    CacheClearReport, CacheManifest, CacheMode, DiskCache, LatestPatchVersions, PatchCdnSource,
    PoeGame,
};
use poe_layouts_core::{
    inspect_layout_database, scrape_campaign_acts_one_to_five, CampaignScrapeRequest,
    LayoutDbSummary,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrefetchBundlesRequest {
    patch_version: Option<String>,
    bundles: Vec<String>,
    refresh: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClearCacheRequest {
    release_line: Option<String>,
    patch_version: Option<String>,
    dry_run: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScrapeCampaignRequest {
    patch_version: Option<String>,
    refresh: bool,
    offline: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScrapeCampaignSummary {
    scope: String,
    patch_version: String,
    release_line: String,
    manifest_path: PathBuf,
    layout_db_path: Option<PathBuf>,
    selected_areas: usize,
    candidate_files: usize,
    extracted_files: usize,
    missing_files: usize,
    warnings: Vec<String>,
}

#[tauri::command]
fn inspect_layout_db(input: Option<PathBuf>) -> Result<LayoutDbSummary, String> {
    let input = input.unwrap_or_else(|| PathBuf::from("app/public/data/layouts.bin"));
    inspect_layout_database(&input).map_err(|error| error.to_string())
}

#[tauri::command]
fn latest_patch_versions() -> Result<LatestPatchVersions, String> {
    fetch_latest_patch_versions().map_err(|error| error.to_string())
}

#[tauri::command]
fn prefetch_bundles(request: PrefetchBundlesRequest) -> Result<CacheManifest, String> {
    let patch_version = resolve_poe1_patch_version(request.patch_version)?;
    let mut bundles = request
        .bundles
        .into_iter()
        .map(|bundle| bundle.trim().to_owned())
        .filter(|bundle| !bundle.is_empty())
        .collect::<Vec<_>>();
    if bundles.is_empty() {
        bundles.push("_.index.bin".to_owned());
    }
    bundles.sort();
    bundles.dedup();

    let source = PatchCdnSource::poe1(patch_version);
    source
        .prefetch_bundles(
            &default_cache(),
            &bundles,
            if request.refresh {
                CacheMode::Refresh
            } else {
                CacheMode::Online
            },
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn clear_cache(request: ClearCacheRequest) -> Result<CacheClearReport, String> {
    let key = if let Some(patch_version) = request.patch_version {
        if patch_version.trim().is_empty() {
            release_cache_key(request.release_line)?
        } else {
            let source = PatchCdnSource::poe1(patch_version);
            format!(
                "{}/patches/{}",
                source.cache_namespace(),
                source.patch_version
            )
        }
    } else {
        release_cache_key(request.release_line)?
    };

    let cache = default_cache();
    if request.dry_run {
        cache
            .preview_clear_key(&key)
            .map_err(|error| error.to_string())
    } else {
        cache.clear_key(&key).map_err(|error| error.to_string())
    }
}

#[tauri::command]
fn scrape_campaign_acts_1_5(
    request: ScrapeCampaignRequest,
) -> Result<ScrapeCampaignSummary, String> {
    let workspace = workspace_dir();
    let source = PatchCdnSource::poe1(resolve_poe1_patch_version(request.patch_version)?);
    let mut decompressor = NodeOozBridge {
        node: PathBuf::from("node"),
        script: workspace.join("scripts/ooz-decompress-bundle.mjs"),
    };
    let output = scrape_campaign_acts_one_to_five(
        &CampaignScrapeRequest {
            source,
            cache: default_cache_root(&workspace),
            mode: scrape_cache_mode(request.offline, request.refresh),
            schema_path: workspace.join("schema/dat/_Core.gql"),
            out_dir: workspace.join(".poe-layouts/raw/campaign-acts-1-5"),
            layout_db_out: Some(workspace.join("app/public/data/layouts.bin")),
        },
        &mut decompressor,
    )
    .map_err(|error| error.to_string())?;
    let manifest = output.manifest;
    Ok(ScrapeCampaignSummary {
        scope: manifest.scope,
        patch_version: manifest.patch_version,
        release_line: manifest.release_line,
        manifest_path: output.manifest_path,
        layout_db_path: output.layout_db_path,
        selected_areas: manifest.selected_areas.len(),
        candidate_files: manifest.candidate_files.len(),
        extracted_files: manifest.extracted_files.len(),
        missing_files: manifest.missing_files.len(),
        warnings: manifest.warnings,
    })
}

fn scrape_cache_mode(offline: bool, refresh: bool) -> CacheMode {
    if offline {
        CacheMode::Offline
    } else if refresh {
        CacheMode::Refresh
    } else {
        CacheMode::Online
    }
}

fn release_cache_key(release_line: Option<String>) -> Result<String, String> {
    let release_line = match release_line {
        Some(release_line) if !release_line.trim().is_empty() => release_line,
        _ => fetch_latest_patch_versions()
            .map_err(|error| error.to_string())?
            .poe1_source()
            .release_line(),
    };
    Ok(PatchCdnSource::release_cache_namespace(
        PoeGame::Poe1,
        release_line.trim(),
    ))
}

fn resolve_poe1_patch_version(patch_version: Option<String>) -> Result<String, String> {
    match patch_version {
        Some(patch_version) if !patch_version.trim().is_empty() => Ok(patch_version),
        _ => Ok(fetch_latest_patch_versions()
            .map_err(|error| error.to_string())?
            .poe1_source()
            .patch_version),
    }
}

fn default_cache() -> DiskCache {
    default_cache_root(&workspace_dir())
}

fn workspace_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cwd.ends_with("app/src-tauri") {
        return cwd
            .parent()
            .and_then(|app_dir| app_dir.parent())
            .map_or(cwd.clone(), PathBuf::from);
    }
    if cwd.ends_with("app") {
        return cwd.parent().map_or(cwd.clone(), PathBuf::from);
    }
    cwd
}

#[derive(Debug, Clone)]
struct NodeOozBridge {
    node: PathBuf,
    script: PathBuf,
}

#[derive(Debug)]
struct NodeOozBridgeError(String);

impl std::fmt::Display for NodeOozBridgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for NodeOozBridgeError {}

impl BundleDecompressor for NodeOozBridge {
    type Error = NodeOozBridgeError;

    fn decompress_bundle(
        &mut self,
        input: &Path,
        output: &Path,
        slice: Option<BundleSlice>,
    ) -> Result<(), Self::Error> {
        run_ooz_bridge(&self.node, &self.script, input, output, slice).map_err(NodeOozBridgeError)
    }
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
    let output_status = command
        .output()
        .map_err(|source| format!("run {} {}: {source}", node.display(), script.display()))?;
    if !output_status.status.success() {
        return Err(format!(
            "ooz bridge failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output_status.status,
            String::from_utf8_lossy(&output_status.stdout),
            String::from_utf8_lossy(&output_status.stderr)
        ));
    }
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            inspect_layout_db,
            latest_patch_versions,
            prefetch_bundles,
            clear_cache,
            scrape_campaign_acts_1_5
        ])
        .run(tauri::generate_context!())
        .expect("failed to run poe-layouts tauri app");
}
