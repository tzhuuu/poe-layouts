use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

#[derive(Debug, Clone)]
pub struct DiskCache {
    root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheFetch {
    pub key: String,
    pub url: String,
    pub path: PathBuf,
    pub metadata_path: PathBuf,
    pub source: CacheSource,
    pub byte_len: u64,
    pub blake3: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CacheSource {
    Cache,
    Network,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CacheMode {
    Online,
    Offline,
    Refresh,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheManifest {
    pub schema_version: u32,
    pub namespace: String,
    pub entries: Vec<CacheManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheManifestEntry {
    pub key: String,
    pub url: String,
    pub byte_len: u64,
    pub blake3: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheVerification {
    pub checked: usize,
    pub missing: Vec<String>,
    pub mismatched: Vec<CacheMismatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheClearReport {
    pub key: String,
    pub path: PathBuf,
    pub existed: bool,
    pub removed: bool,
    pub file_count: usize,
    pub directory_count: usize,
    pub byte_len: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheMismatch {
    pub key: String,
    pub expected_blake3: String,
    pub actual_blake3: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheMetadata {
    url: String,
    byte_len: u64,
    blake3: String,
    etag: Option<String>,
    last_modified: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("cache key must be a relative safe path: {0}")]
    UnsafeKey(String),
    #[error("offline cache miss for {key}")]
    CacheMiss { key: String },
    #[error("cache io error for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cache metadata json error for {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("http error while fetching {url}: {source}")]
    Http {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("unexpected http status while fetching {url}: {status}")]
    HttpStatus { url: String, status: u16 },
}

impl DiskCache {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Fetch `url` into the cache location identified by `key`.
    ///
    /// Existing files are reused and rehashed. Missing files are downloaded and
    /// written atomically with a sidecar metadata JSON file.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError`] when the key is unsafe, the cache cannot be read
    /// or written, the metadata cannot be encoded, or the HTTP request fails.
    pub fn fetch_url(&self, key: &str, url: &str) -> Result<CacheFetch, CacheError> {
        self.fetch_url_with_mode(key, url, CacheMode::Online)
    }

    /// Fetch `url` with an explicit cache/network mode.
    ///
    /// `CacheMode::Offline` only returns already-cached bytes and never attempts
    /// an HTTP request.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError`] when the key is unsafe, offline mode misses, the
    /// cache cannot be read or written, the metadata cannot be encoded, or the
    /// HTTP request fails.
    pub fn fetch_url_with_mode(
        &self,
        key: &str,
        url: &str,
        mode: CacheMode,
    ) -> Result<CacheFetch, CacheError> {
        let path = self.path_for_key(key)?;
        let metadata_path = metadata_path_for(&path);
        if path.exists() && mode != CacheMode::Refresh {
            let byte_len = fs::metadata(&path)
                .map_err(|source| CacheError::Io {
                    path: path.clone(),
                    source,
                })?
                .len();
            let blake3 = hash_file(&path)?;
            return Ok(CacheFetch {
                key: key.to_owned(),
                url: url.to_owned(),
                path,
                metadata_path,
                source: CacheSource::Cache,
                byte_len,
                blake3,
            });
        }
        if mode == CacheMode::Offline {
            return Err(CacheError::CacheMiss {
                key: key.to_owned(),
            });
        }

        let response = reqwest::blocking::get(url).map_err(|source| CacheError::Http {
            url: url.to_owned(),
            source,
        })?;
        if !response.status().is_success() {
            return Err(CacheError::HttpStatus {
                url: url.to_owned(),
                status: response.status().as_u16(),
            });
        }

        let etag = header_string(response.headers(), reqwest::header::ETAG);
        let last_modified = header_string(response.headers(), reqwest::header::LAST_MODIFIED);
        let bytes = response.bytes().map_err(|source| CacheError::Http {
            url: url.to_owned(),
            source,
        })?;
        write_atomic(&path, &bytes)?;
        let blake3 = blake3::hash(&bytes).to_hex().to_string();
        let byte_len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let metadata = CacheMetadata {
            url: url.to_owned(),
            byte_len,
            blake3: blake3.clone(),
            etag,
            last_modified,
        };
        write_json_atomic(&metadata_path, &metadata)?;

        Ok(CacheFetch {
            key: key.to_owned(),
            url: url.to_owned(),
            path,
            metadata_path,
            source: CacheSource::Network,
            byte_len,
            blake3,
        })
    }

    /// Resolve a cache key into its on-disk path.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError::UnsafeKey`] when `key` is absolute or contains
    /// parent/current-directory components.
    pub fn path_for_key(&self, key: &str) -> Result<PathBuf, CacheError> {
        if key.is_empty() {
            return Err(CacheError::UnsafeKey(key.to_owned()));
        }
        let key_path = Path::new(key);
        if key_path.is_absolute() {
            return Err(CacheError::UnsafeKey(key.to_owned()));
        }

        let mut path = self.root.clone();
        for component in key_path.components() {
            match component {
                Component::Normal(part) => path.push(part),
                _ => return Err(CacheError::UnsafeKey(key.to_owned())),
            }
        }
        Ok(path)
    }

    /// Remove the cache subtree identified by `key`.
    ///
    /// This is intended for namespace-style keys such as a `PoE` release-line
    /// namespace, but also works for an individual cached file key.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError`] when the key is unsafe or the subtree cannot be
    /// inspected or removed.
    pub fn clear_key(&self, key: &str) -> Result<CacheClearReport, CacheError> {
        self.clear_key_with_mode(key, false)
    }

    /// Inspect the cache subtree identified by `key` without removing it.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError`] when the key is unsafe or the subtree cannot be
    /// inspected.
    pub fn preview_clear_key(&self, key: &str) -> Result<CacheClearReport, CacheError> {
        self.clear_key_with_mode(key, true)
    }

    /// Write a cache manifest atomically.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError`] when the manifest cannot be encoded or written.
    pub fn write_manifest(&self, path: &Path, manifest: &CacheManifest) -> Result<(), CacheError> {
        let bytes = serde_json::to_vec_pretty(manifest).map_err(|source| CacheError::Json {
            path: path.to_owned(),
            source,
        })?;
        write_atomic(path, &bytes)
    }

    /// Read a cache manifest from disk.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError`] when the manifest cannot be read or decoded.
    pub fn read_manifest(&self, path: &Path) -> Result<CacheManifest, CacheError> {
        let bytes = fs::read(path).map_err(|source| CacheError::Io {
            path: path.to_owned(),
            source,
        })?;
        serde_json::from_slice(&bytes).map_err(|source| CacheError::Json {
            path: path.to_owned(),
            source,
        })
    }

    /// Verify every manifest entry against bytes already present in the cache.
    ///
    /// This is always offline and never issues network requests.
    ///
    /// # Errors
    ///
    /// Returns [`CacheError`] when any manifest key is unsafe or a cached file
    /// cannot be read.
    pub fn verify_manifest(
        &self,
        manifest: &CacheManifest,
    ) -> Result<CacheVerification, CacheError> {
        let mut missing = Vec::new();
        let mut mismatched = Vec::new();
        for entry in &manifest.entries {
            let path = self.path_for_key(&entry.key)?;
            if !path.exists() {
                missing.push(entry.key.clone());
                continue;
            }
            let actual_blake3 = hash_file(&path)?;
            if actual_blake3 != entry.blake3 {
                mismatched.push(CacheMismatch {
                    key: entry.key.clone(),
                    expected_blake3: entry.blake3.clone(),
                    actual_blake3,
                });
            }
        }
        Ok(CacheVerification {
            checked: manifest.entries.len(),
            missing,
            mismatched,
        })
    }

    fn clear_key_with_mode(
        &self,
        key: &str,
        dry_run: bool,
    ) -> Result<CacheClearReport, CacheError> {
        let path = self.path_for_key(key)?;
        if !path.exists() {
            return Ok(CacheClearReport {
                key: key.to_owned(),
                path,
                existed: false,
                removed: false,
                file_count: 0,
                directory_count: 0,
                byte_len: 0,
            });
        }

        let stats = path_stats(&path)?;
        if !dry_run {
            remove_path(&path)?;
        }

        Ok(CacheClearReport {
            key: key.to_owned(),
            path,
            existed: true,
            removed: !dry_run,
            file_count: stats.file_count,
            directory_count: stats.directory_count,
            byte_len: stats.byte_len,
        })
    }
}

impl CacheManifest {
    #[must_use]
    pub fn from_fetches(namespace: impl Into<String>, fetches: &[CacheFetch]) -> Self {
        Self {
            schema_version: 1,
            namespace: namespace.into(),
            entries: fetches
                .iter()
                .map(|fetch| CacheManifestEntry {
                    key: fetch.key.clone(),
                    url: fetch.url.clone(),
                    byte_len: fetch.byte_len,
                    blake3: fetch.blake3.clone(),
                })
                .collect(),
        }
    }
}

fn metadata_path_for(path: &Path) -> PathBuf {
    let mut metadata_path = path.as_os_str().to_owned();
    metadata_path.push(".json");
    PathBuf::from(metadata_path)
}

fn header_string(
    headers: &reqwest::header::HeaderMap,
    name: reqwest::header::HeaderName,
) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), CacheError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| CacheError::Io {
            path: parent.to_owned(),
            source,
        })?;
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = NamedTempFile::new_in(parent).map_err(|source| CacheError::Io {
        path: parent.to_owned(),
        source,
    })?;
    temp.write_all(bytes).map_err(|source| CacheError::Io {
        path: path.to_owned(),
        source,
    })?;
    temp.persist(path).map_err(|error| CacheError::Io {
        path: path.to_owned(),
        source: error.error,
    })?;
    Ok(())
}

fn write_json_atomic(path: &Path, metadata: &CacheMetadata) -> Result<(), CacheError> {
    let bytes = serde_json::to_vec_pretty(metadata).map_err(|source| CacheError::Json {
        path: path.to_owned(),
        source,
    })?;
    write_atomic(path, &bytes)
}

#[derive(Debug, Clone, Copy)]
struct PathStats {
    file_count: usize,
    directory_count: usize,
    byte_len: u64,
}

fn path_stats(path: &Path) -> Result<PathStats, CacheError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| CacheError::Io {
        path: path.to_owned(),
        source,
    })?;
    if metadata.is_dir() {
        let mut stats = PathStats {
            file_count: 0,
            directory_count: 1,
            byte_len: 0,
        };
        for entry in fs::read_dir(path).map_err(|source| CacheError::Io {
            path: path.to_owned(),
            source,
        })? {
            let entry = entry.map_err(|source| CacheError::Io {
                path: path.to_owned(),
                source,
            })?;
            let child = path_stats(&entry.path())?;
            stats.file_count += child.file_count;
            stats.directory_count += child.directory_count;
            stats.byte_len += child.byte_len;
        }
        Ok(stats)
    } else {
        Ok(PathStats {
            file_count: 1,
            directory_count: 0,
            byte_len: metadata.len(),
        })
    }
}

fn remove_path(path: &Path) -> Result<(), CacheError> {
    let metadata = fs::symlink_metadata(path).map_err(|source| CacheError::Io {
        path: path.to_owned(),
        source,
    })?;
    if metadata.is_dir() {
        fs::remove_dir_all(path).map_err(|source| CacheError::Io {
            path: path.to_owned(),
            source,
        })
    } else {
        fs::remove_file(path).map_err(|source| CacheError::Io {
            path: path.to_owned(),
            source,
        })
    }
}

fn hash_file(path: &Path) -> Result<String, CacheError> {
    let mut file = File::open(path).map_err(|source| CacheError::Io {
        path: path.to_owned(),
        source,
    })?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0; 64 * 1024];
    loop {
        let read = file.read(&mut buf).map_err(|source| CacheError::Io {
            path: path.to_owned(),
            source,
        })?;
        if read == 0 {
            return Ok(hasher.finalize().to_hex().to_string());
        }
        hasher.update(&buf[..read]);
    }
}
