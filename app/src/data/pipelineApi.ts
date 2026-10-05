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

export type ScrapeCampaignSummary = {
  scope: string;
  patchVersion: string;
  releaseLine: string;
  manifestPath: string;
  layoutDbPath?: string;
  selectedAreas: number;
  candidateFiles: number;
  extractedFiles: number;
  missingFiles: number;
  warnings: string[];
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

export type ScrapeCampaignRequest = {
  patchVersion?: string;
  refresh: boolean;
  offline: boolean;
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

export async function scrapeCampaignActsOneToFive(
  request: ScrapeCampaignRequest,
): Promise<ScrapeCampaignSummary> {
  return invoke<ScrapeCampaignSummary>("scrape_campaign_acts_1_5", { request });
}
