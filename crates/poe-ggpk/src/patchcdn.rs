use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cache::{CacheError, CacheFetch, CacheManifest, CacheMode, DiskCache};

const BUNDLE_DIR: &str = "Bundles2";
const INDEX_BUNDLE: &str = "_.index.bin";
const LATEST_VERSION_URL: &str = "https://poe-versions.obsoleet.org";
pub const SUPPORTED_POE1_RELEASE_LINE: &str = "3.29";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PoeGame {
    Poe1,
    Poe2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LatestPatchVersions {
    pub poe: String,
    pub poe2: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchCdnSource {
    pub game: PoeGame,
    pub patch_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameVersion {
    pub patch_version: String,
    pub release_line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchIndexSnapshot {
    pub game: PoeGame,
    pub patch_version: String,
    pub release_line: String,
    pub cache_namespace: String,
    pub index_url: String,
    pub cache_source: String,
    pub byte_len: u64,
    pub blake3: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PatchCdnError {
    #[error(transparent)]
    Cache(#[from] CacheError),
    #[error("http error while fetching latest patch versions: {0}")]
    LatestHttp(#[from] reqwest::Error),
}

impl PatchCdnSource {
    #[must_use]
    pub fn poe1(patch_version: impl Into<String>) -> Self {
        Self {
            game: PoeGame::Poe1,
            patch_version: patch_version.into(),
        }
    }

    #[must_use]
    pub fn poe2(patch_version: impl Into<String>) -> Self {
        Self {
            game: PoeGame::Poe2,
            patch_version: patch_version.into(),
        }
    }

    #[must_use]
    pub fn host(&self) -> &'static str {
        match self.game {
            PoeGame::Poe1 => "https://patch.poecdn.com",
            PoeGame::Poe2 => "https://patch-poe2.poecdn.com",
        }
    }

    #[must_use]
    pub fn game_slug(&self) -> &'static str {
        self.game.slug()
    }

    #[must_use]
    pub fn version(&self) -> GameVersion {
        GameVersion::parse(&self.patch_version)
    }

    #[must_use]
    pub fn release_line(&self) -> String {
        self.version().release_line
    }

    #[must_use]
    pub fn cache_namespace(&self) -> String {
        format!("{}/{}", self.game_slug(), self.release_line())
    }

    #[must_use]
    pub fn release_cache_namespace(game: PoeGame, release_line: &str) -> String {
        format!("{}/{}", game.slug(), release_line)
    }

    #[must_use]
    pub fn poe1_supported_release_namespace() -> String {
        Self::release_cache_namespace(PoeGame::Poe1, SUPPORTED_POE1_RELEASE_LINE)
    }

    #[must_use]
    pub fn bundle_url(&self, name: &str) -> String {
        format!(
            "{}/{}/{}/{}",
            self.host(),
            self.patch_version,
            BUNDLE_DIR,
            name
        )
    }

    #[must_use]
    pub fn cache_key(&self, name: &str) -> String {
        format!(
            "{}/patches/{}/{BUNDLE_DIR}/{name}",
            self.cache_namespace(),
            self.patch_version
        )
    }

    /// Fetch a bundle file through the supplied disk cache.
    ///
    /// # Errors
    ///
    /// Returns [`PatchCdnError`] when cache lookup, disk IO, or HTTP download
    /// fails.
    pub fn fetch_bundle(
        &self,
        cache: &DiskCache,
        name: &str,
        mode: CacheMode,
    ) -> Result<CacheFetch, PatchCdnError> {
        Ok(cache.fetch_url_with_mode(&self.cache_key(name), &self.bundle_url(name), mode)?)
    }

    /// Fetch `Bundles2/_.index.bin` through the supplied disk cache.
    ///
    /// # Errors
    ///
    /// Returns [`PatchCdnError`] when cache lookup, disk IO, or HTTP download
    /// fails.
    pub fn fetch_index(
        &self,
        cache: &DiskCache,
        mode: CacheMode,
    ) -> Result<CacheFetch, PatchCdnError> {
        self.fetch_bundle(cache, INDEX_BUNDLE, mode)
    }

    /// Fetch the bundle index and return stable metadata for snapshots.
    ///
    /// # Errors
    ///
    /// Returns [`PatchCdnError`] when the index cannot be fetched.
    pub fn snapshot_index(
        &self,
        cache: &DiskCache,
        mode: CacheMode,
    ) -> Result<PatchIndexSnapshot, PatchCdnError> {
        let fetch = self.fetch_index(cache, mode)?;
        Ok(PatchIndexSnapshot {
            game: self.game,
            patch_version: self.patch_version.clone(),
            release_line: self.release_line(),
            cache_namespace: self.cache_namespace(),
            index_url: self.bundle_url(INDEX_BUNDLE),
            cache_source: format!("{:?}", fetch.source).to_lowercase(),
            byte_len: fetch.byte_len,
            blake3: fetch.blake3,
        })
    }

    /// Fetch all named bundle files and return a manifest for offline runs.
    ///
    /// # Errors
    ///
    /// Returns [`PatchCdnError`] when any bundle cannot be fetched from cache
    /// or network according to `mode`.
    pub fn prefetch_bundles(
        &self,
        cache: &DiskCache,
        names: &[String],
        mode: CacheMode,
    ) -> Result<CacheManifest, PatchCdnError> {
        let fetches = names
            .iter()
            .map(|name| self.fetch_bundle(cache, name, mode))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CacheManifest::from_fetches(
            self.cache_namespace(),
            &fetches,
        ))
    }
}

impl PoeGame {
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Poe1 => "poe1",
            Self::Poe2 => "poe2",
        }
    }
}

impl GameVersion {
    #[must_use]
    pub fn parse(patch_version: &str) -> Self {
        let mut components = patch_version.split('.');
        let release_line = match (components.next(), components.next()) {
            (Some(major), Some(minor)) => format!("{major}.{minor}"),
            _ => patch_version.to_owned(),
        };
        Self {
            patch_version: patch_version.to_owned(),
            release_line,
        }
    }
}

impl LatestPatchVersions {
    #[must_use]
    pub fn poe1_source(&self) -> PatchCdnSource {
        PatchCdnSource::poe1(self.poe.clone())
    }

    #[must_use]
    pub fn poe2_source(&self) -> PatchCdnSource {
        PatchCdnSource::poe2(self.poe2.clone())
    }
}

/// Query the helper endpoint used by the existing JS tools for current versions.
///
/// # Errors
///
/// Returns [`PatchCdnError`] when the endpoint cannot be reached or decoded.
pub fn fetch_latest_patch_versions() -> Result<LatestPatchVersions, PatchCdnError> {
    Ok(reqwest::blocking::get(LATEST_VERSION_URL)?
        .error_for_status()?
        .json()?)
}

#[must_use]
pub fn default_cache_root(workspace_root: &Path) -> DiskCache {
    DiskCache::new(workspace_root.join(".poe-layouts").join("cache"))
}
