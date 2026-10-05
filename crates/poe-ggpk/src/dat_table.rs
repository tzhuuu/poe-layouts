use std::path::Path;

use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};

const ROW_COUNT_SIZE: usize = 4;
const MEMSIZE: usize = 8;
const MEM32_NULL: u32 = 0xfefe_fefe;
const VDATA_MAGIC: [u8; 8] = [0xbb; 8];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatFile<'a> {
    pub row_count: usize,
    pub row_length: usize,
    pub data_fixed: &'a [u8],
    pub data_variable: &'a [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphqlTable {
    pub name: String,
    pub columns: Vec<GraphqlColumn>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphqlColumn {
    pub name: String,
    pub type_name: String,
    pub array: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatColumnHeader {
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
pub struct DatTableRows {
    pub table_name: String,
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
pub enum DatTableError {
    #[error("invalid datc64 file size: {byte_len}")]
    InvalidFileSize { byte_len: usize },
    #[error("datc64 variable-data marker not found")]
    MissingVariableData,
    #[error("datc64 fixed data length {fixed_len} is not aligned to row count {row_count}")]
    MisalignedFixedData { fixed_len: usize, row_count: usize },
    #[error("GraphQL table not found: {0}")]
    MissingTable(String),
    #[error("GraphQL table is not closed: {0}")]
    UnclosedTable(String),
    #[error("GraphQL field is malformed in table {table}: {line}")]
    MalformedField { table: String, line: String },
    #[error("column not found in table {table}: {column}")]
    MissingColumn { table: String, column: String },
    #[error("unsupported column type for {column}: {type_name}")]
    UnsupportedType { column: String, type_name: String },
    #[error("read past end of {section} at offset {offset} for {byte_len} bytes")]
    OutOfBounds {
        section: &'static str,
        offset: usize,
        byte_len: usize,
    },
    #[error("invalid UTF-16 string at variable offset {offset}")]
    InvalidUtf16 { offset: usize },
}

/// Parse the `.datc64` envelope into fixed-width rows and variable data.
///
/// # Errors
///
/// Returns [`DatTableError`] when the file is too small, the variable-data
/// marker is missing, or the fixed section cannot be divided into rows.
pub fn parse_datc64(bytes: &[u8]) -> Result<DatFile<'_>, DatTableError> {
    if bytes.len() < ROW_COUNT_SIZE + VDATA_MAGIC.len() {
        return Err(DatTableError::InvalidFileSize {
            byte_len: bytes.len(),
        });
    }
    let row_count = usize::try_from(read_u32(bytes, 0, "file")?).unwrap_or(usize::MAX);
    let body = &bytes[ROW_COUNT_SIZE..];
    let fixed_len = find_aligned_sequence(body, &VDATA_MAGIC, row_count)
        .ok_or(DatTableError::MissingVariableData)?;
    let row_length = if row_count == 0 {
        0
    } else {
        if fixed_len % row_count != 0 {
            return Err(DatTableError::MisalignedFixedData {
                fixed_len,
                row_count,
            });
        }
        fixed_len / row_count
    };
    Ok(DatFile {
        row_count,
        row_length,
        data_fixed: &body[..fixed_len],
        data_variable: &body[fixed_len..],
    })
}

/// Parse one table definition from the checked-in GraphQL dat schema.
///
/// # Errors
///
/// Returns [`DatTableError`] when the table is missing or a field line is
/// malformed.
pub fn parse_graphql_table(schema: &str, table_name: &str) -> Result<GraphqlTable, DatTableError> {
    let mut in_table = false;
    let mut columns = Vec::new();
    for raw_line in schema.lines() {
        let line = raw_line.trim();
        if !in_table {
            if line == format!("type {table_name} {{")
                || line.starts_with(&format!("type {table_name} "))
            {
                in_table = true;
                if line.ends_with('}') {
                    return Ok(GraphqlTable {
                        name: table_name.to_owned(),
                        columns,
                    });
                }
            }
            continue;
        }
        if line == "}" {
            return Ok(GraphqlTable {
                name: table_name.to_owned(),
                columns,
            });
        }
        if line.is_empty() || line.starts_with('#') || line.starts_with('"') {
            continue;
        }
        let Some((name, rest)) = line.split_once(':') else {
            return Err(DatTableError::MalformedField {
                table: table_name.to_owned(),
                line: line.to_owned(),
            });
        };
        let type_token = rest
            .split(|ch: char| ch.is_whitespace() || ch == '@' || ch == '#')
            .find(|part| !part.is_empty())
            .ok_or_else(|| DatTableError::MalformedField {
                table: table_name.to_owned(),
                line: line.to_owned(),
            })?;
        let (array, type_name) = array_type(type_token);
        columns.push(GraphqlColumn {
            name: name.trim().to_owned(),
            type_name: type_name.to_owned(),
            array,
        });
    }
    if in_table {
        Err(DatTableError::UnclosedTable(table_name.to_owned()))
    } else {
        Err(DatTableError::MissingTable(table_name.to_owned()))
    }
}

/// Build fixed-row headers from a GraphQL table definition.
///
/// # Errors
///
/// Returns [`DatTableError`] when a schema type cannot be mapped to a dat field
/// layout.
pub fn headers_from_graphql_table(
    table: &GraphqlTable,
) -> Result<Vec<DatColumnHeader>, DatTableError> {
    let mut offset = 0;
    let mut headers = Vec::with_capacity(table.columns.len());
    for column in &table.columns {
        let field_type = field_type_for(table, column)?;
        let field_len = field_length(&field_type)?;
        headers.push(DatColumnHeader {
            name: column.name.clone(),
            offset,
            field_type,
        });
        offset += field_len;
    }
    Ok(headers)
}

/// Read projected rows from a `.datc64` table using the GraphQL schema.
///
/// # Errors
///
/// Returns [`DatTableError`] when the table, columns, or binary data are
/// invalid.
pub fn read_dat_table(
    bytes: &[u8],
    schema: &str,
    table_name: &str,
    columns: &[String],
    limit: Option<usize>,
) -> Result<DatTableRows, DatTableError> {
    let dat_file = parse_datc64(bytes)?;
    let table = parse_graphql_table(schema, table_name)?;
    let headers = headers_from_graphql_table(&table)?;
    let selected_headers = columns
        .iter()
        .map(|column| {
            headers
                .iter()
                .find(|header| header.name == *column)
                .ok_or_else(|| DatTableError::MissingColumn {
                    table: table_name.to_owned(),
                    column: column.clone(),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let row_limit = limit.unwrap_or(dat_file.row_count).min(dat_file.row_count);
    let mut rows = Vec::with_capacity(row_limit);
    for row_index in 0..row_limit {
        let mut row = Vec::with_capacity(selected_headers.len() + 1);
        row.push(("_index".to_owned(), DatValue::Unsigned(row_index as u64)));
        for header in &selected_headers {
            row.push((
                header.name.clone(),
                read_value(&dat_file, row_index, header.offset, &header.field_type)?,
            ));
        }
        rows.push(DatRow(row));
    }

    Ok(DatTableRows {
        table_name: table_name.to_owned(),
        row_count: dat_file.row_count,
        row_length: dat_file.row_length,
        columns: columns.to_vec(),
        rows,
    })
}

#[must_use]
pub fn table_name_from_path(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.split('.').next())
        .map(str::to_owned)
}

fn array_type(type_token: &str) -> (bool, &str) {
    type_token
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .map_or((false, type_token), |inner| (true, inner))
}

fn field_type_for(
    table: &GraphqlTable,
    column: &GraphqlColumn,
) -> Result<DatFieldType, DatTableError> {
    let inner = match column.type_name.as_str() {
        "bool" => DatFieldType::Bool,
        "i16" => DatFieldType::I16,
        "i32" => DatFieldType::I32,
        "u16" => DatFieldType::U16,
        "u32" => DatFieldType::U32,
        "f32" => DatFieldType::F32,
        "string" => DatFieldType::String,
        "rid" => DatFieldType::RowKey { foreign: true },
        "_" => DatFieldType::Unknown,
        other if other.chars().next().is_some_and(char::is_uppercase) => DatFieldType::RowKey {
            foreign: other != table.name,
        },
        other => {
            return Err(DatTableError::UnsupportedType {
                column: column.name.clone(),
                type_name: other.to_owned(),
            });
        }
    };
    Ok(if column.array {
        DatFieldType::Array(Box::new(inner))
    } else {
        inner
    })
}

fn field_length(field_type: &DatFieldType) -> Result<usize, DatTableError> {
    Ok(match field_type {
        DatFieldType::Bool => 1,
        DatFieldType::I16 | DatFieldType::U16 => 2,
        DatFieldType::I32 | DatFieldType::U32 | DatFieldType::F32 => 4,
        DatFieldType::String | DatFieldType::RowKey { foreign: false } => 8,
        DatFieldType::RowKey { foreign: true } | DatFieldType::Array(_) => 16,
        DatFieldType::Unknown => {
            return Err(DatTableError::UnsupportedType {
                column: "_".to_owned(),
                type_name: "_".to_owned(),
            });
        }
    })
}

fn read_value(
    dat_file: &DatFile<'_>,
    row_index: usize,
    field_offset: usize,
    field_type: &DatFieldType,
) -> Result<DatValue, DatTableError> {
    let offset = row_index
        .checked_mul(dat_file.row_length)
        .and_then(|base| base.checked_add(field_offset))
        .ok_or(DatTableError::OutOfBounds {
            section: "fixed",
            offset: usize::MAX,
            byte_len: 0,
        })?;
    read_one(dat_file, dat_file.data_fixed, "fixed", offset, field_type)
}

fn read_one(
    dat_file: &DatFile<'_>,
    section: &[u8],
    section_name: &'static str,
    offset: usize,
    field_type: &DatFieldType,
) -> Result<DatValue, DatTableError> {
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
        DatFieldType::Unknown => Err(DatTableError::UnsupportedType {
            column: "_".to_owned(),
            type_name: "_".to_owned(),
        }),
    }
}

fn read_string(data_variable: &[u8], offset: usize) -> Result<DatValue, DatTableError> {
    let mut end =
        find_zero_sequence(data_variable, 4, offset).ok_or(DatTableError::OutOfBounds {
            section: "variable",
            offset,
            byte_len: 4,
        })?;
    while !(end - offset).is_multiple_of(2) {
        end = find_zero_sequence(data_variable, 4, end + 1).ok_or(DatTableError::OutOfBounds {
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
        .map_err(|_| DatTableError::InvalidUtf16 { offset })
}

fn read_u8(data: &[u8], offset: usize, section: &'static str) -> Result<u8, DatTableError> {
    Ok(*checked_slice(data, offset, 1, section)?
        .first()
        .expect("slice length checked"))
}

fn read_i16(data: &[u8], offset: usize, section: &'static str) -> Result<i16, DatTableError> {
    Ok(i16::from_le_bytes(
        checked_slice(data, offset, 2, section)?
            .try_into()
            .expect("slice length checked"),
    ))
}

fn read_u16(data: &[u8], offset: usize, section: &'static str) -> Result<u16, DatTableError> {
    Ok(u16::from_le_bytes(
        checked_slice(data, offset, 2, section)?
            .try_into()
            .expect("slice length checked"),
    ))
}

fn read_i32(data: &[u8], offset: usize, section: &'static str) -> Result<i32, DatTableError> {
    Ok(i32::from_le_bytes(
        checked_slice(data, offset, 4, section)?
            .try_into()
            .expect("slice length checked"),
    ))
}

fn read_u32(data: &[u8], offset: usize, section: &'static str) -> Result<u32, DatTableError> {
    Ok(u32::from_le_bytes(
        checked_slice(data, offset, 4, section)?
            .try_into()
            .expect("slice length checked"),
    ))
}

fn read_f32(data: &[u8], offset: usize, section: &'static str) -> Result<f32, DatTableError> {
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
) -> Result<&'a [u8], DatTableError> {
    data.get(offset..offset + byte_len)
        .ok_or(DatTableError::OutOfBounds {
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
    use super::{headers_from_graphql_table, parse_graphql_table, read_dat_table, DatValue};

    #[test]
    fn graphql_table_headers_are_stable_for_layout_tables() {
        let schema = include_str!("../../../schema/dat/_Core.gql");
        let world_areas = parse_graphql_table(schema, "WorldAreas").expect("parse WorldAreas");
        let world_headers = headers_from_graphql_table(&world_areas).expect("WorldAreas headers");
        let topologies = parse_graphql_table(schema, "Topologies").expect("parse Topologies");
        let topology_headers = headers_from_graphql_table(&topologies).expect("Topologies headers");

        assert_eq!(world_headers[0].name, "Id");
        assert_eq!(world_headers[0].offset, 0);
        assert_eq!(world_headers[14].name, "TopologiesKeys");
        assert_eq!(world_headers[14].offset, 104);
        assert_eq!(topology_headers[1].name, "DGRFile");
        assert_eq!(topology_headers[1].offset, 8);
    }

    #[test]
    fn synthetic_datc64_rows_read_projected_columns() {
        let schema = r"
            type WorldAreas {
              Id: string
              Act: i32
              IsTown: bool
              Connections_WorldAreasKeys: [WorldAreas]
            }
        ";
        let bytes = synthetic_world_areas_datc64();
        let rows = read_dat_table(
            &bytes,
            schema,
            "WorldAreas",
            &[
                "Id".to_owned(),
                "Act".to_owned(),
                "IsTown".to_owned(),
                "Connections_WorldAreasKeys".to_owned(),
            ],
            None,
        )
        .expect("read synthetic datc64");

        assert_eq!(rows.row_count, 2);
        assert_eq!(rows.row_length, 29);
        assert_eq!(
            rows.rows[0].0,
            vec![
                ("_index".to_owned(), DatValue::Unsigned(0)),
                ("Id".to_owned(), DatValue::String("1_1_1".to_owned())),
                ("Act".to_owned(), DatValue::Integer(1)),
                ("IsTown".to_owned(), DatValue::Bool(false)),
                (
                    "Connections_WorldAreasKeys".to_owned(),
                    DatValue::Array(vec![DatValue::Unsigned(1), DatValue::Unsigned(2)])
                ),
            ]
        );
        assert_eq!(
            rows.rows[1].0,
            vec![
                ("_index".to_owned(), DatValue::Unsigned(1)),
                ("Id".to_owned(), DatValue::String("1_1_town".to_owned())),
                ("Act".to_owned(), DatValue::Integer(1)),
                ("IsTown".to_owned(), DatValue::Bool(true)),
                (
                    "Connections_WorldAreasKeys".to_owned(),
                    DatValue::Array(Vec::new())
                ),
            ]
        );
    }

    fn synthetic_world_areas_datc64() -> Vec<u8> {
        let mut variable = vec![0xbb; 8];
        let id0 = push_utf16_string(&mut variable, "1_1_1");
        let id1 = push_utf16_string(&mut variable, "1_1_town");
        let connections0 = variable.len();
        variable.extend_from_slice(&1_u32.to_le_bytes());
        variable.extend_from_slice(&0_u32.to_le_bytes());
        variable.extend_from_slice(&2_u32.to_le_bytes());
        variable.extend_from_slice(&0_u32.to_le_bytes());

        let mut fixed = Vec::new();
        push_world_area_row(&mut fixed, id0, 1, false, 2, connections0);
        push_world_area_row(&mut fixed, id1, 1, true, 0, 0);

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&2_u32.to_le_bytes());
        bytes.extend_from_slice(&fixed);
        bytes.extend_from_slice(&variable);
        bytes
    }

    fn push_world_area_row(
        fixed: &mut Vec<u8>,
        id_offset: usize,
        act: i32,
        is_town: bool,
        connection_count: u32,
        connection_offset: usize,
    ) {
        fixed.extend_from_slice(
            &u32::try_from(id_offset)
                .expect("offset fits u32")
                .to_le_bytes(),
        );
        fixed.extend_from_slice(&0_u32.to_le_bytes());
        fixed.extend_from_slice(&act.to_le_bytes());
        fixed.push(u8::from(is_town));
        fixed.extend_from_slice(&connection_count.to_le_bytes());
        fixed.extend_from_slice(&0_u32.to_le_bytes());
        fixed.extend_from_slice(
            &u32::try_from(connection_offset)
                .expect("offset fits u32")
                .to_le_bytes(),
        );
        fixed.extend_from_slice(&0_u32.to_le_bytes());
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
