use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use poe_ggpk::{
    BundleDecompressor, CacheMode, DatRow, DatValue, DiskCache, ExtractedLogicalFile,
    GraphqlDatError, GraphqlDatRows, GraphqlDatSchema, PatchCdnSource, PatchClient,
    PatchClientError,
};
use poe_schema::{
    build_layout_database, root_layout_database, LayoutDatabaseModel, SchemaError, SourceFileModel,
    TerrainFileKind as LayoutTerrainFileKind, TerrainFileModel, TerrainFileStatus, ZoneModel,
    LAYOUT_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};

pub const CAMPAIGN_ACTS_ONE_TO_FIVE_SCOPE: &str = "campaign-acts-1-5";

#[derive(Debug, Clone)]
pub struct CampaignScrapeRequest {
    pub source: PatchCdnSource,
    pub cache: DiskCache,
    pub mode: CacheMode,
    pub schema_path: PathBuf,
    pub out_dir: PathBuf,
    pub layout_db_out: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CampaignScrapeOutput {
    pub manifest_path: PathBuf,
    pub layout_db_path: Option<PathBuf>,
    pub manifest: CampaignScrapeManifest,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CampaignScrapeManifest {
    pub scope: String,
    pub patch_version: String,
    pub release_line: String,
    pub schema: PathBuf,
    pub out_dir: PathBuf,
    pub tables: Vec<ExtractedLogicalFile>,
    pub selected_areas: Vec<CampaignAreaSummary>,
    pub candidate_files: Vec<TerrainCandidate>,
    pub extracted_files: Vec<ExtractedLogicalFile>,
    pub missing_files: Vec<MissingTerrainCandidate>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CampaignAreaSummary {
    pub row_index: usize,
    pub id: String,
    pub name: String,
    pub act: i64,
    pub is_town: bool,
    pub area_level: i64,
    pub topology_indices: Vec<usize>,
    pub tsi_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TopologySummary {
    pub row_index: usize,
    pub id: String,
    pub graph_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct TerrainCandidate {
    pub logical_path: String,
    pub source: String,
    pub kind: TerrainCandidateKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TerrainCandidateKind {
    Graph,
    Tsi,
    DgrVariant,
    ArmVariant,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MissingTerrainCandidate {
    pub logical_path: String,
    pub source: String,
    pub kind: TerrainCandidateKind,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LayoutDbSummary {
    pub schema_version: Option<String>,
    pub game_version: Option<String>,
    pub release_line: Option<String>,
    pub scope: Option<String>,
    pub counts: LayoutDbCounts,
    pub sample_zones: Vec<LayoutDbZoneSample>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct LayoutDbCounts {
    pub zones: usize,
    pub terrain_files: usize,
    pub source_files: usize,
    pub warnings: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct LayoutDbZoneSample {
    pub id: Option<String>,
    pub name: Option<String>,
    pub act: i32,
    pub area_level: i32,
    pub is_town: bool,
    pub topology_count: usize,
    pub tsi_file: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CampaignScrapeError {
    #[error(transparent)]
    PatchClient(#[from] PatchClientError),
    #[error(transparent)]
    GraphqlDat(#[from] GraphqlDatError),
    #[error(transparent)]
    Schema(#[from] SchemaError),
    #[error("io error for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("json error for {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("{field} does not fit {target_type}: {value}")]
    IntegerConversion {
        field: &'static str,
        target_type: &'static str,
        value: String,
    },
}

/// Build the current Acts 1-5 raw scrape corpus and optional app artifact.
///
/// # Errors
///
/// Returns [`CampaignScrapeError`] when patch data cannot be loaded, required
/// table files cannot be extracted, the GraphQL table schema cannot parse the
/// extracted tables, or output files cannot be written.
#[allow(clippy::too_many_lines)]
pub fn scrape_campaign_acts_one_to_five<D>(
    request: &CampaignScrapeRequest,
    decompressor: &mut D,
) -> Result<CampaignScrapeOutput, CampaignScrapeError>
where
    D: BundleDecompressor,
{
    let client = PatchClient::new(request.source.clone(), request.cache.clone(), request.mode);
    let client_index = client.load_index(decompressor)?;
    let logical_paths = client_index
        .logical_paths
        .iter()
        .cloned()
        .collect::<HashSet<_>>();

    let files_dir = request.out_dir.join("files");
    create_dir_all(&files_dir)?;

    let table_paths = ["data/worldareas.datc64", "data/topologies.datc64"];
    let mut extracted_tables = Vec::new();
    for logical_path in table_paths {
        let output_path = raw_output_path(&files_dir, logical_path);
        let extracted = client.extract_logical_file(
            &client_index.index,
            logical_path,
            &output_path,
            decompressor,
        )?;
        extracted_tables.push(extracted);
    }

    let schema_text = read_to_string(&request.schema_path)?;
    let schema = GraphqlDatSchema::parse(&schema_text)?;
    let world_areas_bytes = read(raw_output_path(&files_dir, "data/worldareas.datc64"))?;
    let topologies_bytes = read(raw_output_path(&files_dir, "data/topologies.datc64"))?;

    let world_areas = schema.read_table(
        &world_areas_bytes,
        "WorldAreas",
        &[
            "Id".to_owned(),
            "Name".to_owned(),
            "Act".to_owned(),
            "IsTown".to_owned(),
            "AreaLevel".to_owned(),
            "IsMapArea".to_owned(),
            "TopologiesKeys".to_owned(),
            "TSIFile".to_owned(),
        ],
        None,
    )?;
    let topologies = schema.read_table(
        &topologies_bytes,
        "Topologies",
        &["Id".to_owned(), "DGRFile".to_owned()],
        None,
    )?;

    let topology_by_index = topology_summaries(&topologies)?;
    let selected_areas = campaign_area_summaries(&world_areas)?;
    let mut candidates = terrain_candidates(&selected_areas, &topology_by_index);
    candidates.sort();
    candidates.dedup();

    let mut extracted_files = Vec::new();
    let mut missing_files = Vec::new();
    for candidate in &candidates {
        if logical_paths.contains(&candidate.logical_path) {
            let output_path = raw_output_path(&files_dir, &candidate.logical_path);
            match client.extract_logical_file(
                &client_index.index,
                &candidate.logical_path,
                &output_path,
                decompressor,
            ) {
                Ok(extracted) => extracted_files.push(extracted),
                Err(error) => missing_files.push(MissingTerrainCandidate {
                    logical_path: candidate.logical_path.clone(),
                    source: candidate.source.clone(),
                    kind: candidate.kind,
                    reason: error.to_string(),
                }),
            }
        } else {
            missing_files.push(MissingTerrainCandidate {
                logical_path: candidate.logical_path.clone(),
                source: candidate.source.clone(),
                kind: candidate.kind,
                reason: "not found in patch index".to_owned(),
            });
        }
    }

    let manifest = CampaignScrapeManifest {
        scope: CAMPAIGN_ACTS_ONE_TO_FIVE_SCOPE.to_owned(),
        patch_version: request.source.patch_version.clone(),
        release_line: request.source.release_line(),
        schema: request.schema_path.clone(),
        out_dir: request.out_dir.clone(),
        tables: extracted_tables,
        selected_areas,
        candidate_files: candidates,
        extracted_files,
        missing_files,
        warnings: vec![
            "This milestone only resolves table-declared graph and TSI paths plus simple .dgr/.arm path variants; deeper terrain dependencies require parsing the extracted graph files.".to_owned(),
            "Campaign selection currently uses WorldAreas Act 1-5, excludes map areas, and skips the NULL sentinel row.".to_owned(),
        ],
    };

    let manifest_path = request.out_dir.join("manifest.json");
    write_json(&manifest_path, &manifest)?;
    let layout_db_path = if let Some(path) = &request.layout_db_out {
        write_layout_database(path, &campaign_manifest_to_layout_model(&manifest)?)?;
        Some(path.clone())
    } else {
        None
    };

    Ok(CampaignScrapeOutput {
        manifest_path,
        layout_db_path,
        manifest,
    })
}

/// Convert a raw campaign scrape manifest into the app-facing layout model.
///
/// # Errors
///
/// Returns [`CampaignScrapeError`] when a row index, act, area level, or
/// topology index cannot fit the current `FlatBuffers` field type.
pub fn campaign_manifest_to_layout_model(
    manifest: &CampaignScrapeManifest,
) -> Result<LayoutDatabaseModel, CampaignScrapeError> {
    let extracted_paths = manifest
        .extracted_files
        .iter()
        .map(|file| file.logical_path.clone())
        .collect::<HashSet<_>>();
    let missing_by_path = manifest
        .missing_files
        .iter()
        .map(|file| (file.logical_path.clone(), file.reason.clone()))
        .collect::<HashMap<_, _>>();

    let zones = manifest
        .selected_areas
        .iter()
        .map(zone_model)
        .collect::<Result<Vec<_>, _>>()?;
    let terrain_files = manifest
        .candidate_files
        .iter()
        .map(|candidate| {
            let (status, reason) = if extracted_paths.contains(&candidate.logical_path) {
                (TerrainFileStatus::Extracted, None)
            } else if let Some(reason) = missing_by_path.get(&candidate.logical_path) {
                (TerrainFileStatus::Missing, Some(reason.clone()))
            } else {
                (TerrainFileStatus::Candidate, None)
            };
            TerrainFileModel {
                logical_path: candidate.logical_path.clone(),
                source: candidate.source.clone(),
                kind: layout_terrain_file_kind(candidate.kind),
                status,
                reason,
            }
        })
        .collect();
    let source_files = manifest
        .tables
        .iter()
        .chain(manifest.extracted_files.iter())
        .map(source_file_model)
        .collect();

    Ok(LayoutDatabaseModel {
        schema_version: LAYOUT_SCHEMA_VERSION.to_owned(),
        game_version: manifest.patch_version.clone(),
        release_line: manifest.release_line.clone(),
        scope: manifest.scope.clone(),
        zones,
        terrain_files,
        source_files,
        warnings: manifest.warnings.clone(),
    })
}

/// Write a layout model as a `FlatBuffers` artifact.
///
/// # Errors
///
/// Returns [`CampaignScrapeError`] when the output directory or artifact cannot
/// be written.
pub fn write_layout_database(
    out: &Path,
    model: &LayoutDatabaseModel,
) -> Result<(), CampaignScrapeError> {
    let bytes = build_layout_database(model);
    if let Some(parent) = out.parent() {
        create_dir_all(parent)?;
    }
    let parent = out.parent().unwrap_or_else(|| Path::new("."));
    let mut temp =
        tempfile::NamedTempFile::new_in(parent).map_err(|source| CampaignScrapeError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    temp.write_all(&bytes)
        .map_err(|source| CampaignScrapeError::Io {
            path: out.to_path_buf(),
            source,
        })?;
    temp.persist(out)
        .map_err(|error| error.error)
        .map_err(|source| CampaignScrapeError::Io {
            path: out.to_path_buf(),
            source,
        })?;
    Ok(())
}

/// Read an app-facing `FlatBuffers` artifact and return a compact summary.
///
/// # Errors
///
/// Returns [`CampaignScrapeError`] when the file cannot be read or does not
/// contain a valid `LayoutDatabase`.
pub fn inspect_layout_database(input: &Path) -> Result<LayoutDbSummary, CampaignScrapeError> {
    let bytes = read(input)?;
    let database = root_layout_database(&bytes)?;
    let zones = database.zones();
    let terrain_files = database.terrain_files();
    let source_files = database.source_files();
    let warnings = database.warnings();
    let mut sample_zones = Vec::new();
    if let Some(zones) = zones {
        sample_zones.extend(zones.iter().take(5).map(|zone| LayoutDbZoneSample {
            id: zone.id().map(str::to_owned),
            name: zone.name().map(str::to_owned),
            act: zone.act(),
            area_level: zone.area_level(),
            is_town: zone.is_town(),
            topology_count: zone.topology_indices().map_or(0, |indices| indices.len()),
            tsi_file: zone.tsi_file().map(str::to_owned),
        }));
    }

    Ok(LayoutDbSummary {
        schema_version: database.schema_version().map(str::to_owned),
        game_version: database.game_version().map(str::to_owned),
        release_line: database.release_line().map(str::to_owned),
        scope: database.scope().map(str::to_owned),
        counts: LayoutDbCounts {
            zones: zones.map_or(0, |items| items.len()),
            terrain_files: terrain_files.map_or(0, |items| items.len()),
            source_files: source_files.map_or(0, |items| items.len()),
            warnings: warnings.map_or(0, |items| items.len()),
        },
        sample_zones,
    })
}

fn topology_summaries(
    rows: &GraphqlDatRows,
) -> Result<HashMap<usize, TopologySummary>, CampaignScrapeError> {
    rows.rows
        .iter()
        .map(|row| {
            let row_index = row_unsigned(row, "_index")
                .ok_or(CampaignScrapeError::IntegerConversion {
                    field: "Topologies._index",
                    target_type: "usize",
                    value: "<missing>".to_owned(),
                })?
                .try_into()
                .map_err(|_| CampaignScrapeError::IntegerConversion {
                    field: "Topologies._index",
                    target_type: "usize",
                    value: row_unsigned(row, "_index").unwrap_or_default().to_string(),
                })?;
            Ok((
                row_index,
                TopologySummary {
                    row_index,
                    id: row_string(row, "Id").unwrap_or_default(),
                    graph_file: non_empty_string(row, "DGRFile"),
                },
            ))
        })
        .collect()
}

fn campaign_area_summaries(
    rows: &GraphqlDatRows,
) -> Result<Vec<CampaignAreaSummary>, CampaignScrapeError> {
    let mut areas = Vec::new();
    for row in &rows.rows {
        let id = row_string(row, "Id").unwrap_or_default();
        if id == "NULL" || id.is_empty() {
            continue;
        }
        let act = row_integer(row, "Act").unwrap_or_default();
        if !(1..=5).contains(&act) {
            continue;
        }
        if !id.starts_with(&format!("1_{act}")) {
            continue;
        }
        if row_bool(row, "IsMapArea").unwrap_or(false) {
            continue;
        }
        let row_index = row_unsigned(row, "_index")
            .ok_or(CampaignScrapeError::IntegerConversion {
                field: "WorldAreas._index",
                target_type: "usize",
                value: "<missing>".to_owned(),
            })?
            .try_into()
            .map_err(|_| CampaignScrapeError::IntegerConversion {
                field: "WorldAreas._index",
                target_type: "usize",
                value: row_unsigned(row, "_index").unwrap_or_default().to_string(),
            })?;
        areas.push(CampaignAreaSummary {
            row_index,
            id,
            name: row_string(row, "Name").unwrap_or_default(),
            act,
            is_town: row_bool(row, "IsTown").unwrap_or(false),
            area_level: row_integer(row, "AreaLevel").unwrap_or_default(),
            topology_indices: row_array_unsigned(row, "TopologiesKeys"),
            tsi_file: non_empty_string(row, "TSIFile").map(normalize_logical_path),
        });
    }
    Ok(areas)
}

fn terrain_candidates(
    areas: &[CampaignAreaSummary],
    topology_by_index: &HashMap<usize, TopologySummary>,
) -> Vec<TerrainCandidate> {
    let mut candidates = Vec::new();
    for area in areas {
        if let Some(tsi_file) = &area.tsi_file {
            candidates.push(TerrainCandidate {
                logical_path: tsi_file.clone(),
                source: format!("WorldAreas[{}].TSIFile {}", area.row_index, area.id),
                kind: TerrainCandidateKind::Tsi,
            });
        }
        for topology_index in &area.topology_indices {
            let Some(topology) = topology_by_index.get(topology_index) else {
                continue;
            };
            let Some(graph_file) = &topology.graph_file else {
                continue;
            };
            let normalized_graph = normalize_logical_path(graph_file);
            let source = format!(
                "WorldAreas[{}].TopologiesKeys -> Topologies[{}] {}",
                area.row_index, topology.row_index, topology.id
            );
            candidates.push(TerrainCandidate {
                logical_path: normalized_graph.clone(),
                source: source.clone(),
                kind: TerrainCandidateKind::Graph,
            });
            if let Some(dgr_path) =
                replace_extension(&normalized_graph, "dgr").filter(|path| path != &normalized_graph)
            {
                candidates.push(TerrainCandidate {
                    logical_path: dgr_path,
                    source: source.clone(),
                    kind: TerrainCandidateKind::DgrVariant,
                });
            }
            if let Some(arm_path) = replace_extension(&normalized_graph, "arm") {
                candidates.push(TerrainCandidate {
                    logical_path: arm_path,
                    source,
                    kind: TerrainCandidateKind::ArmVariant,
                });
            }
        }
    }
    candidates
}

fn zone_model(area: &CampaignAreaSummary) -> Result<ZoneModel, CampaignScrapeError> {
    Ok(ZoneModel {
        row_index: convert_u32("area row index", area.row_index)?,
        id: area.id.clone(),
        name: area.name.clone(),
        act: convert_i32("act", area.act)?,
        area_level: convert_i32("area level", area.area_level)?,
        is_town: area.is_town,
        topology_indices: area
            .topology_indices
            .iter()
            .map(|index| convert_u32("topology index", *index))
            .collect::<Result<Vec<_>, _>>()?,
        tsi_file: area.tsi_file.clone(),
    })
}

fn layout_terrain_file_kind(kind: TerrainCandidateKind) -> LayoutTerrainFileKind {
    match kind {
        TerrainCandidateKind::Graph => LayoutTerrainFileKind::Graph,
        TerrainCandidateKind::Tsi => LayoutTerrainFileKind::Tsi,
        TerrainCandidateKind::DgrVariant => LayoutTerrainFileKind::DgrVariant,
        TerrainCandidateKind::ArmVariant => LayoutTerrainFileKind::ArmVariant,
    }
}

fn source_file_model(file: &ExtractedLogicalFile) -> SourceFileModel {
    SourceFileModel {
        logical_path: file.logical_path.clone(),
        output_path: file.output_path.display().to_string(),
        cache_key: file.cache_key.clone(),
        cache_path: file.cache_path.display().to_string(),
        cache_source: file.cache_source.clone(),
        byte_size: file.byte_len,
        blake3: file.blake3.clone(),
    }
}

fn raw_output_path(root: &Path, logical_path: &str) -> PathBuf {
    logical_path
        .split('/')
        .filter(|part| !part.is_empty() && *part != "." && *part != "..")
        .fold(root.to_path_buf(), |path, part| path.join(part))
}

fn normalize_logical_path(path: impl AsRef<str>) -> String {
    path.as_ref().replace('\\', "/").to_lowercase()
}

fn replace_extension(path: &str, extension: &str) -> Option<String> {
    let (base, _) = path.rsplit_once('.')?;
    Some(format!("{base}.{extension}"))
}

fn row_value<'a>(row: &'a DatRow, name: &str) -> Option<&'a DatValue> {
    row.0
        .iter()
        .find_map(|(key, value)| (key == name).then_some(value))
}

fn row_string(row: &DatRow, name: &str) -> Option<String> {
    match row_value(row, name)? {
        DatValue::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn non_empty_string(row: &DatRow, name: &str) -> Option<String> {
    row_string(row, name).filter(|value| !value.is_empty())
}

fn row_integer(row: &DatRow, name: &str) -> Option<i64> {
    match row_value(row, name)? {
        DatValue::Integer(value) => Some(*value),
        DatValue::Unsigned(value) => i64::try_from(*value).ok(),
        _ => None,
    }
}

fn row_unsigned(row: &DatRow, name: &str) -> Option<u64> {
    match row_value(row, name)? {
        DatValue::Unsigned(value) => Some(*value),
        DatValue::Integer(value) => u64::try_from(*value).ok(),
        _ => None,
    }
}

fn row_bool(row: &DatRow, name: &str) -> Option<bool> {
    match row_value(row, name)? {
        DatValue::Bool(value) => Some(*value),
        _ => None,
    }
}

fn row_array_unsigned(row: &DatRow, name: &str) -> Vec<usize> {
    match row_value(row, name) {
        Some(DatValue::Array(values)) => values
            .iter()
            .filter_map(|value| match value {
                DatValue::Unsigned(value) => usize::try_from(*value).ok(),
                DatValue::Integer(value) => usize::try_from(*value).ok(),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn convert_u32(field: &'static str, value: usize) -> Result<u32, CampaignScrapeError> {
    u32::try_from(value).map_err(|_| CampaignScrapeError::IntegerConversion {
        field,
        target_type: "u32",
        value: value.to_string(),
    })
}

fn convert_i32(field: &'static str, value: i64) -> Result<i32, CampaignScrapeError> {
    i32::try_from(value).map_err(|_| CampaignScrapeError::IntegerConversion {
        field,
        target_type: "i32",
        value: value.to_string(),
    })
}

fn read(path: impl AsRef<Path>) -> Result<Vec<u8>, CampaignScrapeError> {
    let path = path.as_ref();
    std::fs::read(path).map_err(|source| CampaignScrapeError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn read_to_string(path: &Path) -> Result<String, CampaignScrapeError> {
    std::fs::read_to_string(path).map_err(|source| CampaignScrapeError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn create_dir_all(path: &Path) -> Result<(), CampaignScrapeError> {
    std::fs::create_dir_all(path).map_err(|source| CampaignScrapeError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), CampaignScrapeError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|source| CampaignScrapeError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    std::fs::write(path, bytes).map_err(|source| CampaignScrapeError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use poe_schema::{
        LayoutDatabaseModel, SourceFileModel, TerrainFileKind, TerrainFileModel, TerrainFileStatus,
        ZoneModel, LAYOUT_SCHEMA_VERSION,
    };

    use super::{inspect_layout_database, write_layout_database};

    #[test]
    fn inspect_layout_database_summarizes_flatbuffer_artifact() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("layouts.bin");
        write_layout_database(
            &path,
            &LayoutDatabaseModel {
                schema_version: LAYOUT_SCHEMA_VERSION.to_owned(),
                game_version: "3.29.3.3".to_owned(),
                release_line: "3.29".to_owned(),
                scope: "campaign-acts-1-5".to_owned(),
                zones: vec![ZoneModel {
                    row_index: 1,
                    id: "1_1_1".to_owned(),
                    name: "The Twilight Strand".to_owned(),
                    act: 1,
                    area_level: 1,
                    is_town: false,
                    topology_indices: vec![10, 11, 12],
                    tsi_file: None,
                }],
                terrain_files: vec![TerrainFileModel {
                    logical_path: "metadata/terrain/act1/test.dgr".to_owned(),
                    source: "fixture".to_owned(),
                    kind: TerrainFileKind::Graph,
                    status: TerrainFileStatus::Missing,
                    reason: Some("not cached".to_owned()),
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
            },
        )
        .expect("write layout database");

        let summary = inspect_layout_database(&path).expect("inspect layout database");
        assert_eq!(
            summary.schema_version.as_deref(),
            Some(LAYOUT_SCHEMA_VERSION)
        );
        assert_eq!(summary.game_version.as_deref(), Some("3.29.3.3"));
        assert_eq!(summary.counts.zones, 1);
        assert_eq!(summary.counts.terrain_files, 1);
        assert_eq!(summary.sample_zones[0].topology_count, 3);
    }
}
