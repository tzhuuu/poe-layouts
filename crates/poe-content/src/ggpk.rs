use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordTag([u8; 4]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordHeader {
    pub offset: u64,
    pub length: u64,
    pub tag: RecordTag,
}

#[derive(Debug, thiserror::Error)]
pub enum GgpkError {
    #[error("io error while reading GGPK record: {0}")]
    Io(#[from] io::Error),
    #[error("invalid GGPK record length {length} at offset {offset}")]
    InvalidRecordLength { offset: u64, length: u64 },
}

impl RecordTag {
    #[must_use]
    pub const fn new(bytes: [u8; 4]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.0).ok()
    }
}

impl fmt::Display for RecordTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.as_str() {
            Some(tag) => f.write_str(tag),
            None => write!(f, "{:02x?}", self.0),
        }
    }
}

/// Read the next GGPK record header from a seekable stream.
///
/// Returns `Ok(None)` when the stream is already at EOF.
///
/// # Errors
///
/// Returns [`GgpkError`] when the stream cannot be read or the record length is
/// smaller than the header itself.
pub fn read_record_header<R: Read + Seek>(
    reader: &mut R,
) -> Result<Option<RecordHeader>, GgpkError> {
    let offset = reader.stream_position()?;
    let mut header = [0; 12];
    match reader.read_exact(&mut header) {
        Ok(()) => {
            let mut length_bytes = [0; 8];
            length_bytes.copy_from_slice(&header[..8]);
            let length = u64::from_le_bytes(length_bytes);
            if length < 12 {
                return Err(GgpkError::InvalidRecordLength { offset, length });
            }
            let mut tag_bytes = [0; 4];
            tag_bytes.copy_from_slice(&header[8..12]);
            let tag = RecordTag(tag_bytes);
            Ok(Some(RecordHeader {
                offset,
                length,
                tag,
            }))
        }
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
        Err(error) => Err(GgpkError::Io(error)),
    }
}

/// Scan GGPK record headers by seeking over each record payload.
///
/// # Errors
///
/// Returns [`GgpkError`] when any record header cannot be read or skipped.
pub fn scan_record_headers<R: Read + Seek>(reader: &mut R) -> Result<Vec<RecordHeader>, GgpkError> {
    let mut headers = Vec::new();
    while let Some(header) = read_record_header(reader)? {
        let next_offset = header.offset + header.length;
        headers.push(header);
        reader.seek(SeekFrom::Start(next_offset))?;
    }
    Ok(headers)
}
