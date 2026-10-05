#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleHeader {
    pub decompressed_data_size: u32,
    pub chunk_count: u32,
    pub compression_granularity: u32,
    pub chunk_sizes: Vec<u32>,
    pub payload_offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundleChunk<'a> {
    pub compressed: &'a [u8],
    pub decompressed_size: usize,
    pub decompressed_offset: usize,
}

pub trait BundleChunkDecoder {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Decode one compressed bundle chunk.
    ///
    /// # Errors
    ///
    /// Returns the decoder-specific error when the chunk cannot be decoded.
    fn decode_chunk(&mut self, chunk: BundleChunk<'_>) -> Result<Vec<u8>, Self::Error>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredBundleDecoder;

#[derive(Debug, thiserror::Error)]
pub enum StoredBundleError {
    #[error("stored chunk size mismatch: expected {expected} bytes, found {actual}")]
    SizeMismatch { expected: usize, actual: usize },
}

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("bundle header ended early: need at least {needed} bytes, found {actual}")]
    UnexpectedEof { needed: usize, actual: usize },
    #[error("bundle chunk table is too large: {chunk_count} chunks")]
    ChunkTableTooLarge { chunk_count: u32 },
    #[error("bundle size is too large to fit this platform: {size}")]
    SizeTooLarge { size: u32 },
    #[error(
        "bundle chunk {chunk_index} overruns payload: need at least {needed} bytes, found {actual}"
    )]
    ChunkOutOfBounds {
        chunk_index: usize,
        needed: usize,
        actual: usize,
    },
    #[error(
        "decoded chunk {chunk_index} size mismatch: expected {expected} bytes, found {actual}"
    )]
    DecodedChunkSize {
        chunk_index: usize,
        expected: usize,
        actual: usize,
    },
    #[error("bundle decoder error: {0}")]
    Decoder(Box<dyn std::error::Error + Send + Sync>),
}

const DECOMPRESSED_DATA_SIZE_OFFSET: usize = 0;
const CHUNK_COUNT_OFFSET: usize = 36;
const COMPRESSION_GRANULARITY_OFFSET: usize = 40;
const CHUNK_SIZES_OFFSET: usize = 60;
const U32_SIZE: usize = 4;

/// Parse metadata from a compressed `Bundles2/*.bundle.bin` file.
///
/// This does not decompress Oodle chunks. It only reads the stable header and
/// chunk table so callers can inspect cached bundle bytes and plan later
/// decompression work.
///
/// # Errors
///
/// Returns [`BundleError`] when the byte slice is too short or the chunk table
/// cannot fit in memory on this platform.
pub fn parse_bundle_header(bytes: &[u8]) -> Result<BundleHeader, BundleError> {
    let decompressed_data_size = read_u32(bytes, DECOMPRESSED_DATA_SIZE_OFFSET)?;
    let chunk_count = read_u32(bytes, CHUNK_COUNT_OFFSET)?;
    let compression_granularity = read_u32(bytes, COMPRESSION_GRANULARITY_OFFSET)?;
    let chunk_count_usize = usize::try_from(chunk_count)
        .map_err(|_| BundleError::ChunkTableTooLarge { chunk_count })?;
    let chunk_table_len = chunk_count_usize
        .checked_mul(U32_SIZE)
        .ok_or(BundleError::ChunkTableTooLarge { chunk_count })?;
    let payload_offset = CHUNK_SIZES_OFFSET
        .checked_add(chunk_table_len)
        .ok_or(BundleError::ChunkTableTooLarge { chunk_count })?;
    require_len(bytes, payload_offset)?;

    let mut chunk_sizes = Vec::with_capacity(chunk_count_usize);
    for idx in 0..chunk_count_usize {
        chunk_sizes.push(read_u32(bytes, CHUNK_SIZES_OFFSET + idx * U32_SIZE)?);
    }

    Ok(BundleHeader {
        decompressed_data_size,
        chunk_count,
        compression_granularity,
        chunk_sizes,
        payload_offset,
    })
}

/// Decompress a whole bundle into a new byte vector.
///
/// # Errors
///
/// Returns [`BundleError`] when the bundle header is invalid, a chunk range is
/// out of bounds, or the supplied decoder fails.
pub fn decompress_bundle<D>(bytes: &[u8], decoder: &mut D) -> Result<Vec<u8>, BundleError>
where
    D: BundleChunkDecoder,
{
    let header = parse_bundle_header(bytes)?;
    let decompressed_len =
        usize::try_from(header.decompressed_data_size).map_err(|_| BundleError::SizeTooLarge {
            size: header.decompressed_data_size,
        })?;
    let mut output = vec![0; decompressed_len];
    decompress_bundle_slice(bytes, 0, &mut output, decoder)?;
    Ok(output)
}

/// Decompress a slice of a bundle into `output`.
///
/// The slice is identified by `slice_offset` in the decompressed bundle and
/// `output.len()` bytes. Only chunks overlapping that slice are decoded.
///
/// # Errors
///
/// Returns [`BundleError`] when the bundle header is invalid, a chunk range is
/// out of bounds, or the supplied decoder fails.
pub fn decompress_bundle_slice<D>(
    bytes: &[u8],
    slice_offset: usize,
    output: &mut [u8],
    decoder: &mut D,
) -> Result<(), BundleError>
where
    D: BundleChunkDecoder,
{
    let header = parse_bundle_header(bytes)?;
    let mut chunk_begin = header.payload_offset;
    let mut decompressed_offset = 0usize;
    let slice_end = slice_offset
        .checked_add(output.len())
        .ok_or(BundleError::SizeTooLarge { size: u32::MAX })?;
    let mut output_offset = 0usize;

    for (chunk_index, compressed_size) in header.chunk_sizes.iter().enumerate() {
        let compressed_size =
            usize::try_from(*compressed_size).map_err(|_| BundleError::ChunkOutOfBounds {
                chunk_index,
                needed: usize::MAX,
                actual: bytes.len(),
            })?;
        let chunk_end =
            chunk_begin
                .checked_add(compressed_size)
                .ok_or(BundleError::ChunkOutOfBounds {
                    chunk_index,
                    needed: usize::MAX,
                    actual: bytes.len(),
                })?;
        if bytes.len() < chunk_end {
            return Err(BundleError::ChunkOutOfBounds {
                chunk_index,
                needed: chunk_end,
                actual: bytes.len(),
            });
        }

        let decompressed_size = chunk_decompressed_size(&header, chunk_index)?;
        let decompressed_end = decompressed_offset.checked_add(decompressed_size).ok_or(
            BundleError::SizeTooLarge {
                size: header.decompressed_data_size,
            },
        )?;

        if ranges_overlap(
            decompressed_offset,
            decompressed_end,
            slice_offset,
            slice_end,
        ) {
            let chunk_bytes = decoder
                .decode_chunk(BundleChunk {
                    compressed: &bytes[chunk_begin..chunk_end],
                    decompressed_size,
                    decompressed_offset,
                })
                .map_err(|error| BundleError::Decoder(Box::new(error)))?;
            if chunk_bytes.len() != decompressed_size {
                return Err(BundleError::DecodedChunkSize {
                    chunk_index,
                    expected: decompressed_size,
                    actual: chunk_bytes.len(),
                });
            }

            let copy_begin = slice_offset.saturating_sub(decompressed_offset);
            let copy_end = (slice_end.min(decompressed_end)) - decompressed_offset;
            let copy_len = copy_end - copy_begin;
            output[output_offset..output_offset + copy_len]
                .copy_from_slice(&chunk_bytes[copy_begin..copy_end]);
            output_offset += copy_len;
        }

        decompressed_offset = decompressed_end;
        chunk_begin = chunk_end;
    }
    Ok(())
}

impl BundleChunkDecoder for StoredBundleDecoder {
    type Error = StoredBundleError;

    fn decode_chunk(&mut self, chunk: BundleChunk<'_>) -> Result<Vec<u8>, Self::Error> {
        if chunk.compressed.len() != chunk.decompressed_size {
            return Err(StoredBundleError::SizeMismatch {
                expected: chunk.decompressed_size,
                actual: chunk.compressed.len(),
            });
        }
        Ok(chunk.compressed.to_vec())
    }
}

fn chunk_decompressed_size(
    header: &BundleHeader,
    chunk_index: usize,
) -> Result<usize, BundleError> {
    let chunk_count =
        usize::try_from(header.chunk_count).map_err(|_| BundleError::ChunkTableTooLarge {
            chunk_count: header.chunk_count,
        })?;
    let granularity =
        usize::try_from(header.compression_granularity).map_err(|_| BundleError::SizeTooLarge {
            size: header.compression_granularity,
        })?;
    let decompressed_len =
        usize::try_from(header.decompressed_data_size).map_err(|_| BundleError::SizeTooLarge {
            size: header.decompressed_data_size,
        })?;
    if chunk_index + 1 == chunk_count {
        let remainder = decompressed_len % granularity;
        if remainder == 0 {
            Ok(granularity)
        } else {
            Ok(remainder)
        }
    } else {
        Ok(granularity)
    }
}

fn ranges_overlap(a_start: usize, a_end: usize, b_start: usize, b_end: usize) -> bool {
    a_start.max(b_start) < a_end.min(b_end)
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, BundleError> {
    require_len(bytes, offset + U32_SIZE)?;
    Ok(u32::from_le_bytes(
        bytes[offset..offset + U32_SIZE]
            .try_into()
            .expect("slice length checked"),
    ))
}

fn require_len(bytes: &[u8], needed: usize) -> Result<(), BundleError> {
    if bytes.len() < needed {
        return Err(BundleError::UnexpectedEof {
            needed,
            actual: bytes.len(),
        });
    }
    Ok(())
}
