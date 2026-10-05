pub mod generated {
    #![allow(clippy::all, clippy::pedantic, unsafe_code, unused_imports)]
    include!(concat!(env!("OUT_DIR"), "/poe_layouts_generated.rs"));
}

pub use generated::poe_layouts;

pub const LAYOUT_SCHEMA_VERSION: &str = "0.1.0";

#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error("invalid layout database flatbuffer: {0}")]
    InvalidFlatbuffer(#[from] flatbuffers::InvalidFlatbuffer),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutDatabaseModel {
    pub schema_version: String,
    pub game_version: String,
    pub release_line: String,
    pub scope: String,
    pub zones: Vec<ZoneModel>,
    pub terrain_files: Vec<TerrainFileModel>,
    pub source_files: Vec<SourceFileModel>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneModel {
    pub row_index: u32,
    pub id: String,
    pub name: String,
    pub act: i32,
    pub area_level: i32,
    pub is_town: bool,
    pub topology_indices: Vec<u32>,
    pub tsi_file: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainFileKind {
    Graph,
    Tsi,
    DgrVariant,
    ArmVariant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainFileStatus {
    Candidate,
    Extracted,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerrainFileModel {
    pub logical_path: String,
    pub source: String,
    pub kind: TerrainFileKind,
    pub status: TerrainFileStatus,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFileModel {
    pub logical_path: String,
    pub output_path: String,
    pub cache_key: String,
    pub cache_path: String,
    pub cache_source: String,
    pub byte_size: u64,
    pub blake3: String,
}

/// Read a `LayoutDatabase` root from `FlatBuffers` bytes.
///
/// # Errors
///
/// Returns [`SchemaError`] when the bytes do not contain a valid
/// `LayoutDatabase` root.
pub fn root_layout_database(bytes: &[u8]) -> Result<poe_layouts::LayoutDatabase<'_>, SchemaError> {
    Ok(poe_layouts::root_as_layout_database(bytes)?)
}

#[must_use]
pub fn build_layout_database(model: &LayoutDatabaseModel) -> Vec<u8> {
    let mut builder = flatbuffers::FlatBufferBuilder::new();

    let zones = model
        .zones
        .iter()
        .map(|zone| build_zone(&mut builder, zone))
        .collect::<Vec<_>>();
    let terrain_files = model
        .terrain_files
        .iter()
        .map(|file| build_terrain_file(&mut builder, file))
        .collect::<Vec<_>>();
    let source_files = model
        .source_files
        .iter()
        .map(|file| build_source_file(&mut builder, file))
        .collect::<Vec<_>>();
    let warnings = build_strings(&mut builder, &model.warnings);

    let schema_version = builder.create_string(&model.schema_version);
    let game_version = builder.create_string(&model.game_version);
    let release_line = builder.create_string(&model.release_line);
    let scope = builder.create_string(&model.scope);
    let zones = builder.create_vector(&zones);
    let terrain_files = builder.create_vector(&terrain_files);
    let source_files = builder.create_vector(&source_files);
    let warnings = builder.create_vector(&warnings);
    let database = poe_layouts::LayoutDatabase::create(
        &mut builder,
        &poe_layouts::LayoutDatabaseArgs {
            schema_version: Some(schema_version),
            game_version: Some(game_version),
            release_line: Some(release_line),
            scope: Some(scope),
            zones: Some(zones),
            terrain_files: Some(terrain_files),
            source_files: Some(source_files),
            warnings: Some(warnings),
        },
    );
    builder.finish(database, None);
    builder.finished_data().to_vec()
}

fn build_zone<'builder>(
    builder: &mut flatbuffers::FlatBufferBuilder<'builder>,
    zone: &ZoneModel,
) -> flatbuffers::WIPOffset<poe_layouts::Zone<'builder>> {
    let id = builder.create_string(&zone.id);
    let name = builder.create_string(&zone.name);
    let tsi_file = zone
        .tsi_file
        .as_ref()
        .map(|value| builder.create_string(value));
    let topology_indices = builder.create_vector(&zone.topology_indices);
    poe_layouts::Zone::create(
        builder,
        &poe_layouts::ZoneArgs {
            row_index: zone.row_index,
            id: Some(id),
            name: Some(name),
            act: zone.act,
            area_level: zone.area_level,
            is_town: zone.is_town,
            topology_indices: Some(topology_indices),
            tsi_file,
        },
    )
}

fn build_terrain_file<'builder>(
    builder: &mut flatbuffers::FlatBufferBuilder<'builder>,
    file: &TerrainFileModel,
) -> flatbuffers::WIPOffset<poe_layouts::TerrainFile<'builder>> {
    let logical_path = builder.create_string(&file.logical_path);
    let source = builder.create_string(&file.source);
    let reason = file
        .reason
        .as_ref()
        .map(|value| builder.create_string(value));
    poe_layouts::TerrainFile::create(
        builder,
        &poe_layouts::TerrainFileArgs {
            logical_path: Some(logical_path),
            source: Some(source),
            kind: file.kind.into(),
            status: file.status.into(),
            reason,
        },
    )
}

fn build_source_file<'builder>(
    builder: &mut flatbuffers::FlatBufferBuilder<'builder>,
    file: &SourceFileModel,
) -> flatbuffers::WIPOffset<poe_layouts::SourceFile<'builder>> {
    let logical_path = builder.create_string(&file.logical_path);
    let output_path = builder.create_string(&file.output_path);
    let cache_key = builder.create_string(&file.cache_key);
    let cache_path = builder.create_string(&file.cache_path);
    let cache_source = builder.create_string(&file.cache_source);
    let blake3 = builder.create_string(&file.blake3);
    poe_layouts::SourceFile::create(
        builder,
        &poe_layouts::SourceFileArgs {
            logical_path: Some(logical_path),
            output_path: Some(output_path),
            cache_key: Some(cache_key),
            cache_path: Some(cache_path),
            cache_source: Some(cache_source),
            byte_size: file.byte_size,
            blake3: Some(blake3),
        },
    )
}

fn build_strings<'builder>(
    builder: &mut flatbuffers::FlatBufferBuilder<'builder>,
    values: &[String],
) -> Vec<flatbuffers::WIPOffset<&'builder str>> {
    values
        .iter()
        .map(|value| builder.create_string(value))
        .collect()
}

impl From<TerrainFileKind> for poe_layouts::TerrainFileKind {
    fn from(kind: TerrainFileKind) -> Self {
        match kind {
            TerrainFileKind::Graph => Self::Graph,
            TerrainFileKind::Tsi => Self::Tsi,
            TerrainFileKind::DgrVariant => Self::DgrVariant,
            TerrainFileKind::ArmVariant => Self::ArmVariant,
        }
    }
}

impl From<TerrainFileStatus> for poe_layouts::TerrainFileStatus {
    fn from(status: TerrainFileStatus) -> Self {
        match status {
            TerrainFileStatus::Candidate => Self::Candidate,
            TerrainFileStatus::Extracted => Self::Extracted,
            TerrainFileStatus::Missing => Self::Missing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_layout_database, root_layout_database, LayoutDatabaseModel, SourceFileModel,
        TerrainFileKind, TerrainFileModel, TerrainFileStatus, ZoneModel, LAYOUT_SCHEMA_VERSION,
    };

    #[test]
    fn build_and_read_layout_database() {
        let bytes = build_layout_database(&LayoutDatabaseModel {
            schema_version: LAYOUT_SCHEMA_VERSION.to_owned(),
            game_version: "3.29.3.3".to_owned(),
            release_line: "3.29".to_owned(),
            scope: "campaign-acts-1-5".to_owned(),
            zones: vec![ZoneModel {
                row_index: 7,
                id: "1_1_1".to_owned(),
                name: "The Coast".to_owned(),
                act: 1,
                area_level: 2,
                is_town: false,
                topology_indices: vec![10, 11],
                tsi_file: Some("metadata/terrain/act1/coast.tsi".to_owned()),
            }],
            terrain_files: vec![TerrainFileModel {
                logical_path: "metadata/terrain/act1/coast.dgr".to_owned(),
                source: "WorldAreas[7].TopologiesKeys".to_owned(),
                kind: TerrainFileKind::Graph,
                status: TerrainFileStatus::Missing,
                reason: Some("not found in fixture".to_owned()),
            }],
            source_files: vec![SourceFileModel {
                logical_path: "data/worldareas.datc64".to_owned(),
                output_path: ".poe-layouts/raw/files/data/worldareas.datc64".to_owned(),
                cache_key: "poe1/3.29/patches/3.29.3.3/Bundles2/example.bin".to_owned(),
                cache_path: ".poe-layouts/cache/example.bin".to_owned(),
                cache_source: "cache".to_owned(),
                byte_size: 42,
                blake3: "hash".to_owned(),
            }],
            warnings: vec!["fixture warning".to_owned()],
        });

        let database = root_layout_database(&bytes).expect("valid layout database");
        assert_eq!(database.schema_version(), Some(LAYOUT_SCHEMA_VERSION));
        assert_eq!(database.game_version(), Some("3.29.3.3"));
        assert_eq!(database.release_line(), Some("3.29"));
        assert_eq!(database.scope(), Some("campaign-acts-1-5"));
        assert_eq!(database.zones().expect("zones").len(), 1);
        assert_eq!(database.terrain_files().expect("terrain files").len(), 1);
        assert_eq!(database.source_files().expect("source files").len(), 1);
        assert_eq!(database.warnings().expect("warnings").len(), 1);
    }
}
