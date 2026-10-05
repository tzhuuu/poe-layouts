use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use pather_schema::{
    build_layout_database, root_layout_database, LayoutDatabaseModel, SchemaError, SourceFileModel,
    TerrainFileKind as LayoutTerrainFileKind, TerrainFileModel, TerrainFileStatus, ZoneModel,
    LAYOUT_SCHEMA_VERSION,
};
use poe_content::{
    BundleDecompressor, CacheMode, DiskCache, ExtractedLogicalFile, LogicalFileExtractRequest,
    PatchCdnSource, PatchClient, PatchClientError,
};
use poe_dat::{
    read_typed_graphql_table, DatRowView, DatTableError, GraphqlDatError, GraphqlDatSchema,
    TypedDatTableError, TypedDatTableRow,
};
use serde::{Deserialize, Serialize};

pub const CAMPAIGN_ACTS_ONE_TO_FIVE_SCOPE: &str = "campaign-acts-1-5";
const EXCLUDED_TERRAIN_PREFIXES: &[&str] = &["metadata/terrain/leagues/"];

#[derive(Debug, Clone)]
pub struct CampaignScrapeRequest {
    pub source: PatchCdnSource,
    pub cache: DiskCache,
    pub mode: CacheMode,
    pub schema_path: PathBuf,
    pub out_dir: PathBuf,
    pub layout_db_out: Option<PathBuf>,
    pub raw_folder_prefixes: Vec<String>,
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
    pub terrain_folders: Vec<TerrainFolderSummary>,
    pub extracted_files: Vec<ExtractedLogicalFile>,
    pub folder_files: Vec<ExtractedLogicalFile>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct TerrainFolderSummary {
    pub logical_path: String,
    pub source: String,
    pub file_count: usize,
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
    TypedDatTable(#[from] TypedDatTableError),
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
    eprintln!(
        "loading patch index for {} ({})",
        request.source.patch_version,
        request.source.release_line()
    );
    let client_index = client.load_index(decompressor)?;
    eprintln!(
        "loaded index with {} logical paths",
        client_index.logical_paths.len()
    );
    let logical_paths = client_index
        .logical_paths
        .iter()
        .cloned()
        .collect::<HashSet<_>>();

    let files_dir = request.out_dir.join("files");
    create_dir_all(&files_dir)?;

    let table_paths = ["data/worldareas.datc64", "data/topologies.datc64"];
    let mut extracted_tables = Vec::new();
    eprintln!("extracting DAT tables");
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

    let world_areas = read_typed_graphql_table::<WorldAreaRow>(&schema, &world_areas_bytes, None)?;
    let topologies = read_typed_graphql_table::<TopologyRow>(&schema, &topologies_bytes, None)?;

    let topology_by_index = topology_summaries(&topologies);
    let selected_areas = campaign_area_summaries(&world_areas);
    let (mut candidates, skipped_candidates) =
        terrain_candidates(&selected_areas, &topology_by_index, &logical_paths);
    candidates.sort();
    candidates.dedup();
    eprintln!(
        "selected {} campaign areas and {} table-declared terrain candidates",
        selected_areas.len(),
        candidates.len()
    );

    let mut warnings = vec![
        "This milestone resolves table-declared graph and TSI paths that are present in the patch index; deeper room dependencies require parsing extracted graph files.".to_owned(),
        "Campaign selection currently uses WorldAreas Act 1-5, excludes map areas, and skips the NULL sentinel row.".to_owned(),
    ];

    eprintln!("indexing terrain parent folders");
    let terrain_corpus = terrain_folder_corpus(
        &candidates,
        &request.raw_folder_prefixes,
        &client_index.logical_paths,
    );
    let terrain_folders = terrain_corpus.folders;
    eprintln!(
        "discovered {} terrain folders for raw corpus extraction",
        terrain_folders.len()
    );
    let folder_file_paths = terrain_corpus.files;

    let mut folder_files = Vec::new();
    let mut folder_file_failures = HashMap::new();
    let folder_file_count = folder_file_paths.len();
    eprintln!("extracting {folder_file_count} raw terrain folder files");
    let folder_requests = folder_file_paths
        .iter()
        .map(|logical_path| LogicalFileExtractRequest {
            logical_path: logical_path.clone(),
            output_path: raw_output_path(&files_dir, logical_path),
        })
        .collect::<Vec<_>>();
    match client.extract_logical_files(&client_index.index, &folder_requests, decompressor) {
        Ok(extracted) => folder_files = extracted,
        Err(error) => {
            warnings.push(format!(
                "Batched folder extraction failed; retrying individual files: {}",
                error
            ));
            for (index, logical_path) in folder_file_paths.iter().enumerate() {
                if index == 0 || (index + 1) % 25 == 0 || index + 1 == folder_file_count {
                    eprintln!(
                        "extracting raw terrain folder file {}/{}: {}",
                        index + 1,
                        folder_file_count,
                        logical_path
                    );
                }
                let output_path = raw_output_path(&files_dir, logical_path);
                match client.extract_logical_file(
                    &client_index.index,
                    logical_path,
                    &output_path,
                    decompressor,
                ) {
                    Ok(extracted) => folder_files.push(extracted),
                    Err(error) => {
                        let reason = error.to_string();
                        folder_file_failures.insert(logical_path.clone(), reason.clone());
                        warnings.push(format!(
                            "Failed to extract folder file {}: {}",
                            logical_path, reason
                        ));
                    }
                }
            }
        }
    }

    let folder_files_by_path = folder_files
        .iter()
        .map(|file| (file.logical_path.clone(), file.clone()))
        .collect::<HashMap<_, _>>();
    let mut extracted_files = Vec::new();
    let mut missing_files = Vec::new();
    for candidate in &candidates {
        if let Some(extracted) = folder_files_by_path.get(&candidate.logical_path) {
            extracted_files.push(extracted.clone());
        } else {
            missing_files.push(MissingTerrainCandidate {
                logical_path: candidate.logical_path.clone(),
                source: candidate.source.clone(),
                kind: candidate.kind,
                reason: folder_file_failures
                    .get(&candidate.logical_path)
                    .cloned()
                    .unwrap_or_else(|| {
                        if logical_paths.contains(&candidate.logical_path) {
                            "not extracted from terrain folder corpus".to_owned()
                        } else {
                            "not found in patch index".to_owned()
                        }
                    }),
            });
        }
    }

    if !skipped_candidates.is_empty() {
        warnings.push(format!(
            "Skipped {} table-declared terrain paths that were not present in the patch index.",
            skipped_candidates.len()
        ));
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
        terrain_folders,
        extracted_files,
        folder_files,
        missing_files,
        warnings,
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
    let mut source_files_by_path = BTreeMap::new();
    for file in manifest
        .tables
        .iter()
        .chain(manifest.extracted_files.iter())
        .chain(manifest.folder_files.iter())
    {
        source_files_by_path
            .entry(file.logical_path.clone())
            .or_insert_with(|| source_file_model(file));
    }
    let source_files = source_files_by_path.into_values().collect();

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

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorldAreaRow {
    row_index: usize,
    id: String,
    name: String,
    act: i64,
    is_town: bool,
    area_level: i64,
    is_map_area: bool,
    topology_indices: Vec<usize>,
    tsi_file: Option<String>,
}

impl TypedDatTableRow for WorldAreaRow {
    const TABLE_NAME: &'static str = "WorldAreas";
    const COLUMNS: &'static [&'static str] = &[
        "Id",
        "Name",
        "Act",
        "IsTown",
        "AreaLevel",
        "IsMapArea",
        "TopologiesKeys",
        "TSIFile",
    ];

    fn from_dat_row(row: DatRowView<'_>) -> Result<Self, DatTableError> {
        Ok(Self {
            row_index: row.row_index()?,
            id: row.string_or_default("Id")?,
            name: row.string_or_default("Name")?,
            act: row.integer_or_default("Act")?,
            is_town: row.bool_or_default("IsTown")?,
            area_level: row.integer_or_default("AreaLevel")?,
            is_map_area: row.bool_or_default("IsMapArea")?,
            topology_indices: row.unsigned_array("TopologiesKeys")?,
            tsi_file: row.non_empty_string("TSIFile")?.map(normalize_logical_path),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TopologyRow {
    row_index: usize,
    id: String,
    graph_file: Option<String>,
}

impl TypedDatTableRow for TopologyRow {
    const TABLE_NAME: &'static str = "Topologies";
    const COLUMNS: &'static [&'static str] = &["Id", "DGRFile"];

    fn from_dat_row(row: DatRowView<'_>) -> Result<Self, DatTableError> {
        Ok(Self {
            row_index: row.row_index()?,
            id: row.string_or_default("Id")?,
            graph_file: row.non_empty_string("DGRFile")?,
        })
    }
}

fn topology_summaries(rows: &[TopologyRow]) -> HashMap<usize, TopologySummary> {
    rows.iter()
        .map(|row| {
            (
                row.row_index,
                TopologySummary {
                    row_index: row.row_index,
                    id: row.id.clone(),
                    graph_file: row.graph_file.clone(),
                },
            )
        })
        .collect()
}

fn campaign_area_summaries(rows: &[WorldAreaRow]) -> Vec<CampaignAreaSummary> {
    let mut areas = Vec::new();
    for row in rows {
        if row.id == "NULL" || row.id.is_empty() {
            continue;
        }
        if !(1..=5).contains(&row.act) {
            continue;
        }
        if !row.id.starts_with(&format!("1_{}", row.act)) {
            continue;
        }
        if row.is_map_area {
            continue;
        }
        areas.push(CampaignAreaSummary {
            row_index: row.row_index,
            id: row.id.clone(),
            name: row.name.clone(),
            act: row.act,
            is_town: row.is_town,
            area_level: row.area_level,
            topology_indices: row.topology_indices.clone(),
            tsi_file: row.tsi_file.clone(),
        });
    }
    areas
}

fn terrain_candidates(
    areas: &[CampaignAreaSummary],
    topology_by_index: &HashMap<usize, TopologySummary>,
    logical_paths: &HashSet<String>,
) -> (Vec<TerrainCandidate>, Vec<TerrainCandidate>) {
    let mut candidates = Vec::new();
    let mut skipped = Vec::new();
    for area in areas {
        if let Some(tsi_file) = &area.tsi_file {
            if is_excluded_terrain_path(tsi_file) {
                continue;
            }
            push_indexed_candidate(
                logical_paths,
                &mut candidates,
                &mut skipped,
                TerrainCandidate {
                    logical_path: tsi_file.clone(),
                    source: format!("WorldAreas[{}].TSIFile {}", area.row_index, area.id),
                    kind: TerrainCandidateKind::Tsi,
                },
            );
        }
        for topology_index in &area.topology_indices {
            let Some(topology) = topology_by_index.get(topology_index) else {
                continue;
            };
            let Some(graph_file) = &topology.graph_file else {
                continue;
            };
            let normalized_graph = normalize_logical_path(graph_file);
            if is_excluded_terrain_path(&normalized_graph) {
                continue;
            }
            let source = format!(
                "WorldAreas[{}].TopologiesKeys -> Topologies[{}] {}",
                area.row_index, topology.row_index, topology.id
            );
            push_indexed_candidate(
                logical_paths,
                &mut candidates,
                &mut skipped,
                TerrainCandidate {
                    logical_path: normalized_graph.clone(),
                    source,
                    kind: TerrainCandidateKind::Graph,
                },
            );
        }
    }
    (candidates, skipped)
}

struct TerrainFolderCorpus {
    folders: Vec<TerrainFolderSummary>,
    files: Vec<String>,
}

fn terrain_folder_corpus(
    candidates: &[TerrainCandidate],
    raw_folder_prefixes: &[String],
    logical_paths: &[String],
) -> TerrainFolderCorpus {
    let mut folder_sources = BTreeMap::new();
    for candidate in candidates {
        let Some(folder) = terrain_area_folder(&candidate.logical_path) else {
            continue;
        };
        folder_sources
            .entry(folder.clone())
            .or_insert_with(|| format!("terrain area folder for {}", candidate.logical_path));
    }
    for prefix in raw_folder_prefixes {
        let folder = normalize_logical_path(prefix);
        if folder.is_empty() || is_excluded_terrain_path(&folder) {
            continue;
        }
        folder_sources
            .entry(folder.clone())
            .or_insert_with(|| "requested raw folder".to_owned());
    }

    let wanted_folders = folder_sources.keys().cloned().collect::<HashSet<_>>();
    let mut files_by_folder = folder_sources
        .keys()
        .map(|folder| (folder.clone(), BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();

    for (index, logical_path) in logical_paths.iter().enumerate() {
        if index > 0 && index % 100_000 == 0 {
            eprintln!(
                "indexed {} / {} logical paths for terrain folders",
                index,
                logical_paths.len()
            );
        }
        let normalized = normalize_logical_path(logical_path);
        if is_excluded_terrain_path(&normalized) {
            continue;
        }
        let mut current = parent_logical_folder(&normalized);
        while let Some(folder) = current {
            if wanted_folders.contains(&folder) {
                if let Some(files) = files_by_folder.get_mut(&folder) {
                    files.insert(normalized.clone());
                }
            }
            current = parent_logical_folder(&folder);
        }
    }

    let folders = folder_sources
        .into_iter()
        .map(|(folder, source)| TerrainFolderSummary {
            file_count: files_by_folder.get(&folder).map_or(0, BTreeSet::len),
            logical_path: folder,
            source,
        })
        .collect();

    let files = files_by_folder
        .into_values()
        .flatten()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    TerrainFolderCorpus { folders, files }
}

fn parent_logical_folder(path: &str) -> Option<String> {
    let normalized = normalize_logical_path(path);
    normalized
        .rsplit_once('/')
        .and_then(|(folder, _)| (!folder.is_empty()).then(|| folder.to_owned()))
}

fn terrain_area_folder(path: &str) -> Option<String> {
    let normalized = normalize_logical_path(path);
    let parts = normalized.split('/').collect::<Vec<_>>();
    for window_start in 0..parts.len().saturating_sub(3) {
        if parts[window_start] == "metadata"
            && parts.get(window_start + 1) == Some(&"terrain")
            && parts
                .get(window_start + 2)
                .is_some_and(|part| part.starts_with("act"))
        {
            return Some(parts[..=window_start + 3].join("/"));
        }
    }
    parent_logical_folder(&normalized)
}

fn is_excluded_terrain_path(path: &str) -> bool {
    let normalized = normalize_logical_path(path);
    EXCLUDED_TERRAIN_PREFIXES
        .iter()
        .any(|prefix| normalized.starts_with(prefix))
}

fn push_indexed_candidate(
    logical_paths: &HashSet<String>,
    candidates: &mut Vec<TerrainCandidate>,
    skipped: &mut Vec<TerrainCandidate>,
    candidate: TerrainCandidate,
) {
    if logical_paths.contains(&candidate.logical_path) {
        candidates.push(candidate);
    } else {
        skipped.push(candidate);
    }
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
    use std::collections::{HashMap, HashSet};

    use pather_schema::{
        LayoutDatabaseModel, SourceFileModel, TerrainFileKind, TerrainFileModel, TerrainFileStatus,
        ZoneModel, LAYOUT_SCHEMA_VERSION,
    };

    use super::{
        inspect_layout_database, terrain_candidates, terrain_folder_corpus, write_layout_database,
        CampaignAreaSummary, TerrainCandidate, TerrainCandidateKind, TopologySummary,
    };

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

    #[test]
    fn terrain_candidates_exclude_league_specific_topologies() {
        let areas = vec![CampaignAreaSummary {
            row_index: 12,
            id: "1_1_1".to_owned(),
            name: "The Coast".to_owned(),
            act: 1,
            is_town: false,
            area_level: 2,
            topology_indices: vec![7, 8],
            tsi_file: None,
        }];
        let topology_by_index = HashMap::from([
            (
                7,
                TopologySummary {
                    row_index: 7,
                    id: "coast".to_owned(),
                    graph_file: Some("Metadata/Terrain/Act1/Area1/Graphs/coast.tgr".to_owned()),
                },
            ),
            (
                8,
                TopologySummary {
                    row_index: 8,
                    id: "deepwater_coast".to_owned(),
                    graph_file: Some(
                        "Metadata/Terrain/Leagues/Deepwater/Act1_Coast/Graphs/macro_terraces1_1_1.tgr"
                            .to_owned(),
                    ),
                },
            ),
        ]);
        let logical_paths = HashSet::from([
            "metadata/terrain/act1/area1/graphs/coast.tgr".to_owned(),
            "metadata/terrain/leagues/deepwater/act1_coast/graphs/macro_terraces1_1_1.tgr"
                .to_owned(),
        ]);

        let (candidates, skipped) = terrain_candidates(&areas, &topology_by_index, &logical_paths);

        assert!(skipped.is_empty());
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].logical_path,
            "metadata/terrain/act1/area1/graphs/coast.tgr"
        );
    }

    #[test]
    fn terrain_folder_corpus_excludes_league_specific_files() {
        let candidates = vec![TerrainCandidate {
            logical_path: "metadata/terrain/act1/area1/graphs/coast.tgr".to_owned(),
            source: "fixture".to_owned(),
            kind: TerrainCandidateKind::Graph,
        }];
        let logical_paths = vec![
            "Metadata/Terrain/Act1/Area1/Graphs/coast.tgr".to_owned(),
            "Metadata/Terrain/Act1/Area1/coast.dgr".to_owned(),
            "Metadata/Terrain/Leagues/Deepwater/Act1_Coast/Graphs/macro_terraces1_1_1.tgr"
                .to_owned(),
        ];

        let corpus = terrain_folder_corpus(
            &candidates,
            &["Metadata/Terrain/Leagues/Deepwater".to_owned()],
            &logical_paths,
        );

        assert_eq!(corpus.folders.len(), 1);
        assert_eq!(
            corpus.folders[0].logical_path,
            "metadata/terrain/act1/area1"
        );
        assert_eq!(
            corpus.files,
            vec![
                "metadata/terrain/act1/area1/coast.dgr".to_owned(),
                "metadata/terrain/act1/area1/graphs/coast.tgr".to_owned(),
            ]
        );
    }
}
