use std::path::PathBuf;

use poe_ggpk::{
    default_cache_root, fetch_latest_patch_versions, CacheClearReport, CacheManifest, CacheMode,
    DiskCache, LatestPatchVersions, PatchCdnSource, PoeGame,
};
use poe_layouts_core::{inspect_layout_database, LayoutDbSummary};
use serde::Deserialize;

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

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            inspect_layout_db,
            latest_patch_versions,
            prefetch_bundles,
            clear_cache
        ])
        .run(tauri::generate_context!())
        .expect("failed to run poe-layouts tauri app");
}
