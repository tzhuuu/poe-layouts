use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;

use crate::layout_environment::{parse_tsi_fields, resolve_reference};
use crate::{decode_utf16le_text, read, tokenize_dgr_line, DgrLayoutGraph};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RoomVariant {
    pub logical_path: String,
    pub label: String,
    pub version: usize,
    pub width: usize,
    pub height: usize,
    pub asset_count: usize,
    pub boss_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RoomCatalog {
    pub room_set: Option<String>,
    pub variants: Vec<RoomVariant>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LayoutNodeBosses {
    pub node_index: usize,
    pub room_paths: Vec<String>,
    pub boss_tags: Vec<String>,
    pub candidate_count: usize,
}

pub fn resolve_room_bosses(graph: &DgrLayoutGraph, catalog: &RoomCatalog) -> Vec<LayoutNodeBosses> {
    graph
        .nodes
        .iter()
        .filter_map(|node| {
            let label = node.label.as_ref()?;
            let candidates = catalog
                .variants
                .iter()
                .filter(|room| room.label == *label)
                .collect::<Vec<_>>();
            let boss_rooms = candidates
                .iter()
                .filter(|room| !room.boss_tags.is_empty())
                .collect::<Vec<_>>();
            if boss_rooms.is_empty() {
                return None;
            }
            Some(LayoutNodeBosses {
                node_index: node.index,
                room_paths: boss_rooms
                    .iter()
                    .map(|room| room.logical_path.clone())
                    .collect(),
                boss_tags: boss_rooms
                    .iter()
                    .flat_map(|room| room.boss_tags.iter().cloned())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                candidate_count: candidates.len(),
            })
        })
        .collect()
}

pub fn inspect_layout_rooms(files_dir: &Path, graph: &DgrLayoutGraph) -> RoomCatalog {
    let mut catalog = RoomCatalog {
        room_set: None,
        variants: Vec::new(),
        warnings: Vec::new(),
    };
    let Some(master) = graph
        .master_file
        .as_deref()
        .and_then(|reference| resolve_reference(&graph.logical_path, reference))
    else {
        catalog
            .warnings
            .push("No master TSI for this layout".to_owned());
        return catalog;
    };
    let master_text = match read(files_dir.join(&master))
        .and_then(|bytes| decode_utf16le_text(&master, &bytes))
    {
        Ok(text) => text,
        Err(error) => {
            catalog.warnings.push(error.to_string());
            return catalog;
        }
    };
    let Some(room_set) = parse_tsi_fields(&master_text)
        .get("roomset")
        .and_then(|reference| resolve_reference(&master, reference))
    else {
        catalog
            .warnings
            .push(format!("{master}: no active RoomSet"));
        return catalog;
    };
    catalog.room_set = Some(room_set.clone());
    let text = match read(files_dir.join(&room_set))
        .and_then(|bytes| decode_utf16le_text(&room_set, &bytes))
    {
        Ok(text) => text,
        Err(error) => {
            catalog.warnings.push(error.to_string());
            return catalog;
        }
    };
    let paths = text
        .lines()
        .filter(|line| !line.trim().starts_with("//"))
        .flat_map(tokenize_dgr_line)
        .filter(|token| token.to_ascii_lowercase().ends_with(".arm"))
        .filter_map(|reference| resolve_reference(&room_set, &reference))
        .collect::<BTreeSet<_>>();
    for path in paths {
        match read(files_dir.join(&path)).and_then(|bytes| decode_utf16le_text(&path, &bytes)) {
            Ok(text) => match parse_room_variant(&path, &text) {
                Some(variant) => catalog.variants.push(variant),
                None => catalog
                    .warnings
                    .push(format!("{path}: unsupported ARM header")),
            },
            Err(error) => catalog.warnings.push(error.to_string()),
        }
    }
    catalog
}

fn parse_room_variant(path: &str, text: &str) -> Option<RoomVariant> {
    let lines = text.lines().collect::<Vec<_>>();
    let version = lines
        .first()?
        .trim()
        .trim_start_matches('\u{feff}')
        .strip_prefix("version ")?
        .trim()
        .parse()
        .ok()?;
    let asset_count = lines.get(1)?.trim().parse::<usize>().ok()?;
    let dimensions_line = asset_count.checked_add(3)?;
    let dimensions = tokenize_dgr_line(lines.get(dimensions_line)?);
    if dimensions.len() != 2 {
        return None;
    }
    let width = dimensions[0].parse().ok()?;
    let height = dimensions[1].parse().ok()?;
    let label_line = lines.get(dimensions_line.checked_add(1)?)?.trim();
    if !label_line.starts_with('"') {
        return None;
    }
    let label = tokenize_dgr_line(label_line).into_iter().next()?;
    let boss_tags = lines
        .iter()
        .skip(dimensions_line + 2)
        .filter_map(|line| {
            let tokens = tokenize_dgr_line(line);
            if tokens.len() != 4
                || !tokens[..3]
                    .iter()
                    .all(|token| token.parse::<f64>().is_ok_and(f64::is_finite))
            {
                return None;
            }
            let tag = tokens[3].to_ascii_lowercase();
            (tag == "mapboss").then_some(tag)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Some(RoomVariant {
        logical_path: path.to_owned(),
        label,
        version,
        width,
        height,
        asset_count,
        boss_tags,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_internal_label_and_dimensions_not_filename() {
        let room = parse_room_variant(
            "fixture.arm",
            "\u{feff}version 30\n2\n\"a.et\"\n\"b.gt\"\n1 0\n7 6\n\"camp\"\n",
        )
        .unwrap();
        assert_eq!(room.label, "camp");
        assert_eq!(
            (room.width, room.height, room.asset_count, room.version),
            (7, 6, 2, 30)
        );
        assert!(parse_room_variant("bad.arm", "version 30\n999999999999999999\n").is_none());
        assert!(parse_room_variant("bad.arm", "version 30\n0\n1 0\n7 x\n\"camp\"").is_none());
    }

    #[test]
    fn resolves_only_active_unique_candidates_and_reports_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("metadata/fixture");
        std::fs::create_dir_all(&root).unwrap();
        for (name, text) in [
            ("master.tsi", "RoomSet \"generate.rs\""),
            (
                "generate.rs",
                "version 2\n100 \"one.arm\"\n\"one.arm\"\n//!\"disabled.arm\"\n\"missing.arm\"",
            ),
            (
                "one.arm",
                "version 36\n0\n1 0\n21 12\n\"bossroom\"\n32 106 0.43 \"mapboss\"",
            ),
            ("disabled.arm", "version 36\n0\n1 0\n1 1\n\"disabled\""),
        ] {
            let bytes = text
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            std::fs::write(root.join(name), bytes).unwrap();
        }
        let graph = crate::parse_dgr_layout_graph("metadata/fixture/graph.dgr", &"version 19\nMasterFile: \"master.tsi\"\nNodes: 1\nEdges: 0\n\"\"\n\"\"\n\"\"\n0 0 0 \"bossroom\" I 0 100\n".encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>()).unwrap();
        let mut catalog = inspect_layout_rooms(dir.path(), &graph);
        assert_eq!(
            catalog.room_set.as_deref(),
            Some("metadata/fixture/generate.rs")
        );
        assert_eq!(catalog.variants.len(), 1);
        assert_eq!(catalog.variants[0].label, "bossroom");
        let bosses = resolve_room_bosses(&graph, &catalog);
        assert_eq!(bosses.len(), 1);
        assert_eq!(bosses[0].node_index, 0);
        assert_eq!(bosses[0].candidate_count, 1);
        assert_eq!(bosses[0].boss_tags, vec!["mapboss"]);
        let mut ordinary = catalog.variants[0].clone();
        ordinary.logical_path = "metadata/fixture/plain.arm".to_owned();
        ordinary.boss_tags.clear();
        catalog.variants.push(ordinary);
        let mixed = resolve_room_bosses(&graph, &catalog);
        assert_eq!(mixed[0].candidate_count, 2);
        assert_eq!(mixed[0].room_paths.len(), 1);
        catalog.variants[0].boss_tags.clear();
        assert!(resolve_room_bosses(&graph, &catalog).is_empty());
        assert_eq!(catalog.warnings.len(), 1);
        assert!(catalog.warnings[0].contains("missing.arm"));
    }

    #[test]
    fn reads_mapboss_spawn_hooks_without_matching_asset_names_or_disabled_hooks() {
        let room = parse_room_variant("camp.arm", "version 30\n1\n\"Metadata/Art/mapboss.ao\"\n1 0\n7 6\n\"camp\"\n32 106 0.43 \"MapBoss\"\n32 106 0.43 \"mapboss\"\n32 106 0.43 \"mapboss_disabled\"\n// 32 106 0.43 \"mapboss\"\nnot coordinates here \"mapboss\"").unwrap();
        assert_eq!(room.boss_tags, vec!["mapboss"]);
        let ordinary = parse_room_variant(
            "plain.arm",
            "version 30\n1\n\"mapboss\"\n1 0\n7 6\n\"camp\"\n32 106 0.43 \"mapboss_disabled\"",
        )
        .unwrap();
        assert!(ordinary.boss_tags.is_empty());
    }
}
