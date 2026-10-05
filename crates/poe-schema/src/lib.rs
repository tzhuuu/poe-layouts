pub mod generated {
    #![allow(clippy::all, unsafe_code, unused_imports)]
    include!(concat!(env!("OUT_DIR"), "/poe_layouts_generated.rs"));
}

pub use generated::poe_layouts;

#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error("invalid layout database flatbuffer: {0}")]
    InvalidFlatbuffer(#[from] flatbuffers::InvalidFlatbuffer),
}

pub fn root_layout_database(
    bytes: &[u8],
) -> Result<poe_layouts::LayoutDatabase<'_>, SchemaError> {
    Ok(poe_layouts::root_as_layout_database(bytes)?)
}
