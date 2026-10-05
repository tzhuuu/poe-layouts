#![allow(clippy::missing_errors_doc)]

use crate::{DatRow, DatValue, GraphqlDatError, GraphqlDatSchema};

#[derive(Debug, thiserror::Error)]
pub enum DatTableError {
    #[error("missing column {table}.{column}")]
    MissingColumn { table: String, column: &'static str },
    #[error("unexpected type for {table}.{column}: expected {expected}, found {actual}")]
    UnexpectedType {
        table: String,
        column: &'static str,
        expected: &'static str,
        actual: &'static str,
    },
    #[error("{table}.{column} value does not fit {target_type}: {value}")]
    IntegerConversion {
        table: String,
        column: &'static str,
        target_type: &'static str,
        value: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum TypedDatTableError {
    #[error(transparent)]
    Graphql(#[from] GraphqlDatError),
    #[error(transparent)]
    Row(#[from] DatTableError),
}

pub trait TypedDatTableRow: Sized {
    const TABLE_NAME: &'static str;
    const COLUMNS: &'static [&'static str];

    fn from_dat_row(row: DatRowView<'_>) -> Result<Self, DatTableError>;
}

#[derive(Debug, Clone, Copy)]
pub struct DatRowView<'a> {
    table_name: &'a str,
    row: &'a DatRow,
}

impl<'a> DatRowView<'a> {
    #[must_use]
    pub fn new(table_name: &'a str, row: &'a DatRow) -> Self {
        Self { table_name, row }
    }

    pub fn row_index(&self) -> Result<usize, DatTableError> {
        self.unsigned_or_default("_index").and_then(|value| {
            usize::try_from(value)
                .map_err(|_| self.integer_conversion("_index", "usize", value.to_string()))
        })
    }

    pub fn optional_string(&self, column: &'static str) -> Result<Option<String>, DatTableError> {
        match self.value(column)? {
            DatValue::Null => Ok(None),
            DatValue::String(value) => Ok(Some(value.clone())),
            value => Err(self.unexpected_type(column, "string", value)),
        }
    }

    pub fn string_or_default(&self, column: &'static str) -> Result<String, DatTableError> {
        Ok(self.optional_string(column)?.unwrap_or_default())
    }

    pub fn non_empty_string(&self, column: &'static str) -> Result<Option<String>, DatTableError> {
        Ok(self
            .optional_string(column)?
            .filter(|value| !value.is_empty()))
    }

    pub fn integer_or_default(&self, column: &'static str) -> Result<i64, DatTableError> {
        Ok(match self.value(column)? {
            DatValue::Null => 0,
            DatValue::Integer(value) => *value,
            DatValue::Unsigned(value) => i64::try_from(*value)
                .map_err(|_| self.integer_conversion(column, "i64", value.to_string()))?,
            value => return Err(self.unexpected_type(column, "integer", value)),
        })
    }

    pub fn unsigned_or_default(&self, column: &'static str) -> Result<u64, DatTableError> {
        Ok(match self.value(column)? {
            DatValue::Null => 0,
            DatValue::Unsigned(value) => *value,
            DatValue::Integer(value) => u64::try_from(*value)
                .map_err(|_| self.integer_conversion(column, "u64", value.to_string()))?,
            value => return Err(self.unexpected_type(column, "unsigned", value)),
        })
    }

    pub fn bool_or_default(&self, column: &'static str) -> Result<bool, DatTableError> {
        Ok(match self.value(column)? {
            DatValue::Null => false,
            DatValue::Bool(value) => *value,
            value => return Err(self.unexpected_type(column, "bool", value)),
        })
    }

    pub fn unsigned_array(&self, column: &'static str) -> Result<Vec<usize>, DatTableError> {
        let value = self.value(column)?;
        let DatValue::Array(values) = value else {
            return Err(self.unexpected_type(column, "array<unsigned>", value));
        };
        values
            .iter()
            .map(|value| match value {
                DatValue::Unsigned(value) => usize::try_from(*value)
                    .map_err(|_| self.integer_conversion(column, "usize", value.to_string())),
                DatValue::Integer(value) => usize::try_from(*value)
                    .map_err(|_| self.integer_conversion(column, "usize", value.to_string())),
                value => Err(self.unexpected_type(column, "unsigned", value)),
            })
            .collect()
    }

    fn value(&self, column: &'static str) -> Result<&DatValue, DatTableError> {
        self.row
            .0
            .iter()
            .find_map(|(key, value)| (key == column).then_some(value))
            .ok_or_else(|| DatTableError::MissingColumn {
                table: self.table_name.to_owned(),
                column,
            })
    }

    fn unexpected_type(
        &self,
        column: &'static str,
        expected: &'static str,
        value: &DatValue,
    ) -> DatTableError {
        DatTableError::UnexpectedType {
            table: self.table_name.to_owned(),
            column,
            expected,
            actual: dat_value_type(value),
        }
    }

    fn integer_conversion(
        &self,
        column: &'static str,
        target_type: &'static str,
        value: String,
    ) -> DatTableError {
        DatTableError::IntegerConversion {
            table: self.table_name.to_owned(),
            column,
            target_type,
            value,
        }
    }
}

pub fn read_typed_graphql_table<T>(
    schema: &GraphqlDatSchema,
    bytes: &[u8],
    limit: Option<usize>,
) -> Result<Vec<T>, TypedDatTableError>
where
    T: TypedDatTableRow,
{
    let columns = T::COLUMNS
        .iter()
        .map(|column| (*column).to_owned())
        .collect::<Vec<_>>();
    let rows = schema.read_table(bytes, T::TABLE_NAME, &columns, limit)?;
    rows.rows
        .iter()
        .map(|row| T::from_dat_row(DatRowView::new(T::TABLE_NAME, row)).map_err(Into::into))
        .collect()
}

#[must_use]
pub fn dat_value_type(value: &DatValue) -> &'static str {
    match value {
        DatValue::Null => "null",
        DatValue::Bool(_) => "bool",
        DatValue::Integer(_) => "integer",
        DatValue::Unsigned(_) => "unsigned",
        DatValue::Float(_) => "float",
        DatValue::String(_) => "string",
        DatValue::Array(_) => "array",
    }
}

#[cfg(test)]
mod tests {
    use super::{read_typed_graphql_table, DatRowView, DatTableError, TypedDatTableRow};
    use crate::{DatRow, DatValue, GraphqlDatSchema};

    #[test]
    fn row_view_reads_expected_value_types() {
        let row = DatRow(vec![
            ("Id".to_owned(), DatValue::String("1_1_1".to_owned())),
            ("Act".to_owned(), DatValue::Integer(1)),
            ("IsTown".to_owned(), DatValue::Bool(false)),
            (
                "TopologiesKeys".to_owned(),
                DatValue::Array(vec![DatValue::Unsigned(10), DatValue::Integer(11)]),
            ),
        ]);
        let view = DatRowView::new("WorldAreas", &row);

        assert_eq!(view.string_or_default("Id").expect("id"), "1_1_1");
        assert_eq!(view.integer_or_default("Act").expect("act"), 1);
        assert!(!view.bool_or_default("IsTown").expect("town"));
        assert_eq!(
            view.unsigned_array("TopologiesKeys").expect("topologies"),
            vec![10, 11]
        );
    }

    #[test]
    fn row_view_reports_type_mismatches_with_table_context() {
        let row = DatRow(vec![("Act".to_owned(), DatValue::String("one".to_owned()))]);
        let error = DatRowView::new("WorldAreas", &row)
            .integer_or_default("Act")
            .expect_err("type mismatch");

        assert_eq!(
            error.to_string(),
            "unexpected type for WorldAreas.Act: expected integer, found string"
        );
    }

    #[test]
    fn typed_graphql_table_reader_maps_projected_rows() {
        #[derive(Debug, PartialEq, Eq)]
        struct ExampleArea {
            row_index: usize,
            id: String,
            act: i64,
        }

        impl TypedDatTableRow for ExampleArea {
            const TABLE_NAME: &'static str = "WorldAreas";
            const COLUMNS: &'static [&'static str] = &["Id", "Act"];

            fn from_dat_row(row: DatRowView<'_>) -> Result<Self, DatTableError> {
                Ok(Self {
                    row_index: row.row_index()?,
                    id: row.string_or_default("Id")?,
                    act: row.integer_or_default("Act")?,
                })
            }
        }

        let schema = GraphqlDatSchema::parse(
            r"
                type WorldAreas {
                  Id: string
                  Act: i32
                }
            ",
        )
        .expect("schema");
        let rows =
            read_typed_graphql_table::<ExampleArea>(&schema, &synthetic_world_areas_datc64(), None)
                .expect("typed rows");

        assert_eq!(
            rows,
            vec![
                ExampleArea {
                    row_index: 0,
                    id: "1_1_1".to_owned(),
                    act: 1,
                },
                ExampleArea {
                    row_index: 1,
                    id: "1_2_1".to_owned(),
                    act: 2,
                },
            ]
        );
    }

    fn synthetic_world_areas_datc64() -> Vec<u8> {
        let mut variable = vec![0xbb; 8];
        let id0 = push_utf16_string(&mut variable, "1_1_1");
        let id1 = push_utf16_string(&mut variable, "1_2_1");

        let mut fixed = Vec::new();
        push_world_area_row(&mut fixed, id0, 1);
        push_world_area_row(&mut fixed, id1, 2);

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&2_u32.to_le_bytes());
        bytes.extend_from_slice(&fixed);
        bytes.extend_from_slice(&variable);
        bytes
    }

    fn push_world_area_row(fixed: &mut Vec<u8>, id_offset: usize, act: i32) {
        fixed.extend_from_slice(
            &u32::try_from(id_offset)
                .expect("offset fits u32")
                .to_le_bytes(),
        );
        fixed.extend_from_slice(&0_u32.to_le_bytes());
        fixed.extend_from_slice(&act.to_le_bytes());
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
