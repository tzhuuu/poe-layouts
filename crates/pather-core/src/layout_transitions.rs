use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use poe_dat::{
    read_typed_graphql_table, DatRowView, DatTableError, GraphqlDatSchema, TypedDatTableRow,
};
use serde::{Deserialize, Serialize};

use crate::layout_environment::{parse_tsi_fields, resolve_reference};
use crate::{
    decode_utf16le_text, parse_dgr_layout_graph, read, read_to_string, tokenize_dgr_line,
    write_json, CampaignAreaSummary, CampaignScrapeError, DgrLayoutGraph, DgrLayoutNode,
    TerrainCandidate,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransitionZone {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LayoutTransitionKind {
    Zone,
    Door,
    Unresolved,
    Deferred,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LayoutTransition {
    pub kind: LayoutTransitionKind,
    pub tag: Option<String>,
    pub destination: Option<TransitionZone>,
    pub object_path: Option<String>,
    pub room_paths: Vec<String>,
    pub basis: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LayoutNodeTransitions {
    pub node_index: usize,
    pub transitions: Vec<LayoutTransition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZoneLayoutTransitions {
    pub zone_id: String,
    pub logical_path: String,
    pub nodes: Vec<LayoutNodeTransitions>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LayoutTransitionIndex {
    pub patch_version: String,
    pub release_line: String,
    pub layouts: Vec<ZoneLayoutTransitions>,
    pub warnings: Vec<String>,
}

struct ConnectionAreaRow {
    row_index: usize,
    zone: TransitionZone,
    connections: Vec<usize>,
}

impl TypedDatTableRow for ConnectionAreaRow {
    const TABLE_NAME: &'static str = "WorldAreas";
    const COLUMNS: &'static [&'static str] = &["Id", "Name", "Connections_WorldAreasKeys"];

    fn from_dat_row(row: DatRowView<'_>) -> Result<Self, DatTableError> {
        Ok(Self {
            row_index: row.row_index()?,
            zone: TransitionZone {
                id: row.string_or_default("Id")?,
                name: row.string_or_default("Name")?,
            },
            connections: row.unsigned_array("Connections_WorldAreasKeys")?,
        })
    }
}

pub fn scrape_layout_transitions(
    files_dir: &Path,
    logical_paths: &[String],
    areas: &[CampaignAreaSummary],
    candidates: &[TerrainCandidate],
    schema_path: &Path,
    patch_version: &str,
    release_line: &str,
) -> Result<LayoutTransitionIndex, CampaignScrapeError> {
    let schema = GraphqlDatSchema::parse(&read_to_string(schema_path)?)?;
    let rows = read_typed_graphql_table::<ConnectionAreaRow>(
        &schema,
        &read(files_dir.join("data/worldareas.datc64"))?,
        None,
    )?;
    let rows = rows
        .iter()
        .map(|row| (row.row_index, row))
        .collect::<BTreeMap<_, _>>();
    let paths = logical_paths
        .iter()
        .map(|path| normalize_path(path))
        .collect::<BTreeSet<_>>();
    let mut warnings = BTreeSet::new();
    let mut room_sets = BTreeMap::new();
    let mut layouts = Vec::new();
    for path in paths
        .iter()
        .filter(|path| path.ends_with(".dgr") || path.ends_with(".tgr"))
    {
        let graph = match read(files_dir.join(path))
            .and_then(|bytes| parse_dgr_layout_graph(path, &bytes))
        {
            Ok(graph) => graph,
            Err(error) => {
                warnings.insert(error.to_string());
                continue;
            }
        };
        let master = graph
            .master_file
            .as_deref()
            .and_then(|reference| resolve_reference(path, reference));
        let Some(master) = master else { continue };
        let room_set = read(files_dir.join(&master))
            .and_then(|bytes| decode_utf16le_text(&master, &bytes))
            .ok()
            .and_then(|text| parse_tsi_fields(&text).get("roomset").cloned())
            .and_then(|reference| resolve_reference(&master, &reference));
        let rooms = room_set.as_ref().map(|room_set| {
            room_sets
                .entry(room_set.clone())
                .or_insert_with(|| room_set_doors(files_dir, room_set, &paths, &mut warnings))
        });
        for area in areas {
            let declared = candidates.iter().any(|candidate| {
                normalize_path(&candidate.logical_path) == *path
                    && candidate
                        .source
                        .starts_with(&format!("WorldAreas[{}].", area.row_index))
            });
            let same_master = area
                .tsi_file
                .as_deref()
                .is_some_and(|tsi| normalize_path(tsi) == master);
            let pyramid_subgraph =
                area.id == "1_2_14_3" && path.starts_with("metadata/terrain/act2/area14level3/");
            if !declared && !same_master && !pyramid_subgraph {
                continue;
            }
            let Some(row) = rows.get(&area.row_index) else {
                continue;
            };
            let mut nodes =
                resolve_graph_transitions(&graph, &row.connections, &rows, pyramid_subgraph);
            if let Some(rooms) = &rooms {
                for node in &graph.nodes {
                    let Some(doors) = node.label.as_ref().and_then(|label| rooms.get(label)) else {
                        continue;
                    };
                    let entry = match nodes
                        .iter()
                        .position(|entry| entry.node_index == node.index)
                    {
                        Some(index) => &mut nodes[index],
                        None => {
                            nodes.push(LayoutNodeTransitions {
                                node_index: node.index,
                                transitions: Vec::new(),
                            });
                            nodes.last_mut().expect("inserted node")
                        }
                    };
                    for (object_path, room_paths) in doors {
                        entry.transitions.push(LayoutTransition {
                            kind: LayoutTransitionKind::Door,
                            tag: None,
                            destination: None,
                            object_path: Some(object_path.clone()),
                            room_paths: room_paths.iter().cloned().collect(),
                            basis: "room_object_candidate".to_owned(),
                        });
                    }
                }
            }
            nodes.sort_by_key(|node| node.node_index);
            layouts.push(ZoneLayoutTransitions {
                zone_id: area.id.clone(),
                logical_path: path.clone(),
                nodes,
                warnings: room_set
                    .as_ref()
                    .map(|room_set| {
                        warnings
                            .iter()
                            .filter(|warning| warning.starts_with(room_set))
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default(),
            });
        }
    }
    Ok(LayoutTransitionIndex {
        patch_version: patch_version.to_owned(),
        release_line: release_line.to_owned(),
        layouts,
        warnings: warnings.into_iter().collect(),
    })
}

fn resolve_graph_transitions(
    graph: &DgrLayoutGraph,
    connections: &[usize],
    rows: &BTreeMap<usize, &ConnectionAreaRow>,
    deferred: bool,
) -> Vec<LayoutNodeTransitions> {
    graph
        .nodes
        .iter()
        .filter_map(|node| {
            let transitions = active_entrances(node)
                .into_iter()
                .map(|(tag, slot)| {
                    let destination = if deferred {
                        None
                    } else {
                        connections
                            .get(slot - 1)
                            .and_then(|index| rows.get(index))
                            .filter(|row| !row.zone.id.is_empty() && row.zone.id != "NULL")
                            .map(|row| row.zone.clone())
                    };
                    LayoutTransition {
                        kind: if deferred {
                            LayoutTransitionKind::Deferred
                        } else if destination.is_some() {
                            LayoutTransitionKind::Zone
                        } else {
                            LayoutTransitionKind::Unresolved
                        },
                        tag: Some(tag),
                        destination,
                        object_path: None,
                        room_paths: Vec::new(),
                        basis: if deferred {
                            "ancient_pyramid_subgraph"
                        } else {
                            "world_areas_connection_slot"
                        }
                        .to_owned(),
                    }
                })
                .collect::<Vec<_>>();
            (!transitions.is_empty()).then_some(LayoutNodeTransitions {
                node_index: node.index,
                transitions,
            })
        })
        .collect()
}

fn active_entrances(node: &DgrLayoutNode) -> Vec<(String, usize)> {
    let count = node
        .metadata
        .first()
        .and_then(|count| count.parse::<usize>().ok())
        .unwrap_or(0);
    node.metadata
        .iter()
        .skip(1)
        .take(count)
        .filter_map(|tag| {
            let lower = tag.to_ascii_lowercase();
            let suffix = lower.strip_prefix("entrance")?;
            if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            let slot = suffix.parse::<usize>().ok()?;
            (slot > 0).then(|| (tag.clone(), slot))
        })
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .collect()
}

type RoomDoors = BTreeMap<String, BTreeMap<String, BTreeSet<String>>>;

fn room_set_doors(
    files_dir: &Path,
    room_set: &str,
    paths: &BTreeSet<String>,
    warnings: &mut BTreeSet<String>,
) -> RoomDoors {
    let mut rooms = RoomDoors::new();
    if !paths.contains(room_set) {
        warnings.insert(format!(
            "{room_set}: room set not extracted; door coverage is unavailable"
        ));
        return rooms;
    }
    let text = match read(files_dir.join(room_set))
        .and_then(|bytes| decode_utf16le_text(room_set, &bytes))
    {
        Ok(text) => text,
        Err(error) => {
            warnings.insert(error.to_string());
            return rooms;
        }
    };
    let room_paths = text
        .lines()
        .filter(|line| !line.trim().starts_with("//"))
        .flat_map(tokenize_dgr_line)
        .filter(|token| token.to_ascii_lowercase().ends_with(".arm"))
        .filter_map(|reference| resolve_reference(room_set, &reference))
        .collect::<BTreeSet<_>>();
    let missing = room_paths
        .iter()
        .filter(|path| !paths.contains(*path))
        .count();
    if missing > 0 {
        warnings.insert(format!(
            "{room_set}: {missing} room dependencies not extracted; door coverage is partial"
        ));
    }
    for room_path in room_paths.iter().filter(|path| paths.contains(*path)) {
        let text = match read(files_dir.join(room_path))
            .and_then(|bytes| decode_utf16le_text(room_path, &bytes))
        {
            Ok(text) => text,
            Err(error) => {
                warnings.insert(error.to_string());
                continue;
            }
        };
        let Some((label, doors)) = parse_room_doors(&text) else {
            continue;
        };
        for door in doors {
            rooms
                .entry(label.clone())
                .or_default()
                .entry(door)
                .or_default()
                .insert(room_path.clone());
        }
    }
    rooms
}

fn parse_room_doors(text: &str) -> Option<(String, BTreeSet<String>)> {
    let lines = text.lines().collect::<Vec<_>>();
    let assets = lines.get(1)?.trim().parse::<usize>().ok()?;
    let label_line = lines.get(assets.checked_add(4)?)?.trim();
    if !label_line.starts_with('"') {
        return None;
    }
    let label = tokenize_dgr_line(label_line).into_iter().next()?;
    let doors = lines
        .iter()
        .filter_map(|line| {
            let tokens = tokenize_dgr_line(line);
            let first_path = tokens
                .iter()
                .position(|token| token.to_ascii_lowercase().starts_with("metadata/"))?;
            if first_path < 10
                || !tokens[..first_path]
                    .iter()
                    .all(|token| token.parse::<f64>().is_ok())
            {
                return None;
            }
            let object = normalize_path(tokens.get(first_path + 1)?);
            let name = object.rsplit('/').next()?;
            (name == "door" || name.ends_with("door") || name.starts_with("door_"))
                .then_some(object)
        })
        .collect();
    Some((label, doors))
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

pub fn refresh_layout_transitions(
    raw_dir: &Path,
    schema_path: &Path,
) -> Result<LayoutTransitionIndex, CampaignScrapeError> {
    #[derive(Deserialize)]
    struct CachedFile {
        logical_path: String,
    }
    #[derive(Deserialize)]
    struct CachedManifest {
        patch_version: String,
        release_line: String,
        selected_areas: Vec<CampaignAreaSummary>,
        candidate_files: Vec<TerrainCandidate>,
        folder_files: Vec<CachedFile>,
        extracted_files: Vec<CachedFile>,
    }
    let manifest_path = raw_dir.join("manifest.json");
    let bytes = read(&manifest_path)?;
    let cached: CachedManifest =
        serde_json::from_slice(&bytes).map_err(|source| CampaignScrapeError::Json {
            path: manifest_path.clone(),
            source,
        })?;
    let paths = cached
        .folder_files
        .iter()
        .chain(&cached.extracted_files)
        .map(|file| file.logical_path.clone())
        .collect::<Vec<_>>();
    let index = scrape_layout_transitions(
        &raw_dir.join("files"),
        &paths,
        &cached.selected_areas,
        &cached.candidate_files,
        schema_path,
        &cached.patch_version,
        &cached.release_line,
    )?;
    write_json(&raw_dir.join("layout-transitions.json"), &index)?;
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|source| CampaignScrapeError::Json {
            path: manifest_path.clone(),
            source,
        })?;
    manifest["layout_transitions"] =
        serde_json::to_value(&index.layouts).map_err(|source| CampaignScrapeError::Json {
            path: manifest_path.clone(),
            source,
        })?;
    manifest["layout_transition_warnings"] =
        serde_json::to_value(&index.warnings).map_err(|source| CampaignScrapeError::Json {
            path: manifest_path.clone(),
            source,
        })?;
    write_json(&manifest_path, &manifest)?;
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_ordered_slots_without_deduplicating_destinations_and_ignores_disabled_tags() {
        let text = "version 25\nSize: 1 1\nMasterFile: \"master.tsi\"\nNodes: 1\nEdges: 0\nDefault%: 100\n0 0 0 \"entrance\" I 5 \"entrance1\" \"entrance2\" \"entrance3disabled\" \"entrance4\" \"entrance0\" 100 0 N";
        let graph = parse_dgr_layout_graph(
            "fixture.dgr",
            &text
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let target = ConnectionAreaRow {
            row_index: 8,
            zone: TransitionZone {
                id: "target".to_owned(),
                name: "Destination".to_owned(),
            },
            connections: Vec::new(),
        };
        let rows = BTreeMap::from([(8, &target)]);
        let nodes = resolve_graph_transitions(&graph, &[8, 8], &rows, false);
        assert_eq!(nodes[0].transitions.len(), 3);
        assert_eq!(
            nodes[0].transitions[0].destination,
            nodes[0].transitions[1].destination
        );
        assert_eq!(
            nodes[0].transitions[2].kind,
            LayoutTransitionKind::Unresolved
        );
        assert!(resolve_graph_transitions(&graph, &[8, 8], &rows, true)[0]
            .transitions
            .iter()
            .all(
                |transition| transition.kind == LayoutTransitionKind::Deferred
                    && transition.destination.is_none()
            ));
    }

    #[test]
    fn reads_room_label_and_door_entity_without_confusing_decorative_assets_or_doormen() {
        let text = "version 36\n1\n\"asset.gt\"\n1 0\n21 12\n\"bossroom\"\n0 0 0 0 0 0 1 0 0 0 1 \"Metadata/Art/boat_door.ao\" \"Metadata/MiscellaneousObjects/Doodad\"\n0 0 0 0 0 0 1 0 0 0 1 \"Metadata/NPC/Doorman.ao\" \"Metadata/NPC/Doorman\"\n0 0 0 0 0 0 1 0 0 0 1 \"Metadata/Art/gate.ao\" \"Metadata/QuestObjects/BanditDoor\"";
        let (label, doors) = parse_room_doors(text).unwrap();
        assert_eq!(label, "bossroom");
        assert_eq!(
            doors,
            BTreeSet::from(["metadata/questobjects/banditdoor".to_owned()])
        );
    }

    #[test]
    fn joins_active_room_candidates_by_internal_label_and_reports_missing_dependencies() {
        let temp = tempfile::tempdir().unwrap();
        let room_set = "metadata/fixture/generate.rs";
        let active = "metadata/fixture/active.arm";
        let disabled = "metadata/fixture/disabled.arm";
        let missing = "metadata/fixture/missing.arm";
        std::fs::create_dir_all(temp.path().join("metadata/fixture")).unwrap();
        for (path, text) in [
            (room_set, "version 2\n100 \"active.arm\"\n//!\"disabled.arm\"\n\"missing.arm\""),
            (active, "version 36\n0\n1 0\n1 1\n\"not_the_filename\"\n0 0 0 0 0 0 1 0 0 0 1 \"Metadata/Art/gate.ao\" \"Metadata/QuestObjects/BanditDoor\""),
            (disabled, "version 36\n0\n1 0\n1 1\n\"disabled_label\"\n0 0 0 0 0 0 1 0 0 0 1 \"Metadata/Art/gate.ao\" \"Metadata/QuestObjects/OtherDoor\""),
        ] {
            std::fs::write(temp.path().join(path), text.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>()).unwrap();
        }
        let paths = [room_set, active, disabled]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let mut warnings = BTreeSet::new();
        let rooms = room_set_doors(temp.path(), room_set, &paths, &mut warnings);
        assert_eq!(rooms.len(), 1);
        assert_eq!(
            rooms["not_the_filename"]["metadata/questobjects/banditdoor"],
            BTreeSet::from([active.to_owned()])
        );
        assert!(!rooms.contains_key("disabled_label"));
        assert_eq!(warnings.len(), 1);
        assert!(warnings
            .first()
            .unwrap()
            .contains("1 room dependencies not extracted"));
        assert!(!paths.contains(missing));
    }
}
