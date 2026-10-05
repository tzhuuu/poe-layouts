#![allow(clippy::missing_errors_doc)]

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::cache::{CacheFetch, CacheMode, DiskCache};
use crate::index_bundle::{
    murmur64a_lower, parse_index_bundle, unpack_path_reps, IndexBundle, IndexBundleError,
    LogicalFileLocation, PathRepsError,
};
use crate::patchcdn::{PatchCdnError, PatchCdnSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundleSlice {
    pub offset: usize,
    pub size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleSliceOutput {
    pub output_path: PathBuf,
    pub slice: BundleSlice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalFileExtractRequest {
    pub logical_path: String,
    pub output_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PatchClientIndex {
    pub patch_version: String,
    pub release_line: String,
    pub index: IndexBundle,
    pub logical_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExtractedLogicalFile {
    pub logical_path: String,
    pub output_path: PathBuf,
    pub location: LogicalFileLocation,
    pub cache_key: String,
    pub cache_path: PathBuf,
    pub cache_source: String,
    pub byte_len: u64,
    pub blake3: String,
}

#[derive(Debug, Clone)]
pub struct PatchClient {
    source: PatchCdnSource,
    cache: DiskCache,
    mode: CacheMode,
}

pub trait BundleDecompressor {
    type Error: std::error::Error + Send + Sync + 'static;

    fn decompress_bundle(
        &mut self,
        input: &Path,
        output: &Path,
        slice: Option<BundleSlice>,
    ) -> Result<(), Self::Error>;

    fn decompress_bundle_slices(
        &mut self,
        input: &Path,
        slices: &[BundleSliceOutput],
    ) -> Result<(), Self::Error> {
        for slice in slices {
            self.decompress_bundle(input, &slice.output_path, Some(slice.slice))?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PatchClientError {
    #[error(transparent)]
    PatchCdn(#[from] PatchCdnError),
    #[error(transparent)]
    Index(#[from] IndexBundleError),
    #[error(transparent)]
    PathReps(#[from] PathRepsError),
    #[error("logical path not found in patch index: {0}")]
    MissingLogicalPath(String),
    #[error("logical file offset or size does not fit this platform: {0}")]
    LocationTooLarge(String),
    #[error("bundle decompression failed: {0}")]
    Decompress(Box<dyn std::error::Error + Send + Sync>),
    #[error("io error for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl PatchClient {
    #[must_use]
    pub fn new(source: PatchCdnSource, cache: DiskCache, mode: CacheMode) -> Self {
        Self {
            source,
            cache,
            mode,
        }
    }

    #[must_use]
    pub fn source(&self) -> &PatchCdnSource {
        &self.source
    }

    pub fn load_index<D>(&self, decompressor: &mut D) -> Result<PatchClientIndex, PatchClientError>
    where
        D: BundleDecompressor,
    {
        let temp = tempfile::tempdir().map_err(|source| PatchClientError::Io {
            path: std::env::temp_dir(),
            source,
        })?;
        let index_bundle = self.source.fetch_index(&self.cache, self.mode)?;
        let decompressed_index_path = temp.path().join("index.bin");
        decompressor
            .decompress_bundle(&index_bundle.path, &decompressed_index_path, None)
            .map_err(|error| PatchClientError::Decompress(Box::new(error)))?;

        let index_bytes = read_file(&decompressed_index_path)?;
        let index = parse_index_bundle(&index_bytes)?;

        let path_reps_bundle_path = temp.path().join("path-reps.bundle.bin");
        let path_reps_path = temp.path().join("path-reps.bin");
        write_file(&path_reps_bundle_path, &index.path_reps_bundle)?;
        decompressor
            .decompress_bundle(&path_reps_bundle_path, &path_reps_path, None)
            .map_err(|error| PatchClientError::Decompress(Box::new(error)))?;
        let path_reps = read_file(&path_reps_path)?;
        let logical_paths = unpack_path_reps(&path_reps)?;

        Ok(PatchClientIndex {
            patch_version: self.source.patch_version.clone(),
            release_line: self.source.release_line(),
            index,
            logical_paths,
        })
    }

    pub fn extract_logical_file<D>(
        &self,
        index: &IndexBundle,
        logical_path: &str,
        output_path: &Path,
        decompressor: &mut D,
    ) -> Result<ExtractedLogicalFile, PatchClientError>
    where
        D: BundleDecompressor,
    {
        let location = index
            .file_location(logical_path)?
            .ok_or_else(|| PatchClientError::MissingLogicalPath(logical_path.to_owned()))?;
        let fetch = self
            .source
            .fetch_bundle(&self.cache, &location.bundle, self.mode)?;
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| PatchClientError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let offset = usize::try_from(location.offset)
            .map_err(|_| PatchClientError::LocationTooLarge(logical_path.to_owned()))?;
        let size = usize::try_from(location.size)
            .map_err(|_| PatchClientError::LocationTooLarge(logical_path.to_owned()))?;
        let logical_byte_len = u64::from(location.size);
        if self.mode == CacheMode::Refresh || !file_has_size(output_path, logical_byte_len)? {
            decompressor
                .decompress_bundle(&fetch.path, output_path, Some(BundleSlice { offset, size }))
                .map_err(|error| PatchClientError::Decompress(Box::new(error)))?;
        }
        Ok(ExtractedLogicalFile {
            logical_path: logical_path.to_owned(),
            output_path: output_path.to_path_buf(),
            location,
            cache_key: fetch.key,
            cache_path: fetch.path,
            cache_source: format!("{:?}", fetch.source).to_lowercase(),
            byte_len: logical_byte_len,
            blake3: fetch.blake3,
        })
    }

    pub fn extract_logical_files<D>(
        &self,
        index: &IndexBundle,
        requests: &[LogicalFileExtractRequest],
        decompressor: &mut D,
    ) -> Result<Vec<ExtractedLogicalFile>, PatchClientError>
    where
        D: BundleDecompressor,
    {
        let mut extracted = Vec::new();
        let mut pending_by_bundle = BTreeMap::<PathBuf, Vec<PendingExtract>>::new();
        let mut fetches_by_bundle = HashMap::<String, CacheFetch>::new();
        let file_entries_by_hash = index
            .files
            .iter()
            .map(|entry| (entry.path_hash, entry))
            .collect::<HashMap<_, _>>();
        eprintln!(
            "resolving {} logical file requests against indexed path hash map",
            requests.len()
        );

        for request in requests {
            let path_hash = murmur64a_lower(&request.logical_path);
            let entry = file_entries_by_hash.get(&path_hash).ok_or_else(|| {
                PatchClientError::MissingLogicalPath(request.logical_path.clone())
            })?;
            let bundle_index = usize::try_from(entry.bundle_index).map_err(|_| {
                IndexBundleError::InvalidBundleIndex {
                    bundle_index: entry.bundle_index,
                }
            })?;
            let bundle = index.bundles.get(bundle_index).ok_or({
                IndexBundleError::InvalidBundleIndex {
                    bundle_index: entry.bundle_index,
                }
            })?;
            let location = LogicalFileLocation {
                bundle: format!("{}.bundle.bin", bundle.name),
                offset: entry.offset,
                size: entry.size,
            };
            let fetch = match fetches_by_bundle.get(&location.bundle) {
                Some(fetch) => fetch.clone(),
                None => {
                    let fetch =
                        self.source
                            .fetch_bundle(&self.cache, &location.bundle, self.mode)?;
                    fetches_by_bundle.insert(location.bundle.clone(), fetch.clone());
                    fetch
                }
            };
            if let Some(parent) = request.output_path.parent() {
                std::fs::create_dir_all(parent).map_err(|source| PatchClientError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            let offset = usize::try_from(location.offset)
                .map_err(|_| PatchClientError::LocationTooLarge(request.logical_path.clone()))?;
            let size = usize::try_from(location.size)
                .map_err(|_| PatchClientError::LocationTooLarge(request.logical_path.clone()))?;
            let logical_byte_len = u64::from(location.size);
            let file = ExtractedLogicalFile {
                logical_path: request.logical_path.clone(),
                output_path: request.output_path.clone(),
                location,
                cache_key: fetch.key,
                cache_path: fetch.path.clone(),
                cache_source: format!("{:?}", fetch.source).to_lowercase(),
                byte_len: logical_byte_len,
                blake3: fetch.blake3,
            };
            if self.mode != CacheMode::Refresh
                && file_has_size(&request.output_path, logical_byte_len)?
            {
                extracted.push(file);
                continue;
            }
            pending_by_bundle
                .entry(fetch.path)
                .or_default()
                .push(PendingExtract {
                    file,
                    slice: BundleSlice { offset, size },
                });
        }

        let bundle_count = pending_by_bundle.len();
        let pending_file_count = pending_by_bundle.values().map(Vec::len).sum::<usize>();
        eprintln!(
            "extracting {pending_file_count} pending files from {bundle_count} bundle batches"
        );
        for (index, (bundle_path, mut pending)) in pending_by_bundle.into_iter().enumerate() {
            pending.sort_by_key(|item| item.slice.offset);
            eprintln!(
                "extracting bundle batch {}/{}: {} ({} files)",
                index + 1,
                bundle_count,
                bundle_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("<bundle>"),
                pending.len()
            );
            let slices = pending
                .iter()
                .map(|item| BundleSliceOutput {
                    output_path: item.file.output_path.clone(),
                    slice: item.slice,
                })
                .collect::<Vec<_>>();
            decompressor
                .decompress_bundle_slices(&bundle_path, &slices)
                .map_err(|error| PatchClientError::Decompress(Box::new(error)))?;
            extracted.extend(pending.into_iter().map(|item| item.file));
        }

        Ok(extracted)
    }
}

#[derive(Debug)]
struct PendingExtract {
    file: ExtractedLogicalFile,
    slice: BundleSlice,
}

fn file_has_size(path: &Path, byte_len: u64) -> Result<bool, PatchClientError> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file() && metadata.len() == byte_len),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(PatchClientError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn read_file(path: &Path) -> Result<Vec<u8>, PatchClientError> {
    std::fs::read(path).map_err(|source| PatchClientError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), PatchClientError> {
    std::fs::write(path, bytes).map_err(|source| PatchClientError::Io {
        path: path.to_path_buf(),
        source,
    })
}
