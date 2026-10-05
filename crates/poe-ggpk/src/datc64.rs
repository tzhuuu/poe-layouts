#![allow(clippy::missing_errors_doc)]

use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};

const ROW_COUNT_SIZE: usize = 4;
const MEMSIZE: usize = 8;
const MEM32_NULL: u32 = 0xfefe_fefe;
const VDATA_MAGIC: [u8; 8] = [0xbb; 8];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Datc64File<'a> {
    pub row_count: usize,
    pub row_length: usize,
    pub data_fixed: &'a [u8],
    pub data_variable: &'a [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Datc64Column {
    pub name: String,
    pub offset: usize,
    pub field_type: DatFieldType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatFieldType {
    Bool,
    I16,
    I32,
    U16,
    U32,
    F32,
    String,
    RowKey { foreign: bool },
    Array(Box<DatFieldType>),
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum DatValue {
    Null,
    Bool(bool),
    Integer(i64),
    Unsigned(u64),
    Float(f64),
    String(String),
    Array(Vec<DatValue>),
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Datc64Rows {
    pub row_count: usize,
    pub row_length: usize,
    pub columns: Vec<String>,
    pub rows: Vec<DatRow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DatRow(pub Vec<(String, DatValue)>);

impl Serialize for DatRow {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Datc64Error {
    #[error("invalid datc64 file size: {byte_len}")]
    InvalidFileSize { byte_len: usize },
    #[error("datc64 variable-data marker not found")]
    MissingVariableData,
    #[error("datc64 fixed data length {fixed_len} is not aligned to row count {row_count}")]
    MisalignedFixedData { fixed_len: usize, row_count: usize },
    #[error("unsupported dat field type: {0}")]
    UnsupportedType(String),
    #[error("read past end of {section} at offset {offset} for {byte_len} bytes")]
    OutOfBounds {
        section: &'static str,
        offset: usize,
        byte_len: usize,
    },
    #[error("invalid UTF-16 string at variable offset {offset}")]
    InvalidUtf16 { offset: usize },
}

pub fn parse_datc64(bytes: &[u8]) -> Result<Datc64File<'_>, Datc64Error> {
    if bytes.len() < ROW_COUNT_SIZE + VDATA_MAGIC.len() {
        return Err(Datc64Error::InvalidFileSize {
            byte_len: bytes.len(),
        });
    }
    let row_count = usize::try_from(read_u32(bytes, 0, "file")?).unwrap_or(usize::MAX);
    let body = &bytes[ROW_COUNT_SIZE..];
    let fixed_len = find_aligned_sequence(body, &VDATA_MAGIC, row_count)
        .ok_or(Datc64Error::MissingVariableData)?;
    let row_length = if row_count == 0 {
        0
    } else {
        if fixed_len % row_count != 0 {
            return Err(Datc64Error::MisalignedFixedData {
                fixed_len,
                row_count,
            });
        }
        fixed_len / row_count
    };
    Ok(Datc64File {
        row_count,
        row_length,
        data_fixed: &body[..fixed_len],
        data_variable: &body[fixed_len..],
    })
}

pub fn read_datc64_rows(
    bytes: &[u8],
    columns: &[Datc64Column],
    limit: Option<usize>,
) -> Result<Datc64Rows, Datc64Error> {
    let dat_file = parse_datc64(bytes)?;
    for column in columns {
        if !is_readable_field_type(&column.field_type) {
            return Err(Datc64Error::UnsupportedType(field_type_label(
                &column.field_type,
            )));
        }
    }

    let row_limit = limit.unwrap_or(dat_file.row_count).min(dat_file.row_count);
    let mut rows = Vec::with_capacity(row_limit);
    for row_index in 0..row_limit {
        let mut row = Vec::with_capacity(columns.len() + 1);
        row.push(("_index".to_owned(), DatValue::Unsigned(row_index as u64)));
        for column in columns {
            row.push((
                column.name.clone(),
                read_value(&dat_file, row_index, column.offset, &column.field_type)?,
            ));
        }
        rows.push(DatRow(row));
    }

    Ok(Datc64Rows {
        row_count: dat_file.row_count,
        row_length: dat_file.row_length,
        columns: columns.iter().map(|column| column.name.clone()).collect(),
        rows,
    })
}

pub fn field_length(field_type: &DatFieldType) -> Result<usize, Datc64Error> {
    Ok(match field_type {
        DatFieldType::Bool => 1,
        DatFieldType::I16 | DatFieldType::U16 => 2,
        DatFieldType::I32 | DatFieldType::U32 | DatFieldType::F32 => 4,
        DatFieldType::String | DatFieldType::RowKey { foreign: false } => 8,
        DatFieldType::RowKey { foreign: true } | DatFieldType::Array(_) => 16,
        DatFieldType::Unknown => {
            return Err(Datc64Error::UnsupportedType(field_type_label(field_type)));
        }
    })
}

#[must_use]
pub fn is_readable_field_type(field_type: &DatFieldType) -> bool {
    match field_type {
        DatFieldType::Unknown => false,
        DatFieldType::Array(element_type) => is_readable_field_type(element_type),
        DatFieldType::Bool
        | DatFieldType::I16
        | DatFieldType::I32
        | DatFieldType::U16
        | DatFieldType::U32
        | DatFieldType::F32
        | DatFieldType::String
        | DatFieldType::RowKey { .. } => true,
    }
}

#[must_use]
pub fn field_type_label(field_type: &DatFieldType) -> String {
    match field_type {
        DatFieldType::Bool => "bool".to_owned(),
        DatFieldType::I16 => "i16".to_owned(),
        DatFieldType::I32 => "i32".to_owned(),
        DatFieldType::U16 => "u16".to_owned(),
        DatFieldType::U32 => "u32".to_owned(),
        DatFieldType::F32 => "f32".to_owned(),
        DatFieldType::String => "string".to_owned(),
        DatFieldType::RowKey { foreign: true } => "foreignrow".to_owned(),
        DatFieldType::RowKey { foreign: false } => "row".to_owned(),
        DatFieldType::Array(element_type) => format!("[{}]", field_type_label(element_type)),
        DatFieldType::Unknown => "_".to_owned(),
    }
}

fn read_value(
    dat_file: &Datc64File<'_>,
    row_index: usize,
    field_offset: usize,
    field_type: &DatFieldType,
) -> Result<DatValue, Datc64Error> {
    let offset = row_index
        .checked_mul(dat_file.row_length)
        .and_then(|base| base.checked_add(field_offset))
        .ok_or(Datc64Error::OutOfBounds {
            section: "fixed",
            offset: usize::MAX,
            byte_len: 0,
        })?;
    read_one(dat_file, dat_file.data_fixed, "fixed", offset, field_type)
}

fn read_one(
    dat_file: &Datc64File<'_>,
    section: &[u8],
    section_name: &'static str,
    offset: usize,
    field_type: &DatFieldType,
) -> Result<DatValue, Datc64Error> {
    match field_type {
        DatFieldType::Bool => Ok(DatValue::Bool(read_u8(section, offset, section_name)? != 0)),
        DatFieldType::I16 => Ok(DatValue::Integer(i64::from(read_i16(
            section,
            offset,
            section_name,
        )?))),
        DatFieldType::I32 => Ok(DatValue::Integer(i64::from(read_i32(
            section,
            offset,
            section_name,
        )?))),
        DatFieldType::U16 => Ok(DatValue::Unsigned(u64::from(read_u16(
            section,
            offset,
            section_name,
        )?))),
        DatFieldType::U32 => Ok(DatValue::Unsigned(u64::from(read_u32(
            section,
            offset,
            section_name,
        )?))),
        DatFieldType::F32 => Ok(DatValue::Float(f64::from(read_f32(
            section,
            offset,
            section_name,
        )?))),
        DatFieldType::String => {
            let variable_offset =
                usize::try_from(read_u32(section, offset, section_name)?).unwrap_or(usize::MAX);
            read_string(dat_file.data_variable, variable_offset)
        }
        DatFieldType::RowKey { .. } => {
            let row_index = read_u32(section, offset, section_name)?;
            if row_index == MEM32_NULL {
                Ok(DatValue::Null)
            } else {
                Ok(DatValue::Unsigned(u64::from(row_index)))
            }
        }
        DatFieldType::Array(element_type) => {
            let array_len =
                usize::try_from(read_u32(section, offset, section_name)?).unwrap_or(usize::MAX);
            if array_len == 0 {
                return Ok(DatValue::Array(Vec::new()));
            }
            let variable_offset =
                usize::try_from(read_u32(section, offset + MEMSIZE, section_name)?)
                    .unwrap_or(usize::MAX);
            let element_size = field_length(element_type)?;
            let mut values = Vec::with_capacity(array_len);
            for index in 0..array_len {
                values.push(read_one(
                    dat_file,
                    dat_file.data_variable,
                    "variable",
                    variable_offset + index * element_size,
                    element_type,
                )?);
            }
            Ok(DatValue::Array(values))
        }
        DatFieldType::Unknown => Err(Datc64Error::UnsupportedType(field_type_label(field_type))),
    }
}

fn read_string(data_variable: &[u8], offset: usize) -> Result<DatValue, Datc64Error> {
    let mut end = find_zero_sequence(data_variable, 4, offset).ok_or(Datc64Error::OutOfBounds {
        section: "variable",
        offset,
        byte_len: 4,
    })?;
    while !(end - offset).is_multiple_of(2) {
        end = find_zero_sequence(data_variable, 4, end + 1).ok_or(Datc64Error::OutOfBounds {
            section: "variable",
            offset: end + 1,
            byte_len: 4,
        })?;
    }
    let bytes = checked_slice(data_variable, offset, end - offset, "variable")?;
    let code_units = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&code_units)
        .map(DatValue::String)
        .map_err(|_| Datc64Error::InvalidUtf16 { offset })
}

fn read_u8(data: &[u8], offset: usize, section: &'static str) -> Result<u8, Datc64Error> {
    Ok(*checked_slice(data, offset, 1, section)?
        .first()
        .expect("slice length checked"))
}

fn read_i16(data: &[u8], offset: usize, section: &'static str) -> Result<i16, Datc64Error> {
    Ok(i16::from_le_bytes(
        checked_slice(data, offset, 2, section)?
            .try_into()
            .expect("slice length checked"),
    ))
}

fn read_u16(data: &[u8], offset: usize, section: &'static str) -> Result<u16, Datc64Error> {
    Ok(u16::from_le_bytes(
        checked_slice(data, offset, 2, section)?
            .try_into()
            .expect("slice length checked"),
    ))
}

fn read_i32(data: &[u8], offset: usize, section: &'static str) -> Result<i32, Datc64Error> {
    Ok(i32::from_le_bytes(
        checked_slice(data, offset, 4, section)?
            .try_into()
            .expect("slice length checked"),
    ))
}

fn read_u32(data: &[u8], offset: usize, section: &'static str) -> Result<u32, Datc64Error> {
    Ok(u32::from_le_bytes(
        checked_slice(data, offset, 4, section)?
            .try_into()
            .expect("slice length checked"),
    ))
}

fn read_f32(data: &[u8], offset: usize, section: &'static str) -> Result<f32, Datc64Error> {
    Ok(f32::from_le_bytes(
        checked_slice(data, offset, 4, section)?
            .try_into()
            .expect("slice length checked"),
    ))
}

fn checked_slice<'a>(
    data: &'a [u8],
    offset: usize,
    byte_len: usize,
    section: &'static str,
) -> Result<&'a [u8], Datc64Error> {
    data.get(offset..offset + byte_len)
        .ok_or(Datc64Error::OutOfBounds {
            section,
            offset,
            byte_len,
        })
}

fn find_aligned_sequence(data: &[u8], sequence: &[u8], element_count: usize) -> Option<usize> {
    let mut from_index = 0;
    loop {
        let idx = find_sequence(data, sequence, from_index)?;
        if element_count == 0 || idx % element_count == 0 {
            return Some(idx);
        }
        from_index = idx + 1;
    }
}

fn find_zero_sequence(data: &[u8], length: usize, offset: usize) -> Option<usize> {
    (offset..=data.len().saturating_sub(length))
        .find(|idx| data[*idx..*idx + length].iter().all(|byte| *byte == 0))
}

fn find_sequence(data: &[u8], sequence: &[u8], from_index: usize) -> Option<usize> {
    data.get(from_index..)?
        .windows(sequence.len())
        .position(|window| window == sequence)
        .map(|idx| idx + from_index)
}

#[cfg(test)]
mod tests {
    use super::{read_datc64_rows, DatFieldType, DatValue, Datc64Column};

    #[test]
    fn generic_datc64_reader_reads_projected_columns_without_schema() {
        let bytes = synthetic_datc64();
        let columns = vec![
            Datc64Column {
                name: "Id".to_owned(),
                offset: 0,
                field_type: DatFieldType::String,
            },
            Datc64Column {
                name: "Act".to_owned(),
                offset: 8,
                field_type: DatFieldType::I32,
            },
        ];
        let rows = read_datc64_rows(&bytes, &columns, None).expect("read rows");

        assert_eq!(rows.row_count, 1);
        assert_eq!(rows.row_length, 12);
        assert_eq!(
            rows.rows[0].0,
            vec![
                ("_index".to_owned(), DatValue::Unsigned(0)),
                ("Id".to_owned(), DatValue::String("1_1_1".to_owned())),
                ("Act".to_owned(), DatValue::Integer(1)),
            ]
        );
    }

    fn synthetic_datc64() -> Vec<u8> {
        let mut variable = vec![0xbb; 8];
        let id_offset = push_utf16_string(&mut variable, "1_1_1");

        let mut fixed = Vec::new();
        fixed.extend_from_slice(
            &u32::try_from(id_offset)
                .expect("offset fits u32")
                .to_le_bytes(),
        );
        fixed.extend_from_slice(&0_u32.to_le_bytes());
        fixed.extend_from_slice(&1_i32.to_le_bytes());

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&fixed);
        bytes.extend_from_slice(&variable);
        bytes
    }

    fn push_utf16_string(variable: &mut Vec<u8>, value: &str) -> usize {
        let offset = variable.len();
        for code_unit in value.encode_utf16() {
            variable.extend_from_slice(&code_unit.to_le_bytes());
        }
        variable.extend_from_slice(&[0, 0, 0, 0]);
        offset
    }
}
