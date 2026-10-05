use std::str::Utf8Error;

use crate::bundle::{decompress_bundle, BundleChunkDecoder, BundleError};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct IndexBundle {
    pub bundles: Vec<BundleIndexEntry>,
    pub files: Vec<FileIndexEntry>,
    pub directories: Vec<DirectoryIndexEntry>,
    pub path_reps_bundle: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HydratedIndexBundle {
    pub index: IndexBundle,
    pub path_reps: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BundleIndexEntry {
    pub name: String,
    pub decompressed_size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FileIndexEntry {
    pub path_hash: u64,
    pub bundle_index: u32,
    pub offset: u32,
    pub size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DirectoryIndexEntry {
    pub path_hash: u64,
    pub path_reps_offset: u32,
    pub direct_size: u32,
    pub recursive_size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LogicalFileLocation {
    pub bundle: String,
    pub offset: u32,
    pub size: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum IndexBundleError {
    #[error(transparent)]
    Bundle(#[from] BundleError),
    #[error("index bundle ended early: need at least {needed} bytes, found {actual}")]
    UnexpectedEof { needed: usize, actual: usize },
    #[error("index bundle count is negative for {section}: {count}")]
    NegativeCount { section: &'static str, count: i32 },
    #[error("index bundle offset overflow while reading {section}")]
    OffsetOverflow { section: &'static str },
    #[error("index bundle contains invalid utf-8 bundle name: {0}")]
    InvalidUtf8(#[from] Utf8Error),
    #[error("file entry references missing bundle index {bundle_index}")]
    InvalidBundleIndex { bundle_index: u32 },
}

#[derive(Debug, thiserror::Error)]
pub enum PathRepsError {
    #[error("path reps ended early: need at least {needed} bytes, found {actual}")]
    UnexpectedEof { needed: usize, actual: usize },
    #[error("path reps string at offset {offset} is missing a null terminator")]
    MissingNull { offset: usize },
    #[error("path reps contains invalid utf-8: {0}")]
    InvalidUtf8(#[from] Utf8Error),
}

const U32_SIZE: usize = 4;
const I32_SIZE: usize = 4;
const U64_SIZE: usize = 8;

/// Parse decompressed `Bundles2/_.index.bin` contents.
///
/// The CDN stores `_.index.bin` as a compressed bundle. Callers must decompress
/// that bundle first, then pass the decompressed bytes here.
///
/// # Errors
///
/// Returns [`IndexBundleError`] when the decompressed index has invalid section
/// counts, truncated fields, or invalid UTF-8 bundle names.
pub fn parse_index_bundle(bytes: &[u8]) -> Result<IndexBundle, IndexBundleError> {
    let mut offset = 0;

    let bundles_count = read_count(bytes, &mut offset, "bundles")?;
    let mut bundles = Vec::with_capacity(bundles_count);
    for _ in 0..bundles_count {
        let name_len = read_count(bytes, &mut offset, "bundle_name")?;
        let name_bytes = read_slice(bytes, &mut offset, name_len, "bundle_name")?;
        let name = std::str::from_utf8(name_bytes)?.to_owned();
        let decompressed_size = read_u32(bytes, &mut offset, "bundle_decompressed_size")?;
        bundles.push(BundleIndexEntry {
            name,
            decompressed_size,
        });
    }

    let files_count = read_count(bytes, &mut offset, "files")?;
    let mut files = Vec::with_capacity(files_count);
    for _ in 0..files_count {
        files.push(FileIndexEntry {
            path_hash: read_u64(bytes, &mut offset, "file_hash")?,
            bundle_index: read_u32(bytes, &mut offset, "file_bundle_index")?,
            offset: read_u32(bytes, &mut offset, "file_offset")?,
            size: read_u32(bytes, &mut offset, "file_size")?,
        });
    }

    let directories_count = read_count(bytes, &mut offset, "directories")?;
    let mut directories = Vec::with_capacity(directories_count);
    for _ in 0..directories_count {
        directories.push(DirectoryIndexEntry {
            path_hash: read_u64(bytes, &mut offset, "directory_hash")?,
            path_reps_offset: read_u32(bytes, &mut offset, "directory_path_reps_offset")?,
            direct_size: read_u32(bytes, &mut offset, "directory_direct_size")?,
            recursive_size: read_u32(bytes, &mut offset, "directory_recursive_size")?,
        });
    }

    Ok(IndexBundle {
        bundles,
        files,
        directories,
        path_reps_bundle: bytes[offset..].to_vec(),
    })
}

/// Decompress and parse `Bundles2/_.index.bin`, including its nested path reps
/// bundle.
///
/// # Errors
///
/// Returns [`IndexBundleError`] when bundle decompression fails or the
/// decompressed index sections are invalid.
pub fn hydrate_index_bundle<D>(
    index_bundle_bytes: &[u8],
    decoder: &mut D,
) -> Result<HydratedIndexBundle, IndexBundleError>
where
    D: BundleChunkDecoder,
{
    let index_bytes = decompress_bundle(index_bundle_bytes, decoder)?;
    let index = parse_index_bundle(&index_bytes)?;
    let path_reps = decompress_bundle(&index.path_reps_bundle, decoder)?;
    Ok(HydratedIndexBundle { index, path_reps })
}

impl IndexBundle {
    #[must_use]
    pub fn summary(&self) -> IndexBundleSummary {
        IndexBundleSummary {
            bundles: self.bundles.len(),
            files: self.files.len(),
            directories: self.directories.len(),
            path_reps_bundle_size: self.path_reps_bundle.len(),
        }
    }

    /// Look up the bundle location for a logical file path.
    ///
    /// Paths are hashed case-insensitively to match the existing JS toolchain.
    ///
    /// # Errors
    ///
    /// Returns [`IndexBundleError::InvalidBundleIndex`] when the file entry is
    /// present but points outside the bundle table.
    pub fn file_location(
        &self,
        logical_path: &str,
    ) -> Result<Option<LogicalFileLocation>, IndexBundleError> {
        let path_hash = murmur64a_lower(logical_path);
        let Some(entry) = self.files.iter().find(|entry| entry.path_hash == path_hash) else {
            return Ok(None);
        };
        let bundle_index = usize::try_from(entry.bundle_index).map_err(|_| {
            IndexBundleError::InvalidBundleIndex {
                bundle_index: entry.bundle_index,
            }
        })?;
        let Some(bundle) = self.bundles.get(bundle_index) else {
            return Err(IndexBundleError::InvalidBundleIndex {
                bundle_index: entry.bundle_index,
            });
        };
        Ok(Some(LogicalFileLocation {
            bundle: format!("{}.bundle.bin", bundle.name),
            offset: entry.offset,
            size: entry.size,
        }))
    }
}

impl HydratedIndexBundle {
    /// Expand decompressed path reps into logical file paths.
    ///
    /// # Errors
    ///
    /// Returns [`PathRepsError`] when path reps are truncated or contain invalid
    /// strings.
    pub fn logical_paths(&self) -> Result<Vec<String>, PathRepsError> {
        unpack_path_reps(&self.path_reps)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct IndexBundleSummary {
    pub bundles: usize,
    pub files: usize,
    pub directories: usize,
    pub path_reps_bundle_size: usize,
}

#[must_use]
pub fn murmur64a_lower(path: &str) -> u64 {
    murmur64a(path.to_lowercase().as_bytes())
}

#[must_use]
pub fn murmur64a(data: &[u8]) -> u64 {
    const M: u64 = 0xc6a4_a793_5bd1_e995;
    const R: u32 = 47;
    const SEED: u64 = 0x1337_b33f;

    let data_len = u64::try_from(data.len()).unwrap_or(u64::MAX);
    let mut hash = SEED ^ data_len.wrapping_mul(M);
    let mut chunks = data.chunks_exact(8);
    for chunk in &mut chunks {
        let mut key_bytes = [0; 8];
        key_bytes.copy_from_slice(chunk);
        let mut key = u64::from_le_bytes(key_bytes);
        key = key.wrapping_mul(M);
        key ^= key >> R;
        key = key.wrapping_mul(M);

        hash ^= key;
        hash = hash.wrapping_mul(M);
    }

    let remainder = chunks.remainder();
    if !remainder.is_empty() {
        for (idx, byte) in remainder.iter().enumerate() {
            hash ^= u64::from(*byte) << (idx * 8);
        }
        hash = hash.wrapping_mul(M);
    }

    hash ^= hash >> R;
    hash = hash.wrapping_mul(M);
    hash ^= hash >> R;
    hash
}

/// Unpack a decompressed path reps payload into logical file paths.
///
/// # Errors
///
/// Returns [`PathRepsError`] when path reps are truncated or contain invalid
/// strings.
pub fn unpack_path_reps(data: &[u8]) -> Result<Vec<String>, PathRepsError> {
    let mut offset = 0usize;
    let mut base_mode = false;
    let mut bases: Vec<String> = Vec::new();
    let mut paths = Vec::new();

    while offset <= data.len().saturating_sub(I32_SIZE) {
        let idx = read_i32_path_rep(data, &mut offset)? - 1;
        if idx == -1 {
            base_mode = !base_mode;
            if base_mode {
                bases.clear();
            }
            continue;
        }

        let string_offset = offset;
        let Some(null_offset) = data[string_offset..]
            .iter()
            .position(|byte| *byte == 0)
            .map(|relative| string_offset + relative)
        else {
            return Err(PathRepsError::MissingNull {
                offset: string_offset,
            });
        };
        let mut path = std::str::from_utf8(&data[string_offset..null_offset])?.to_owned();
        offset = null_offset + 1;

        if let Ok(base_index) = usize::try_from(idx) {
            if let Some(base) = bases.get(base_index) {
                path = format!("{base}{path}");
            }
        }

        if base_mode {
            bases.push(path);
        } else {
            paths.push(path);
        }
    }

    Ok(paths)
}

#[must_use]
pub fn root_directories(paths: &[String]) -> Vec<String> {
    let mut roots = paths
        .iter()
        .filter_map(|path| path.split('/').next())
        .filter(|root| !root.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    roots.sort();
    roots.dedup();
    roots
}

fn read_i32_path_rep(data: &[u8], offset: &mut usize) -> Result<i32, PathRepsError> {
    let end = offset
        .checked_add(I32_SIZE)
        .ok_or(PathRepsError::UnexpectedEof {
            needed: usize::MAX,
            actual: data.len(),
        })?;
    if data.len() < end {
        return Err(PathRepsError::UnexpectedEof {
            needed: end,
            actual: data.len(),
        });
    }
    let value = i32::from_le_bytes(
        data[*offset..end]
            .try_into()
            .expect("path rep slice length checked"),
    );
    *offset = end;
    Ok(value)
}

fn read_count(
    bytes: &[u8],
    offset: &mut usize,
    section: &'static str,
) -> Result<usize, IndexBundleError> {
    let count = read_i32(bytes, offset, section)?;
    if count < 0 {
        return Err(IndexBundleError::NegativeCount { section, count });
    }
    usize::try_from(count).map_err(|_| IndexBundleError::OffsetOverflow { section })
}

fn read_i32(
    bytes: &[u8],
    offset: &mut usize,
    section: &'static str,
) -> Result<i32, IndexBundleError> {
    let raw = read_slice(bytes, offset, I32_SIZE, section)?;
    Ok(i32::from_le_bytes(
        raw.try_into().expect("slice length checked"),
    ))
}

fn read_u32(
    bytes: &[u8],
    offset: &mut usize,
    section: &'static str,
) -> Result<u32, IndexBundleError> {
    let raw = read_slice(bytes, offset, U32_SIZE, section)?;
    Ok(u32::from_le_bytes(
        raw.try_into().expect("slice length checked"),
    ))
}

fn read_u64(
    bytes: &[u8],
    offset: &mut usize,
    section: &'static str,
) -> Result<u64, IndexBundleError> {
    let raw = read_slice(bytes, offset, U64_SIZE, section)?;
    Ok(u64::from_le_bytes(
        raw.try_into().expect("slice length checked"),
    ))
}

fn read_slice<'a>(
    bytes: &'a [u8],
    offset: &mut usize,
    len: usize,
    section: &'static str,
) -> Result<&'a [u8], IndexBundleError> {
    let end = offset
        .checked_add(len)
        .ok_or(IndexBundleError::OffsetOverflow { section })?;
    if bytes.len() < end {
        return Err(IndexBundleError::UnexpectedEof {
            needed: end,
            actual: bytes.len(),
        });
    }
    let slice = &bytes[*offset..end];
    *offset = end;
    Ok(slice)
}
