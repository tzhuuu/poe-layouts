use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::cache::{CacheError, CacheFetch, CacheMode, DiskCache};

pub const DEFAULT_DAT_SCHEMA_URL: &str =
    "https://raw.githubusercontent.com/poe-tool-dev/dat-schema/main/dat-schema/_Core.gql";
pub const DEFAULT_DAT_SCHEMA_CACHE_KEY: &str = "schemas/poe-tool-dev/dat-schema/_Core.gql";
pub const DAT_SCHEMA_SNAPSHOT_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct DatSchemaClient {
    cache: DiskCache,
    url: String,
    cache_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DatSchemaSnapshotManifest {
    pub schema_version: u32,
    pub source_url: String,
    pub cache_key: String,
    pub cache_source: String,
    pub schema_path: PathBuf,
    pub byte_len: u64,
    pub blake3: String,
    pub required_types: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum DatSchemaError {
    #[error(transparent)]
    Cache(#[from] CacheError),
    #[error("dat schema is not utf-8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    #[error("dat schema is missing required types: {0}")]
    MissingTypes(String),
    #[error("dat schema io error for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("dat schema json error for {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

impl DatSchemaClient {
    #[must_use]
    pub fn new(cache: DiskCache) -> Self {
        Self {
            cache,
            url: DEFAULT_DAT_SCHEMA_URL.to_owned(),
            cache_key: DEFAULT_DAT_SCHEMA_CACHE_KEY.to_owned(),
        }
    }

    #[must_use]
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }

    #[must_use]
    pub fn with_cache_key(mut self, cache_key: impl Into<String>) -> Self {
        self.cache_key = cache_key.into();
        self
    }

    /// Fetch the GraphQL dat schema through the shared disk cache.
    ///
    /// # Errors
    ///
    /// Returns [`DatSchemaError`] when cache lookup, disk IO, or HTTP download
    /// fails.
    pub fn fetch(&self, mode: CacheMode) -> Result<CacheFetch, DatSchemaError> {
        Ok(self
            .cache
            .fetch_url_with_mode(&self.cache_key, &self.url, mode)?)
    }

    /// Fetch, validate, and write a checked-in schema snapshot plus manifest.
    ///
    /// # Errors
    ///
    /// Returns [`DatSchemaError`] when the schema cannot be fetched, decoded,
    /// validated, or written.
    pub fn update_snapshot(
        &self,
        schema_path: &Path,
        manifest_path: &Path,
        required_types: &[String],
        mode: CacheMode,
    ) -> Result<DatSchemaSnapshotManifest, DatSchemaError> {
        let fetch = self.fetch(mode)?;
        let bytes = fs::read(&fetch.path).map_err(|source| DatSchemaError::Io {
            path: fetch.path.clone(),
            source,
        })?;
        validate_required_types(&bytes, required_types)?;
        write_atomic(schema_path, &bytes)?;

        let manifest = DatSchemaSnapshotManifest {
            schema_version: DAT_SCHEMA_SNAPSHOT_VERSION,
            source_url: self.url.clone(),
            cache_key: self.cache_key.clone(),
            cache_source: format!("{:?}", fetch.source).to_lowercase(),
            schema_path: schema_path.to_owned(),
            byte_len: fetch.byte_len,
            blake3: fetch.blake3,
            required_types: required_types.to_vec(),
        };
        write_json_atomic(manifest_path, &manifest)?;
        Ok(manifest)
    }
}

/// Validate that the GraphQL schema defines all required table types.
///
/// # Errors
///
/// Returns [`DatSchemaError`] when the bytes are not UTF-8 or a required type is
/// missing.
pub fn validate_required_types(
    bytes: &[u8],
    required_types: &[String],
) -> Result<(), DatSchemaError> {
    let schema = std::str::from_utf8(bytes)?;
    let missing_types = required_types
        .iter()
        .filter(|type_name| !contains_graphql_type(schema, type_name))
        .cloned()
        .collect::<Vec<_>>();
    if missing_types.is_empty() {
        Ok(())
    } else {
        Err(DatSchemaError::MissingTypes(missing_types.join(", ")))
    }
}

#[must_use]
pub fn contains_graphql_type(schema: &str, type_name: &str) -> bool {
    schema.lines().any(|line| {
        let Some(rest) = line.trim_start().strip_prefix("type ") else {
            return false;
        };
        rest.split(|ch: char| ch.is_whitespace() || ch == '{' || ch == '@')
            .next()
            == Some(type_name)
    })
}

fn write_json_atomic(
    path: &Path,
    manifest: &DatSchemaSnapshotManifest,
) -> Result<(), DatSchemaError> {
    let bytes = format!(
        "{}\n",
        serde_json::to_string_pretty(manifest).map_err(|source| DatSchemaError::Json {
            path: path.to_owned(),
            source,
        })?
    );
    write_atomic(path, bytes.as_bytes())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), DatSchemaError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| DatSchemaError::Io {
            path: parent.to_owned(),
            source,
        })?;
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = NamedTempFile::new_in(parent).map_err(|source| DatSchemaError::Io {
        path: parent.to_owned(),
        source,
    })?;
    temp.write_all(bytes).map_err(|source| DatSchemaError::Io {
        path: path.to_owned(),
        source,
    })?;
    temp.persist(path).map_err(|error| DatSchemaError::Io {
        path: path.to_owned(),
        source: error.error,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{contains_graphql_type, validate_required_types, DEFAULT_DAT_SCHEMA_URL};

    #[test]
    fn graphql_type_detection_handles_directives_and_braces() {
        let schema = r"
            type WorldAreas @table {
              Id: string
            }

            type Topologies {
              DGRFile: string
            }
        ";

        assert!(contains_graphql_type(schema, "WorldAreas"));
        assert!(contains_graphql_type(schema, "Topologies"));
        assert!(!contains_graphql_type(schema, "Areas"));
    }

    #[test]
    fn required_type_validation_reports_missing_types() {
        let error = validate_required_types(
            b"type WorldAreas { Id: string }",
            &["WorldAreas".to_owned(), "Topologies".to_owned()],
        )
        .expect_err("missing topology type should fail validation");

        assert_eq!(
            error.to_string(),
            "dat schema is missing required types: Topologies"
        );
    }

    #[test]
    fn default_schema_url_points_at_core_graphql_schema() {
        assert!(DEFAULT_DAT_SCHEMA_URL.ends_with("/dat-schema/_Core.gql"));
    }
}
