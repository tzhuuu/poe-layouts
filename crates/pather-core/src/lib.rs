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

mod layout_environment;
mod layout_transitions;
mod room_catalog;
mod room_plan;
pub use room_plan::{parse_arm_room_plan, RoomMarker, RoomObject, RoomPlan, RoomTile};
pub use room_catalog::{inspect_layout_rooms, resolve_room_bosses, LayoutNodeBosses, RoomCatalog, RoomVariant};
pub use layout_environment::{
    refresh_layout_environments, scrape_layout_environments, LayoutEnvironment,
    LayoutEnvironmentIndex, LayoutEnvironmentSummary,
};
pub use layout_transitions::{
    refresh_layout_transitions, scrape_layout_transitions, LayoutNodeTransitions, LayoutTransition,
    LayoutTransitionIndex, LayoutTransitionKind, TransitionZone, ZoneLayoutTransitions,
};

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
    pub high_level_graph_path: PathBuf,
    pub layout_environments_path: PathBuf,
    pub layout_transitions_path: PathBuf,
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
    pub selected_topologies: Vec<TopologySummary>,
    pub candidate_files: Vec<TerrainCandidate>,
    pub terrain_folders: Vec<TerrainFolderSummary>,
    pub layout_environments: Vec<LayoutEnvironmentSummary>,
    pub layout_environment_warnings: Vec<String>,
    pub layout_transitions: Vec<ZoneLayoutTransitions>,
    pub layout_transition_warnings: Vec<String>,
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CampaignHighLevelGraph {
    pub scope: String,
    pub patch_version: String,
    pub release_line: String,
    pub counts: CampaignHighLevelGraphCounts,
    pub nodes: Vec<CampaignGraphNode>,
    pub edges: Vec<CampaignGraphEdge>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct CampaignHighLevelGraphCounts {
    pub acts: usize,
    pub areas: usize,
    pub topologies: usize,
    pub terrain_folders: usize,
    pub terrain_files: usize,
    pub edges: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CampaignGraphNode {
    pub id: String,
    pub kind: CampaignGraphNodeKind,
    pub label: String,
    pub act: Option<i64>,
    pub row_index: Option<usize>,
    pub area_level: Option<i64>,
    pub is_town: Option<bool>,
    pub logical_path: Option<String>,
    pub terrain_kind: Option<TerrainCandidateKind>,
    pub terrain_status: Option<CampaignTerrainStatus>,
    pub file_count: Option<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CampaignGraphNodeKind {
    Act,
    Area,
    Topology,
    TerrainFolder,
    TerrainFile,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CampaignTerrainStatus {
    Candidate,
    Extracted,
    Missing,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CampaignGraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub kind: CampaignGraphEdgeKind,
    pub label: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CampaignGraphEdgeKind {
    ContainsArea,
    UsesTopology,
    DeclaresGraph,
    DeclaresTsi,
    GroupsTerrainFile,
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DgrLayoutGraph {
    pub logical_path: String,
    pub version: Option<u32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub master_file: Option<String>,
    pub nodes: Vec<DgrLayoutNode>,
    pub edges: Vec<DgrLayoutEdge>,
    pub warnings: Vec<String>,
    pub node_transitions: Vec<LayoutNodeTransitions>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DgrLayoutNode {
    pub index: usize,
    pub x: i32,
    pub y: i32,
    pub links: Vec<usize>,
    pub label: Option<String>,
    pub rotation: Option<String>,
    pub metadata: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DgrLayoutEdge {
    pub index: usize,
    pub from: usize,
    pub to: usize,
    pub edge_tile: Option<String>,
    pub metadata: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LayoutRoomSummary {
    pub label: String,
    pub node_count: usize,
    pub layouts: Vec<LayoutRoomOccurrence>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LayoutRoomOccurrence {
    pub logical_path: String,
    pub node_indices: Vec<usize>,
}

pub fn summarize_layout_rooms(graphs: &[DgrLayoutGraph]) -> Vec<LayoutRoomSummary> {
    let mut rooms: BTreeMap<String, BTreeMap<String, BTreeSet<usize>>> = BTreeMap::new();
    for graph in graphs {
        for node in &graph.nodes {
            if let Some(label) = &node.label {
                rooms
                    .entry(label.clone())
                    .or_default()
                    .entry(graph.logical_path.clone())
                    .or_default()
                    .insert(node.index);
            }
        }
    }
    let mut summaries = rooms
        .into_iter()
        .map(|(label, layouts)| {
            let layouts = layouts
                .into_iter()
                .map(|(logical_path, indices)| LayoutRoomOccurrence {
                    logical_path,
                    node_indices: indices.into_iter().collect(),
                })
                .collect::<Vec<_>>();
            LayoutRoomSummary {
                label,
                node_count: layouts.iter().map(|layout| layout.node_indices.len()).sum(),
                layouts,
            }
        })
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| {
        right
            .layouts
            .len()
            .cmp(&left.layouts.len())
            .then(left.label.cmp(&right.label))
    });
    summaries
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
    #[error("invalid DGR file {path}: {message}")]
    DgrParse { path: String, message: String },

    #[error("ARM parse error for {path}: {message}")]
    ArmParse { path: String, message: String },
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
    let selected_topologies = selected_topology_summaries(&selected_areas, &topology_by_index);
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

    let layout_environments = scrape_layout_environments(
        &files_dir,
        &folder_files
            .iter()
            .map(|file| file.logical_path.clone())
            .collect::<Vec<_>>(),
        &request.source.patch_version,
        &request.source.release_line(),
    );
    let layout_environments_path = request.out_dir.join("layout-environments.json");
    write_json(&layout_environments_path, &layout_environments)?;
    let layout_transitions = scrape_layout_transitions(
        &files_dir,
        &folder_files
            .iter()
            .chain(&extracted_files)
            .map(|file| file.logical_path.clone())
            .collect::<Vec<_>>(),
        &selected_areas,
        &candidates,
        &request.schema_path,
        &request.source.patch_version,
        &request.source.release_line(),
    )?;
    let layout_transitions_path = request.out_dir.join("layout-transitions.json");
    write_json(&layout_transitions_path, &layout_transitions)?;

    let manifest = CampaignScrapeManifest {
        scope: CAMPAIGN_ACTS_ONE_TO_FIVE_SCOPE.to_owned(),
        patch_version: request.source.patch_version.clone(),
        release_line: request.source.release_line(),
        schema: request.schema_path.clone(),
        out_dir: request.out_dir.clone(),
        tables: extracted_tables,
        selected_areas,
        selected_topologies,
        candidate_files: candidates,
        terrain_folders,
        layout_environments: layout_environments.layouts,
        layout_environment_warnings: layout_environments.warnings,
        layout_transitions: layout_transitions.layouts,
        layout_transition_warnings: layout_transitions.warnings,
        extracted_files,
        folder_files,
        missing_files,
        warnings,
    };

    let manifest_path = request.out_dir.join("manifest.json");
    write_json(&manifest_path, &manifest)?;
    let high_level_graph = campaign_manifest_to_high_level_graph(&manifest);
    let high_level_graph_path = request.out_dir.join("high-level-graph.json");
    write_json(&high_level_graph_path, &high_level_graph)?;
    let layout_db_path = if let Some(path) = &request.layout_db_out {
        write_layout_database(path, &campaign_manifest_to_layout_model(&manifest)?)?;
        Some(path.clone())
    } else {
        None
    };

    Ok(CampaignScrapeOutput {
        manifest_path,
        high_level_graph_path,
        layout_environments_path,
        layout_transitions_path,
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

pub fn campaign_manifest_to_high_level_graph(
    manifest: &CampaignScrapeManifest,
) -> CampaignHighLevelGraph {
    let extracted_paths = manifest
        .extracted_files
        .iter()
        .map(|file| file.logical_path.as_str())
        .collect::<HashSet<_>>();
    let missing_paths = manifest
        .missing_files
        .iter()
        .map(|file| file.logical_path.as_str())
        .collect::<HashSet<_>>();
    let topology_indices = manifest
        .selected_topologies
        .iter()
        .map(|topology| topology.row_index)
        .collect::<HashSet<_>>();

    let mut nodes = Vec::new();
    for act in 1..=5 {
        nodes.push(CampaignGraphNode {
            id: graph_act_id(act),
            kind: CampaignGraphNodeKind::Act,
            label: format!("Act {act}"),
            act: Some(act),
            row_index: None,
            area_level: None,
            is_town: None,
            logical_path: None,
            terrain_kind: None,
            terrain_status: None,
            file_count: None,
        });
    }

    for area in &manifest.selected_areas {
        nodes.push(CampaignGraphNode {
            id: graph_area_id(&area.id),
            kind: CampaignGraphNodeKind::Area,
            label: area.name.clone(),
            act: Some(area.act),
            row_index: Some(area.row_index),
            area_level: Some(area.area_level),
            is_town: Some(area.is_town),
            logical_path: None,
            terrain_kind: None,
            terrain_status: None,
            file_count: None,
        });
    }

    for topology in &manifest.selected_topologies {
        nodes.push(CampaignGraphNode {
            id: graph_topology_id(topology.row_index),
            kind: CampaignGraphNodeKind::Topology,
            label: topology_label(topology),
            act: None,
            row_index: Some(topology.row_index),
            area_level: None,
            is_town: None,
            logical_path: None,
            terrain_kind: None,
            terrain_status: None,
            file_count: None,
        });
    }

    for folder in &manifest.terrain_folders {
        nodes.push(CampaignGraphNode {
            id: graph_folder_id(&folder.logical_path),
            kind: CampaignGraphNodeKind::TerrainFolder,
            label: folder.logical_path.clone(),
            act: None,
            row_index: None,
            area_level: None,
            is_town: None,
            logical_path: Some(folder.logical_path.clone()),
            terrain_kind: None,
            terrain_status: None,
            file_count: Some(folder.file_count),
        });
    }

    for candidate in &manifest.candidate_files {
        nodes.push(CampaignGraphNode {
            id: graph_terrain_file_id(&candidate.logical_path),
            kind: CampaignGraphNodeKind::TerrainFile,
            label: logical_file_name(&candidate.logical_path),
            act: None,
            row_index: None,
            area_level: None,
            is_town: None,
            logical_path: Some(candidate.logical_path.clone()),
            terrain_kind: Some(candidate.kind),
            terrain_status: Some(terrain_status(
                &candidate.logical_path,
                &extracted_paths,
                &missing_paths,
            )),
            file_count: None,
        });
    }

    let mut edges = Vec::new();
    for area in &manifest.selected_areas {
        push_graph_edge(
            &mut edges,
            graph_act_id(area.act),
            graph_area_id(&area.id),
            CampaignGraphEdgeKind::ContainsArea,
            "contains area",
        );
        for topology_index in &area.topology_indices {
            if topology_indices.contains(topology_index) {
                push_graph_edge(
                    &mut edges,
                    graph_area_id(&area.id),
                    graph_topology_id(*topology_index),
                    CampaignGraphEdgeKind::UsesTopology,
                    "uses topology",
                );
            }
        }
    }

    for candidate in &manifest.candidate_files {
        let target = graph_terrain_file_id(&candidate.logical_path);
        match candidate.kind {
            TerrainCandidateKind::Graph => {
                if let Some(topology_index) = source_row_index(&candidate.source, "Topologies") {
                    push_graph_edge(
                        &mut edges,
                        graph_topology_id(topology_index),
                        target.clone(),
                        CampaignGraphEdgeKind::DeclaresGraph,
                        "declares graph",
                    );
                }
            }
            TerrainCandidateKind::Tsi => {
                if let Some(area_row_index) = source_row_index(&candidate.source, "WorldAreas") {
                    if let Some(area) = manifest
                        .selected_areas
                        .iter()
                        .find(|area| area.row_index == area_row_index)
                    {
                        push_graph_edge(
                            &mut edges,
                            graph_area_id(&area.id),
                            target.clone(),
                            CampaignGraphEdgeKind::DeclaresTsi,
                            "declares TSI",
                        );
                    }
                }
            }
            TerrainCandidateKind::DgrVariant | TerrainCandidateKind::ArmVariant => {}
        }

        if let Some(folder) = terrain_area_folder(&candidate.logical_path) {
            push_graph_edge(
                &mut edges,
                graph_folder_id(&folder),
                target,
                CampaignGraphEdgeKind::GroupsTerrainFile,
                "groups terrain file",
            );
        }
    }

    let counts = CampaignHighLevelGraphCounts {
        acts: 5,
        areas: manifest.selected_areas.len(),
        topologies: manifest.selected_topologies.len(),
        terrain_folders: manifest.terrain_folders.len(),
        terrain_files: manifest.candidate_files.len(),
        edges: edges.len(),
    };

    CampaignHighLevelGraph {
        scope: manifest.scope.clone(),
        patch_version: manifest.patch_version.clone(),
        release_line: manifest.release_line.clone(),
        counts,
        nodes,
        edges,
        warnings: manifest.warnings.clone(),
    }
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

pub fn parse_dgr_layout_graph(
    logical_path: &str,
    bytes: &[u8],
) -> Result<DgrLayoutGraph, CampaignScrapeError> {
    let text = decode_utf16le_text(logical_path, bytes)?;
    let lines = text
        .lines()
        .map(|line| line.trim().trim_start_matches('\u{feff}'))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    let version = lines
        .iter()
        .find_map(|line| line.strip_prefix("version "))
        .and_then(|value| value.trim().parse::<u32>().ok());
    let (width, height) = lines
        .iter()
        .find_map(|line| line.strip_prefix("Size: "))
        .and_then(parse_size_line)
        .unwrap_or((None, None));
    let master_file = lines
        .iter()
        .find_map(|line| line.strip_prefix("MasterFile: "))
        .and_then(|value| tokenize_dgr_line(value).into_iter().next());
    let node_count = lines
        .iter()
        .find_map(|line| line.strip_prefix("Nodes: "))
        .and_then(|value| value.trim().parse::<usize>().ok())
        .ok_or_else(|| CampaignScrapeError::DgrParse {
            path: logical_path.to_owned(),
            message: "missing Nodes header".to_owned(),
        })?;
    let edge_count = lines
        .iter()
        .find_map(|line| line.strip_prefix("Edges: "))
        .and_then(|value| value.trim().parse::<usize>().ok())
        .ok_or_else(|| CampaignScrapeError::DgrParse {
            path: logical_path.to_owned(),
            message: "missing Edges header".to_owned(),
        })?;
    let data_start = layout_graph_data_start(logical_path, &lines)?;

    let node_lines = lines
        .get(data_start..data_start + node_count)
        .ok_or_else(|| CampaignScrapeError::DgrParse {
            path: logical_path.to_owned(),
            message: format!("expected {node_count} node rows"),
        })?;
    let edge_start = data_start + node_count;
    let edge_lines = lines
        .get(edge_start..edge_start + edge_count)
        .ok_or_else(|| CampaignScrapeError::DgrParse {
            path: logical_path.to_owned(),
            message: format!("expected {edge_count} edge rows"),
        })?;

    let mut warnings = Vec::new();
    let nodes = node_lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| match parse_dgr_node(index, line) {
            Ok(node) => Some(node),
            Err(message) => {
                warnings.push(format!("node {index}: {message}"));
                None
            }
        })
        .collect::<Vec<_>>();
    let edges = edge_lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| match parse_dgr_edge(index, line) {
            Ok(edge) => Some(edge),
            Err(message) => {
                warnings.push(format!("edge {index}: {message}"));
                None
            }
        })
        .collect::<Vec<_>>();

    Ok(DgrLayoutGraph {
        logical_path: logical_path.to_owned(),
        version,
        width,
        height,
        master_file,
        nodes,
        edges,
        warnings,
        node_transitions: Vec::new(),
    })
}

fn layout_graph_data_start(
    logical_path: &str,
    lines: &[&str],
) -> Result<usize, CampaignScrapeError> {
    if let Some(default_index) = lines.iter().position(|line| line.starts_with("Default%:")) {
        let start = default_index + 1;
        if let Some(count) = lines.get(start).and_then(|line| line.parse::<usize>().ok()) {
            // Some DGR files have a standalone preamble count; only zero is observed.
            if count != 0 {
                return Err(CampaignScrapeError::DgrParse {
                    path: logical_path.to_owned(),
                    message: format!("unsupported nonzero graph preamble count: {count}"),
                });
            }
            return Ok(start + 1);
        }
        return Ok(start);
    }

    lines
        .iter()
        .position(|line| line.starts_with("Edges: "))
        .map(|index| index + 4)
        .ok_or_else(|| CampaignScrapeError::DgrParse {
            path: logical_path.to_owned(),
            message: "missing Edges header".to_owned(),
        })
}

fn decode_utf16le_text(path: &str, bytes: &[u8]) -> Result<String, CampaignScrapeError> {
    if bytes.len() % 2 != 0 {
        return Err(CampaignScrapeError::DgrParse {
            path: path.to_owned(),
            message: "UTF-16LE byte length is odd".to_owned(),
        });
    }
    let words = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    Ok(String::from_utf16_lossy(&words))
}

fn parse_size_line(value: &str) -> Option<(Option<i32>, Option<i32>)> {
    let mut parts = value.split_whitespace();
    let width = parts.next()?.parse::<i32>().ok();
    let height = parts.next()?.parse::<i32>().ok();
    Some((width, height))
}

fn parse_dgr_node(index: usize, line: &str) -> Result<DgrLayoutNode, String> {
    let tokens = tokenize_dgr_line(line);
    let x = parse_i32_token(&tokens, 0, "x")?;
    let y = parse_i32_token(&tokens, 1, "y")?;
    let link_count = parse_usize_token(&tokens, 2, "link count")?;
    let link_start = 3;
    let label_index = link_start + link_count;
    if tokens.len() <= label_index {
        return Err(format!(
            "expected {link_count} link ids before node metadata, got {} tokens",
            tokens.len()
        ));
    }
    let links = (link_start..label_index)
        .filter_map(|token_index| tokens[token_index].parse::<usize>().ok())
        .collect::<Vec<_>>();
    if links.len() != link_count {
        return Err("one or more link ids were not integers".to_owned());
    }
    let label = non_empty_token(tokens.get(label_index));
    let rotation = non_empty_token(tokens.get(label_index + 1));
    let metadata = tokens
        .get(label_index + 2..)
        .unwrap_or_default()
        .iter()
        .filter(|value| !value.is_empty())
        .cloned()
        .collect();

    Ok(DgrLayoutNode {
        index,
        x,
        y,
        links,
        label,
        rotation,
        metadata,
    })
}

fn parse_dgr_edge(index: usize, line: &str) -> Result<DgrLayoutEdge, String> {
    let tokens = tokenize_dgr_line(line);
    let from = parse_usize_token(&tokens, 0, "from node")?;
    let to = parse_usize_token(&tokens, 1, "to node")?;
    let edge_tile = tokens
        .iter()
        .find(|token| token.to_ascii_lowercase().ends_with(".et"))
        .cloned();
    let metadata = tokens
        .get(2..)
        .unwrap_or_default()
        .iter()
        .filter(|value| !value.is_empty())
        .cloned()
        .collect();

    Ok(DgrLayoutEdge {
        index,
        from,
        to,
        edge_tile,
        metadata,
    })
}

fn tokenize_dgr_line(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut chars = line.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' => {
                if in_quote {
                    tokens.push(current.clone());
                    current.clear();
                    in_quote = false;
                    while chars.peek().is_some_and(|next| next.is_whitespace()) {
                        chars.next();
                    }
                } else {
                    if !current.trim().is_empty() {
                        tokens.push(current.trim().to_owned());
                        current.clear();
                    }
                    in_quote = true;
                }
            }
            character if character.is_whitespace() && !in_quote => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(character),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn parse_i32_token(tokens: &[String], index: usize, name: &str) -> Result<i32, String> {
    tokens
        .get(index)
        .ok_or_else(|| format!("missing {name}"))?
        .parse::<i32>()
        .map_err(|_| format!("invalid {name}"))
}

fn parse_usize_token(tokens: &[String], index: usize, name: &str) -> Result<usize, String> {
    tokens
        .get(index)
        .ok_or_else(|| format!("missing {name}"))?
        .parse::<usize>()
        .map_err(|_| format!("invalid {name}"))
}

fn non_empty_token(token: Option<&String>) -> Option<String> {
    token.filter(|value| !value.is_empty()).cloned()
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

fn selected_topology_summaries(
    areas: &[CampaignAreaSummary],
    topology_by_index: &HashMap<usize, TopologySummary>,
) -> Vec<TopologySummary> {
    let mut topologies = areas
        .iter()
        .flat_map(|area| area.topology_indices.iter())
        .filter_map(|index| topology_by_index.get(index))
        .cloned()
        .collect::<Vec<_>>();
    topologies.sort_by_key(|topology| topology.row_index);
    topologies.dedup_by_key(|topology| topology.row_index);
    topologies
}

fn topology_label(topology: &TopologySummary) -> String {
    if topology.id.is_empty() {
        format!("Topology {}", topology.row_index)
    } else {
        topology.id.clone()
    }
}

fn graph_act_id(act: i64) -> String {
    format!("act:{act}")
}

fn graph_area_id(area_id: &str) -> String {
    format!("area:{area_id}")
}

fn graph_topology_id(row_index: usize) -> String {
    format!("topology:{row_index}")
}

fn graph_folder_id(logical_path: &str) -> String {
    format!("terrain-folder:{}", normalize_logical_path(logical_path))
}

fn graph_terrain_file_id(logical_path: &str) -> String {
    format!("terrain-file:{}", normalize_logical_path(logical_path))
}

fn push_graph_edge(
    edges: &mut Vec<CampaignGraphEdge>,
    source: String,
    target: String,
    kind: CampaignGraphEdgeKind,
    label: &str,
) {
    edges.push(CampaignGraphEdge {
        id: format!("edge:{}:{}:{}", edge_kind_id(kind), source, target),
        source,
        target,
        kind,
        label: label.to_owned(),
    });
}

fn edge_kind_id(kind: CampaignGraphEdgeKind) -> &'static str {
    match kind {
        CampaignGraphEdgeKind::ContainsArea => "contains-area",
        CampaignGraphEdgeKind::UsesTopology => "uses-topology",
        CampaignGraphEdgeKind::DeclaresGraph => "declares-graph",
        CampaignGraphEdgeKind::DeclaresTsi => "declares-tsi",
        CampaignGraphEdgeKind::GroupsTerrainFile => "groups-terrain-file",
    }
}

fn terrain_status(
    logical_path: &str,
    extracted_paths: &HashSet<&str>,
    missing_paths: &HashSet<&str>,
) -> CampaignTerrainStatus {
    if extracted_paths.contains(logical_path) {
        CampaignTerrainStatus::Extracted
    } else if missing_paths.contains(logical_path) {
        CampaignTerrainStatus::Missing
    } else {
        CampaignTerrainStatus::Candidate
    }
}

fn logical_file_name(logical_path: &str) -> String {
    logical_path
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(logical_path)
        .to_owned()
}

fn source_row_index(source: &str, table: &str) -> Option<usize> {
    let marker = format!("{table}[");
    let start = source.find(&marker)? + marker.len();
    let end = source.get(start..)?.find(']')? + start;
    source.get(start..end)?.parse().ok()
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
    use std::fs;
    use std::path::{Path, PathBuf};

    use pather_schema::{
        LayoutDatabaseModel, SourceFileModel, TerrainFileKind, TerrainFileModel, TerrainFileStatus,
        ZoneModel, LAYOUT_SCHEMA_VERSION,
    };
    use poe_content::{ExtractedLogicalFile, LogicalFileLocation};

    use super::{
        campaign_manifest_to_high_level_graph, inspect_layout_database, parse_dgr_layout_graph,
        summarize_layout_rooms, terrain_candidates, terrain_folder_corpus, write_layout_database,
        CampaignAreaSummary, CampaignGraphEdgeKind, CampaignGraphNodeKind, CampaignScrapeManifest,
        CampaignTerrainStatus, TerrainCandidate, TerrainCandidateKind, TerrainFolderSummary,
        TopologySummary,
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

    #[test]
    fn high_level_graph_links_areas_topologies_and_terrain_files() {
        let manifest = CampaignScrapeManifest {
            scope: "campaign-acts-1-5".to_owned(),
            patch_version: "3.29.3.3".to_owned(),
            release_line: "3.29".to_owned(),
            schema: PathBuf::from("data/cache/dat-schema/_Core.gql"),
            out_dir: PathBuf::from(".poe-layouts/raw/campaign-acts-1-5"),
            tables: Vec::new(),
            selected_areas: vec![CampaignAreaSummary {
                row_index: 5,
                id: "1_1_1".to_owned(),
                name: "The Twilight Strand".to_owned(),
                act: 1,
                is_town: false,
                area_level: 1,
                topology_indices: vec![12],
                tsi_file: None,
            }],
            selected_topologies: vec![TopologySummary {
                row_index: 12,
                id: "twilight_strand".to_owned(),
                graph_file: Some(
                    "metadata/terrain/act1/area1/graphs/twilight_strand.tgr".to_owned(),
                ),
            }],
            candidate_files: vec![TerrainCandidate {
                logical_path: "metadata/terrain/act1/area1/graphs/twilight_strand.tgr".to_owned(),
                source: "WorldAreas[5].TopologiesKeys -> Topologies[12] twilight_strand"
                    .to_owned(),
                kind: TerrainCandidateKind::Graph,
            }],
            terrain_folders: vec![TerrainFolderSummary {
                logical_path: "metadata/terrain/act1/area1".to_owned(),
                source:
                    "terrain area folder for metadata/terrain/act1/area1/graphs/twilight_strand.tgr"
                        .to_owned(),
                file_count: 3,
            }],
            extracted_files: vec![ExtractedLogicalFile {
                logical_path: "metadata/terrain/act1/area1/graphs/twilight_strand.tgr".to_owned(),
                output_path: PathBuf::from(
                    ".poe-layouts/raw/campaign-acts-1-5/files/metadata/terrain/act1/area1/graphs/twilight_strand.tgr",
                ),
                location: LogicalFileLocation {
                    bundle: "fixture.bundle.bin".to_owned(),
                    offset: 0,
                    size: 1,
                },
                cache_key: "fixture".to_owned(),
                cache_path: PathBuf::from(".poe-layouts/cache/fixture"),
                cache_source: "cache".to_owned(),
                byte_len: 1,
                blake3: "hash".to_owned(),
            }],
            folder_files: Vec::new(),
            layout_environments: Vec::new(),
            layout_environment_warnings: Vec::new(),
            layout_transitions: Vec::new(),
            layout_transition_warnings: Vec::new(),
            missing_files: Vec::new(),
            warnings: Vec::new(),
        };

        let graph = campaign_manifest_to_high_level_graph(&manifest);

        assert_eq!(graph.counts.areas, 1);
        assert_eq!(graph.counts.topologies, 1);
        assert_eq!(graph.counts.terrain_files, 1);
        assert!(graph
            .nodes
            .iter()
            .any(|node| { node.id == "area:1_1_1" && node.kind == CampaignGraphNodeKind::Area }));
        assert!(graph.nodes.iter().any(|node| {
            node.id == "terrain-file:metadata/terrain/act1/area1/graphs/twilight_strand.tgr"
                && node.terrain_status == Some(CampaignTerrainStatus::Extracted)
        }));
        assert!(graph.edges.iter().any(|edge| {
            edge.kind == CampaignGraphEdgeKind::UsesTopology
                && edge.source == "area:1_1_1"
                && edge.target == "topology:12"
        }));
        assert!(graph.edges.iter().any(|edge| {
            edge.kind == CampaignGraphEdgeKind::DeclaresGraph
                && edge.source == "topology:12"
                && edge.target
                    == "terrain-file:metadata/terrain/act1/area1/graphs/twilight_strand.tgr"
        }));
    }

    #[test]
    fn summarize_layout_rooms_counts_distinct_layouts_and_node_occurrences() {
        let text = r#"version 19
Nodes: 4
Edges: 0
""
""
""
0 0 0 "waypoint" I 0 100
1 1 0 "waypoint" I 0 100
2 2 0 "entrance" I 0 100
3 3 0 "" (any) 0 100
"#;
        let bytes = text
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let first = parse_dgr_layout_graph("first.tgr", &bytes).expect("first graph");
        let mut second = parse_dgr_layout_graph("second.tgr", &bytes).expect("second graph");
        second.nodes.truncate(1);
        let rooms = summarize_layout_rooms(&[first.clone(), first, second]);

        assert_eq!(rooms.len(), 2);
        assert_eq!(rooms[0].label, "waypoint");
        assert_eq!(rooms[0].layouts.len(), 2);
        assert_eq!(rooms[0].node_count, 3);
        assert_eq!(rooms[0].layouts[0].node_indices, vec![0, 1]);
        assert_eq!(rooms[1].label, "entrance");
        assert_eq!(rooms[1].layouts.len(), 1);
        assert_eq!(rooms[1].node_count, 1);
        assert!(summarize_layout_rooms(&[]).is_empty());
    }

    #[test]
    fn parse_dgr_layout_graph_reads_nodes_and_edges() {
        let text = r#"version 19
Size: 12 8
MasterFile: "Metadata/Terrain/Act1/Area7Level1/master.tsi"
Nodes: 2
Edges: 1
""
"Metadata/Terrain/Act1/Area7Level1/GroundTypes/prison_floor.gt"
""
Default%: 0 0 0
60 174 1 0 "entranceout" R180 3 "default" "entrance1" "waypoint" 100 0 N
258 14 1 0 "entranceup" R270 1 "entrance2" 100 0 N
0 1 0 100 0 "Metadata/Terrain/Dungeon/rooms.et" 10 100 "" 0 0 0 P N 1
"#;
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));

        let graph = parse_dgr_layout_graph("fixture.dgr", &bytes).expect("parse DGR");

        assert_eq!(graph.version, Some(19));
        assert_eq!(graph.width, Some(12));
        assert_eq!(graph.height, Some(8));
        assert_eq!(
            graph.master_file.as_deref(),
            Some("Metadata/Terrain/Act1/Area7Level1/master.tsi")
        );
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.nodes[0].links, vec![0]);
        assert_eq!(graph.nodes[0].label.as_deref(), Some("entranceout"));
        assert_eq!(graph.nodes[0].rotation.as_deref(), Some("R180"));
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].from, 0);
        assert_eq!(graph.edges[0].to, 1);
        assert_eq!(
            graph.edges[0].edge_tile.as_deref(),
            Some("Metadata/Terrain/Dungeon/rooms.et")
        );
    }

    #[test]
    fn parse_dgr_layout_graph_reads_tgr_without_default_row() {
        let text = r#"version 19
Size: 84 18
MasterFile: "Metadata/Terrain/Act1/Area1/master.tsi"
Nodes: 2
Edges: 1
""
"Metadata/Terrain/Act1/Area1/GroundTypes/chris_sand_dune.gt"
""
1668 75 1 0 "townentrance" I 2 "entrance1" "AutoWaypoint" 100 0 1 1
296 81 1 0 "washedup" R270 1 "default" 100 0 1 1
1 0 0 100 1 "Metadata/Terrain/Beach/LargeCliffs/beach_large_cliff.et" 0 0
"#;
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));

        let graph = parse_dgr_layout_graph("fixture.tgr", &bytes).expect("parse TGR");

        assert_eq!(graph.version, Some(19));
        assert_eq!(graph.width, Some(84));
        assert_eq!(graph.height, Some(18));
        assert_eq!(
            graph.master_file.as_deref(),
            Some("Metadata/Terrain/Act1/Area1/master.tsi")
        );
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.nodes[0].links, vec![0]);
        assert_eq!(graph.nodes[0].label.as_deref(), Some("townentrance"));
        assert_eq!(graph.nodes[0].rotation.as_deref(), Some("I"));
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].from, 1);
        assert_eq!(graph.edges[0].to, 0);
        assert_eq!(
            graph.edges[0].edge_tile.as_deref(),
            Some("Metadata/Terrain/Beach/LargeCliffs/beach_large_cliff.et")
        );
    }

    #[test]
    #[ignore = "requires the local Acts 1-5 raw cache"]
    fn validates_local_graph_corpus() {
        fn visit(path: &Path, counts: &mut (usize, usize)) {
            for entry in fs::read_dir(path).expect("read graph corpus") {
                let path = entry.expect("read corpus entry").path();
                if path.is_dir() {
                    visit(&path, counts);
                } else if path
                    .extension()
                    .is_some_and(|extension| extension == "tgr" || extension == "dgr")
                {
                    let graph = parse_dgr_layout_graph(
                        &path.to_string_lossy(),
                        &fs::read(&path).expect("read cached graph"),
                    )
                    .expect("parse cached graph");
                    assert!(
                        graph.warnings.is_empty(),
                        "{}: {:?}",
                        path.display(),
                        graph.warnings
                    );
                    for node in &graph.nodes {
                        let mut actual = node.links.clone();
                        let mut expected = graph
                            .edges
                            .iter()
                            .filter(|edge| edge.from == node.index || edge.to == node.index)
                            .map(|edge| edge.index)
                            .collect::<Vec<_>>();
                        actual.sort_unstable();
                        expected.sort_unstable();
                        assert_eq!(actual, expected, "{}: node {}", path.display(), node.index);
                    }
                    if path.extension().is_some_and(|extension| extension == "tgr") {
                        counts.0 += 1;
                    } else {
                        counts.1 += 1;
                    }
                }
            }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.poe-layouts/raw/campaign-acts-1-5/files/metadata");
        let mut counts = (0, 0);
        visit(&root, &mut counts);
        eprintln!("Graph corpus: {} TGRs, {} DGRs", counts.0, counts.1);
        assert!(counts.0 > 0 && counts.1 > 0);
    }

    #[test]
    fn parse_dgr_layout_graph_skips_empty_preamble_without_shifting_indices() {
        let text = r#"version 25
Size: 9 9
MasterFile: "Metadata/Terrain/Act1/Area4Level0/master.tsi"
Nodes: 3
Edges: 2
""
"Metadata/Terrain/Act1/Area4Level0/GroundTypes/cave_floor.gt"
""
Default%: 0 0 0
0
0 196 1 1 "entranceup" I 2 "default" "entrance1" 100 0 N 0
196 12 1 0 "waterboss" FR270 1 "entrance2disabled" 100 0 N 0
104 81 2 0 1 "" (any) 0 100 0 N 0
2 1 0 100 0 0 "Metadata/Terrain/Dungeon/rooms.et" 20 100 0 "" 0 "" 20 0 P N 1
0 2 0 100 0 0 "Metadata/Terrain/Dungeon/rooms.et" 20 100 0 "" 0 "" 20 0 P N 1
"#;
        let bytes = text
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let graph = parse_dgr_layout_graph("fixture.dgr", &bytes).expect("parse DGR preamble");
        assert!(graph.warnings.is_empty());
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2);
        assert_eq!(graph.nodes[0].index, 0);
        assert_eq!(graph.nodes[0].x, 0);
        assert_eq!(graph.nodes[0].label.as_deref(), Some("entranceup"));
        assert_eq!(graph.nodes[2].index, 2);
        assert_eq!(graph.nodes[2].links, vec![0, 1]);
        assert_eq!((graph.edges[1].from, graph.edges[1].to), (0, 2));
        for node in &graph.nodes {
            let mut actual = node.links.clone();
            let mut expected = graph
                .edges
                .iter()
                .filter(|edge| edge.from == node.index || edge.to == node.index)
                .map(|edge| edge.index)
                .collect::<Vec<_>>();
            actual.sort_unstable();
            expected.sort_unstable();
            assert_eq!(actual, expected);
        }
        let unsupported = text.replacen("\n0\n", "\n1\n", 1);
        let bytes = unsupported
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let error =
            parse_dgr_layout_graph("unsupported.dgr", &bytes).expect_err("reject unknown preamble");
        assert!(error
            .to_string()
            .contains("unsupported nonzero graph preamble count: 1"));
    }
}
