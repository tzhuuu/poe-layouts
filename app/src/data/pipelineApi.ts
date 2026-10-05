import { invoke } from "@tauri-apps/api/core";

export type LatestPatchVersions = {
  poe: string;
  poe2: string;
};

export type CacheManifestEntry = {
  key: string;
  url: string;
  byte_len: number;
  blake3: string;
};

export type CacheManifest = {
  schema_version: number;
  namespace: string;
  entries: CacheManifestEntry[];
};

export type CacheClearReport = {
  key: string;
  path: string;
  existed: boolean;
  removed: boolean;
  file_count: number;
  directory_count: number;
  byte_len: number;
};

export type PrefetchBundlesRequest = {
  patchVersion?: string;
  bundles: string[];
  refresh: boolean;
};

export type ClearCacheRequest = {
  releaseLine?: string;
  patchVersion?: string;
  dryRun: boolean;
};

export async function latestPatchVersions(): Promise<LatestPatchVersions> {
  return invoke<LatestPatchVersions>("latest_patch_versions");
}

export async function prefetchBundles(
  request: PrefetchBundlesRequest,
): Promise<CacheManifest> {
  return invoke<CacheManifest>("prefetch_bundles", { request });
}

export async function clearCache(
  request: ClearCacheRequest,
): Promise<CacheClearReport> {
  return invoke<CacheClearReport>("clear_cache", { request });
}
