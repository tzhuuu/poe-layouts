#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleHeader {
    pub decompressed_data_size: u32,
    pub chunk_count: u32,
    pub compression_granularity: u32,
    pub chunk_sizes: Vec<u32>,
    pub payload_offset: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("bundle header ended early: need at least {needed} bytes, found {actual}")]
    UnexpectedEof { needed: usize, actual: usize },
    #[error("bundle chunk table is too large: {chunk_count} chunks")]
    ChunkTableTooLarge { chunk_count: u32 },
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
