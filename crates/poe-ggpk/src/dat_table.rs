#![allow(clippy::missing_errors_doc)]

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::dat_file::{
    field_length, field_type_label, is_readable_field_type, read_projected_rows, DatColumn,
    DatFieldType, DatFileError, DatRow,
};

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
pub struct DatTableReader {
    tables: Vec<GraphqlTable>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatColumnHeader {
    pub name: String,
    pub source_name: String,
    pub offset: usize,
    pub field_type: DatFieldType,
}

impl From<&DatColumnHeader> for DatColumn {
    fn from(header: &DatColumnHeader) -> Self {
        Self {
            name: header.name.clone(),
            offset: header.offset,
            field_type: header.field_type.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DatTableRows {
    pub table_name: String,
    pub row_count: usize,
    pub row_length: usize,
    pub columns: Vec<String>,
    pub rows: Vec<DatRow>,
}

#[derive(Debug, thiserror::Error)]
pub enum DatTableError {
    #[error(transparent)]
    DatFile(#[from] DatFileError),
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
}

impl DatTableReader {
    pub fn from_graphql(schema: &str) -> Result<Self, DatTableError> {
        Ok(Self {
            tables: parse_graphql_schema(schema)?,
        })
    }

    #[must_use]
    pub fn table_names(&self) -> Vec<&str> {
        self.tables
            .iter()
            .map(|table| table.name.as_str())
            .collect()
    }

    pub fn table(&self, table_name: &str) -> Result<&GraphqlTable, DatTableError> {
        self.tables
            .iter()
            .find(|table| table.name == table_name)
            .ok_or_else(|| DatTableError::MissingTable(table_name.to_owned()))
    }

    pub fn headers(&self, table_name: &str) -> Result<Vec<DatColumnHeader>, DatTableError> {
        headers_from_graphql_table(self.table(table_name)?)
    }

    pub fn column_names(&self, table_name: &str) -> Result<Vec<String>, DatTableError> {
        Ok(self
            .headers(table_name)?
            .into_iter()
            .map(|header| header.name)
            .collect())
    }

    pub fn readable_column_names(&self, table_name: &str) -> Result<Vec<String>, DatTableError> {
        Ok(self
            .headers(table_name)?
            .into_iter()
            .filter(|header| is_readable_field_type(&header.field_type))
            .map(|header| header.name)
            .collect())
    }

    pub fn read_table(
        &self,
        bytes: &[u8],
        table_name: &str,
        columns: &[String],
        limit: Option<usize>,
    ) -> Result<DatTableRows, DatTableError> {
        let selected_columns = if columns.is_empty() {
            self.readable_column_names(table_name)?
        } else {
            columns.to_vec()
        };
        read_dat_table_with_headers(
            bytes,
            table_name,
            &self.headers(table_name)?,
            &selected_columns,
            limit,
        )
    }
}

pub fn parse_graphql_table(schema: &str, table_name: &str) -> Result<GraphqlTable, DatTableError> {
    parse_graphql_schema(schema)?
        .into_iter()
        .find(|table| table.name == table_name)
        .ok_or_else(|| DatTableError::MissingTable(table_name.to_owned()))
}

pub fn parse_graphql_schema(schema: &str) -> Result<Vec<GraphqlTable>, DatTableError> {
    let mut in_table = false;
    let mut table_name = String::new();
    let mut columns = Vec::new();
    let mut tables = Vec::new();
    for raw_line in schema.lines() {
        let line = raw_line.trim();
        if !in_table {
            if let Some(name) = parse_type_start(line) {
                in_table = true;
                name.clone_into(&mut table_name);
                if line.ends_with('}') {
                    tables.push(GraphqlTable {
                        name: table_name.clone(),
                        columns: Vec::new(),
                    });
                    table_name.clear();
                    in_table = false;
                }
            }
            continue;
        }
        if line == "}" {
            tables.push(GraphqlTable {
                name: table_name.clone(),
                columns,
            });
            table_name.clear();
            columns = Vec::new();
            in_table = false;
            continue;
        }
        if line.is_empty() || line.starts_with('#') || line.starts_with('"') {
            continue;
        }
        columns.push(parse_graphql_field(&table_name, line)?);
    }
    if in_table {
        Err(DatTableError::UnclosedTable(table_name.clone()))
    } else {
        Ok(tables)
    }
}

pub fn headers_from_graphql_table(
    table: &GraphqlTable,
) -> Result<Vec<DatColumnHeader>, DatTableError> {
    let mut offset = 0;
    let mut headers = Vec::with_capacity(table.columns.len());
    let effective_names = effective_column_names(table);
    for (column, name) in table.columns.iter().zip(effective_names) {
        let field_type = field_type_for(table, column)?;
        let field_len = field_length(&field_type)?;
        headers.push(DatColumnHeader {
            name,
            source_name: column.name.clone(),
            offset,
            field_type,
        });
        offset += field_len;
    }
    Ok(headers)
}

pub fn read_dat_table(
    bytes: &[u8],
    schema: &str,
    table_name: &str,
    columns: &[String],
    limit: Option<usize>,
) -> Result<DatTableRows, DatTableError> {
    DatTableReader::from_graphql(schema)?.read_table(bytes, table_name, columns, limit)
}

#[must_use]
pub fn table_name_from_path(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.split('.').next())
        .map(str::to_owned)
}

fn read_dat_table_with_headers(
    bytes: &[u8],
    table_name: &str,
    headers: &[DatColumnHeader],
    columns: &[String],
    limit: Option<usize>,
) -> Result<DatTableRows, DatTableError> {
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
    for header in &selected_headers {
        if !is_readable_field_type(&header.field_type) {
            return Err(DatTableError::UnsupportedType {
                column: header.name.clone(),
                type_name: field_type_label(&header.field_type),
            });
        }
    }
    let selected_columns = selected_headers
        .iter()
        .map(|header| DatColumn::from(*header))
        .collect::<Vec<_>>();
    let rows = read_projected_rows(bytes, &selected_columns, limit)?;

    Ok(DatTableRows {
        table_name: table_name.to_owned(),
        row_count: rows.row_count,
        row_length: rows.row_length,
        columns: rows.columns,
        rows: rows.rows,
    })
}

fn array_type(type_token: &str) -> (bool, &str) {
    type_token
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .map_or((false, type_token), |inner| (true, inner))
}

fn parse_type_start(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("type ")?;
    rest.split(|ch: char| ch.is_whitespace() || ch == '{')
        .find(|part| !part.is_empty())
}

fn parse_graphql_field(table_name: &str, line: &str) -> Result<GraphqlColumn, DatTableError> {
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
    Ok(GraphqlColumn {
        name: name.trim().to_owned(),
        type_name: type_name.to_owned(),
        array,
    })
}

fn effective_column_names(table: &GraphqlTable) -> Vec<String> {
    let mut counts = HashMap::<String, usize>::new();
    table
        .columns
        .iter()
        .map(|column| {
            let base_name = if column.name.is_empty() || column.name == "_" {
                generated_column_base(column)
            } else {
                column.name.clone()
            };
            let count = counts.entry(base_name.clone()).or_default();
            *count += 1;
            if *count == 1 {
                base_name
            } else {
                format!("{base_name}{count}")
            }
        })
        .collect()
}

fn generated_column_base(column: &GraphqlColumn) -> String {
    if column
        .type_name
        .chars()
        .next()
        .is_some_and(char::is_uppercase)
    {
        format!("{}Key", column.type_name)
    } else {
        "_".to_owned()
    }
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

#[cfg(test)]
mod tests {
    use super::{headers_from_graphql_table, parse_graphql_table, read_dat_table, DatTableReader};
    use crate::dat_file::DatValue;

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

    #[test]
    fn reader_generates_stable_names_for_anonymous_columns() {
        let schema = r"
            type OtherTable {
              Id: string
            }

            type Example {
              Id: string
              _: OtherTable
              _: OtherTable
              _: [OtherTable]
              _: i32
              _: i32
              _: [_]
            }
        ";
        let reader = DatTableReader::from_graphql(schema).expect("parse schema");
        let columns = reader.column_names("Example").expect("column names");
        let readable = reader
            .readable_column_names("Example")
            .expect("readable column names");

        assert_eq!(
            columns,
            vec![
                "Id".to_owned(),
                "OtherTableKey".to_owned(),
                "OtherTableKey2".to_owned(),
                "OtherTableKey3".to_owned(),
                "_".to_owned(),
                "_2".to_owned(),
                "_3".to_owned(),
            ]
        );
        assert_eq!(
            readable,
            vec![
                "Id".to_owned(),
                "OtherTableKey".to_owned(),
                "OtherTableKey2".to_owned(),
                "OtherTableKey3".to_owned(),
                "_".to_owned(),
                "_2".to_owned(),
            ]
        );
    }

    #[test]
    fn reader_can_be_reused_for_projected_datc64_rows() {
        let schema = r"
            type WorldAreas {
              Id: string
              Act: i32
              IsTown: bool
              Connections_WorldAreasKeys: [WorldAreas]
            }
        ";
        let reader = DatTableReader::from_graphql(schema).expect("parse schema");
        let bytes = synthetic_world_areas_datc64();
        let rows = reader
            .read_table(&bytes, "WorldAreas", &["Id".to_owned()], Some(1))
            .expect("read table");

        assert_eq!(rows.columns, vec!["Id".to_owned()]);
        assert_eq!(
            rows.rows[0].0,
            vec![
                ("_index".to_owned(), DatValue::Unsigned(0)),
                ("Id".to_owned(), DatValue::String("1_1_1".to_owned())),
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
