use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{decode_utf16le_text, read, tokenize_dgr_line, write_json, CampaignScrapeError};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LayoutEnvironment {
    Indoor,
    Outdoor,
    Mixed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LayoutEnvironmentSummary {
    pub logical_path: String,
    pub environment: LayoutEnvironment,
    pub basis: String,
    pub master_file: Option<String>,
    pub terrain_root: Option<String>,
    pub room_set: Option<String>,
    pub outer_ground_type: Option<String>,
    pub has_generate_rs: bool,
    pub has_rooms_dir: bool,
    pub has_room_tiles_rs: bool,
    pub has_room_nodes_rs: bool,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LayoutEnvironmentIndex {
    pub patch_version: String,
    pub release_line: String,
    pub layouts: Vec<LayoutEnvironmentSummary>,
    pub warnings: Vec<String>,
}

pub fn scrape_layout_environments(
    files_dir: &Path,
    logical_paths: &[String],
    patch_version: &str,
    release_line: &str,
) -> LayoutEnvironmentIndex {
    let paths = logical_paths
        .iter()
        .map(|path| normalize_path(path))
        .collect::<BTreeSet<_>>();
    let mut warnings = Vec::new();
    let mut tsi_fields = BTreeMap::new();
    for path in paths.iter().filter(|path| path.ends_with(".tsi")) {
        match read(files_dir.join(path)).and_then(|bytes| decode_utf16le_text(path, &bytes)) {
            Ok(text) => {
                tsi_fields.insert(path.clone(), parse_tsi_fields(&text));
            }
            Err(error) => warnings.push(error.to_string()),
        }
    }
    let layouts = paths
        .iter()
        .filter(|path| path.ends_with(".dgr") || path.ends_with(".tgr") || path.ends_with(".tsi"))
        .map(|path| {
            let master_file = if path.ends_with(".tsi") {
                Some(path.clone())
            } else {
                match read(files_dir.join(path)).and_then(|bytes| decode_utf16le_text(path, &bytes))
                {
                    Ok(text) => graph_master_file(&text)
                        .as_deref()
                        .and_then(|reference| resolve_reference(path, reference)),
                    Err(error) => {
                        warnings.push(error.to_string());
                        None
                    }
                }
            };
            let fields = master_file.as_ref().and_then(|path| tsi_fields.get(path));
            if fields.is_none() {
                warnings.push(format!(
                    "{path}: master TSI unavailable; environment is unknown"
                ));
            }
            classify_layout(path, master_file.as_deref(), fields, &paths)
        })
        .collect();
    LayoutEnvironmentIndex {
        patch_version: patch_version.to_owned(),
        release_line: release_line.to_owned(),
        layouts,
        warnings,
    }
}

pub fn refresh_layout_environments(
    raw_dir: &Path,
) -> Result<LayoutEnvironmentIndex, CampaignScrapeError> {
    #[derive(Deserialize)]
    struct CachedFile {
        logical_path: String,
    }
    #[derive(Deserialize)]
    struct CachedManifest {
        patch_version: String,
        release_line: String,
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
    let index = scrape_layout_environments(
        &raw_dir.join("files"),
        &paths,
        &cached.patch_version,
        &cached.release_line,
    );
    write_json(&raw_dir.join("layout-environments.json"), &index)?;
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|source| CampaignScrapeError::Json {
            path: manifest_path.clone(),
            source,
        })?;
    manifest["layout_environments"] =
        serde_json::to_value(&index.layouts).map_err(|source| CampaignScrapeError::Json {
            path: manifest_path.clone(),
            source,
        })?;
    manifest["layout_environment_warnings"] =
        serde_json::to_value(&index.warnings).map_err(|source| CampaignScrapeError::Json {
            path: manifest_path.clone(),
            source,
        })?;
    write_json(&manifest_path, &manifest)?;
    Ok(index)
}

pub(super) fn parse_tsi_fields(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim().trim_start_matches('\u{feff}');
            let (key, value) = line.split_once(char::is_whitespace)?;
            Some((
                key.to_ascii_lowercase(),
                value.trim().trim_matches('"').to_owned(),
            ))
        })
        .collect()
}

fn graph_master_file(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (key, value) = line.trim().trim_start_matches('\u{feff}').split_once(':')?;
        if !key.eq_ignore_ascii_case("MasterFile") {
            return None;
        }
        tokenize_dgr_line(value)
            .into_iter()
            .next()
            .filter(|value| !value.is_empty())
    })
}

fn classify_layout(
    logical_path: &str,
    master_file: Option<&str>,
    fields: Option<&BTreeMap<String, String>>,
    paths: &BTreeSet<String>,
) -> LayoutEnvironmentSummary {
    let terrain_root =
        master_file.and_then(|path| path.rsplit_once('/').map(|(root, _)| root.to_owned()));
    let has_file = |name: &str| {
        terrain_root
            .as_ref()
            .is_some_and(|root| paths.contains(&format!("{root}/{name}")))
    };
    let has_rooms_dir = terrain_root.as_ref().is_some_and(|root| {
        let prefix = format!("{root}/rooms/");
        paths
            .range(prefix.clone()..)
            .next()
            .is_some_and(|path| path.starts_with(&prefix))
    });
    let room_set = fields
        .and_then(|fields| fields.get("roomset"))
        .and_then(|value| master_file.and_then(|master| resolve_reference(master, value)));
    let outer_ground_type = fields
        .and_then(|fields| fields.get("outergroundtype"))
        .cloned();
    let has_generate_rs = has_file("generate.rs");
    let has_room_tiles_rs = has_file("room_tiles.rs");
    let has_room_nodes_rs = has_file("room_nodes.rs");
    let mut summary = LayoutEnvironmentSummary {
        logical_path: logical_path.to_owned(),
        environment: LayoutEnvironment::Unknown,
        basis: "room_set".to_owned(),
        master_file: master_file.map(str::to_owned),
        terrain_root,
        room_set,
        outer_ground_type,
        has_generate_rs,
        has_room_tiles_rs,
        has_room_nodes_rs,
        has_rooms_dir,
        evidence: Vec::new(),
    };
    if let Some(master) = master_file {
        summary.evidence.push(format!("MasterFile: {master}"));
    }
    if let Some(room_set) = &summary.room_set {
        summary.evidence.push(format!("RoomSet: {room_set}"));
    }
    let active_set = summary
        .room_set
        .as_deref()
        .and_then(|path| path.rsplit('/').next());
    let outdoor_room_set = matches!(active_set, Some("room_tiles.rs" | "room_nodes.rs"));
    let generated_room_set = active_set.is_some_and(|name| {
        name == "generate.rs" || (name.starts_with("generate_") && name.ends_with(".rs"))
    });
    summary.environment = if outdoor_room_set {
        LayoutEnvironment::Outdoor
    } else if generated_room_set {
        LayoutEnvironment::Indoor
    } else {
        LayoutEnvironment::Unknown
    };
    summary.evidence.push(
        match summary.environment {
            LayoutEnvironment::Indoor => "Active generate.rs or generate_*.rs room set",
            LayoutEnvironment::Outdoor => "Active room_tiles.rs or room_nodes.rs room set",
            LayoutEnvironment::Mixed => "Mixed room set",
            LayoutEnvironment::Unknown => "Active room set is missing or unrecognized",
        }
        .to_owned(),
    );
    summary
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

pub(super) fn resolve_reference(source: &str, reference: &str) -> Option<String> {
    let reference = normalize_path(reference);
    let joined = if reference.starts_with("metadata/") {
        reference
    } else {
        format!("{}/{}", source.rsplit_once('/')?.0, reference)
    };
    let mut parts = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrapes_each_graphs_master_file_instead_of_guessing_from_its_suffix() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = [
            "metadata/indoor/master.tsi",
            "metadata/indoor/generate.rs",
            "metadata/outdoor/master.tsi",
            "metadata/outdoor/room_tiles.rs",
            "metadata/graphs/indoor.tgr",
            "metadata/graphs/outdoor.dgr",
            "metadata/graphs/missing.dgr",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        let fixtures = [
            (paths[0].as_str(), "RoomSet \"generate.rs\"\nOuterGroundType \"Metadata/Terrain/black_inside_wall.gt\""),
            (paths[2].as_str(), "RoomSet \"room_tiles.rs\""),
            (paths[4].as_str(), "MasterFile: \"Metadata/Indoor/master.tsi\""),
            (paths[5].as_str(), "MasterFile: \"../outdoor/master.tsi\""),
            (paths[6].as_str(), "MasterFile: \"Metadata/Missing/master.tsi\""),
        ];
        for (path, text) in fixtures {
            let path = temp.path().join(path);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            std::fs::write(
                path,
                text.encode_utf16()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
            .expect("write fixture");
        }
        let index = scrape_layout_environments(temp.path(), &paths, "fixture.patch", "fixture");
        let graph = |name: &str| {
            index
                .layouts
                .iter()
                .find(|layout| layout.logical_path == name)
                .expect("graph")
        };
        assert_eq!(graph(&paths[4]).environment, LayoutEnvironment::Indoor);
        assert_eq!(graph(&paths[5]).environment, LayoutEnvironment::Outdoor);
        assert_eq!(graph(&paths[6]).environment, LayoutEnvironment::Unknown);
        assert_eq!(index.warnings.len(), 1);
        assert_eq!(index.patch_version, "fixture.patch");
        assert_eq!(index.layouts.len(), 5);
    }

    #[test]
    fn classifies_by_active_room_set_without_boundary_or_inventory_overrides() {
        let master = "metadata/terrain/fixture/master.tsi";
        let mut paths = [
            master,
            "metadata/terrain/fixture/generate.rs",
            "metadata/terrain/fixture/room_nodes.rs",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
        let indoor = parse_tsi_fields(
            "RoomSet\t\"generate.rs\"\nOuterGroundType \"Metadata/Terrain/black_inside_wall.gt\"",
        );
        assert_eq!(
            classify_layout("fixture.dgr", Some(master), Some(&indoor), &paths).environment,
            LayoutEnvironment::Indoor
        );
        let outdoor = parse_tsi_fields(
            "RoomSet \"room_nodes.rs\"\nOuterGroundType \"Metadata/Terrain/black_inside_wall.gt\"",
        );
        assert_eq!(
            classify_layout("fixture.dgr", Some(master), Some(&outdoor), &paths).environment,
            LayoutEnvironment::Outdoor
        );
        paths.insert("metadata/terrain/fixture/rooms/local.arm".to_owned());
        assert_eq!(
            classify_layout("fixture.dgr", Some(master), Some(&indoor), &paths).environment,
            LayoutEnvironment::Indoor
        );
        assert_eq!(
            classify_layout("fixture.tgr", Some(master), None, &paths).environment,
            LayoutEnvironment::Unknown
        );
        let exposed = parse_tsi_fields(
            "RoomSet \"generate.rs\"\nOuterGroundType \"Metadata/Terrain/sky.gt\"",
        );
        paths.remove("metadata/terrain/fixture/rooms/local.arm");
        assert_eq!(
            classify_layout("fixture.dgr", Some(master), Some(&exposed), &paths).environment,
            LayoutEnvironment::Indoor
        );
        paths.remove("metadata/terrain/fixture/generate.rs");
        assert_eq!(
            classify_layout("fixture.dgr", Some(master), Some(&indoor), &paths).environment,
            LayoutEnvironment::Indoor
        );
        for (room_set, expected) in [
            ("generate_boss.rs", LayoutEnvironment::Indoor),
            ("room_tiles.rs", LayoutEnvironment::Outdoor),
            ("other.rs", LayoutEnvironment::Unknown),
            ("generate_invalid.txt", LayoutEnvironment::Unknown),
        ] {
            let fields = parse_tsi_fields(&format!("RoomSet \"{room_set}\""));
            let summary = classify_layout("fixture.dgr", Some(master), Some(&fields), &paths);
            assert_eq!(summary.environment, expected, "{room_set}");
            assert_eq!(summary.basis, "room_set");
        }
    }

    #[test]
    fn resolves_absolute_and_relative_tsi_references() {
        assert_eq!(
            resolve_reference(
                "metadata/terrain/test/master.tsi",
                "Metadata\\Terrain\\Other\\generate.rs"
            )
            .as_deref(),
            Some("metadata/terrain/other/generate.rs")
        );
        assert_eq!(
            resolve_reference("metadata/terrain/test/graphs/layout.dgr", "../master.tsi")
                .as_deref(),
            Some("metadata/terrain/test/master.tsi")
        );
        assert_eq!(
            resolve_reference("metadata/test.tsi", "../../../escape.rs"),
            None
        );
    }
}
