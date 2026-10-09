use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::layout_environment::{parse_tsi_fields, resolve_reference};
use crate::{decode_utf16le_text, parse_dgr_layout_graph, tokenize_dgr_line, DgrLayoutGraph};

const MAX_DEPTH: usize = 16;
const MAX_VISITS: usize = 4096;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LayoutCandidate {
    pub logical_path: String,
    pub group: Vec<String>,
    pub root_paths: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct LayoutCandidates {
    pub candidates: Vec<LayoutCandidate>,
    pub warnings: Vec<String>,
}

pub fn resolve_layout_candidates(files_dir: &Path, roots: &[String]) -> LayoutCandidates {
    let mut resolver = Resolver {
        files_dir,
        result: LayoutCandidates::default(),
        visits: 0,
    };
    for root in roots {
        let root = root.replace('\\', "/").to_ascii_lowercase();
        resolver.visit(&root, &root, &[], &mut Vec::new());
    }
    resolver.result.warnings.sort();
    resolver.result.warnings.dedup();
    resolver.result
}

struct Resolver<'a> {
    files_dir: &'a Path,
    result: LayoutCandidates,
    visits: usize,
}

impl Resolver<'_> {
    fn read_bytes(&self, path: &str) -> Result<Vec<u8>, String> {
        if !safe_path(path) {
            return Err(format!("Unsafe layout reference: {path}"));
        }
        fs::read(self.files_dir.join(path)).map_err(|error| format!("{path}: {error}"))
    }

    fn read_text(&self, path: &str) -> Result<String, String> {
        let bytes = self.read_bytes(path)?;
        decode_utf16le_text(path, &bytes).map_err(|error| error.to_string())
    }

    fn add(&mut self, path: &str, root: &str, group: &[String]) {
        if let Some(candidate) = self
            .result
            .candidates
            .iter_mut()
            .find(|item| item.logical_path == path)
        {
            if !candidate.root_paths.iter().any(|item| item == root) {
                candidate.root_paths.push(root.to_owned());
            }
        } else {
            self.result.candidates.push(LayoutCandidate {
                logical_path: path.to_owned(),
                group: group.to_vec(),
                root_paths: vec![root.to_owned()],
            });
        }
    }

    fn visit(
        &mut self,
        path: &str,
        root: &str,
        group: &[String],
        ancestors: &mut Vec<String>,
    ) -> bool {
        self.visits += 1;
        if self.visits > MAX_VISITS
            || ancestors.len() >= MAX_DEPTH
            || ancestors.iter().any(|item| item == path)
        {
            self.result
                .warnings
                .push(format!("{path}: cyclic or excessive subgraph nesting"));
            return false;
        }
        let graph = match self.read_bytes(path).and_then(|bytes| {
            if !is_graph(path) {
                return Err(format!("{path}: expected a DGR or TGR reference"));
            }
            parse_dgr_layout_graph(path, &bytes).map_err(|error| error.to_string())
        }) {
            Ok(graph) => graph,
            Err(error) => {
                self.result.warnings.push(error);
                return false;
            }
        };
        self.result.warnings.extend(
            graph
                .warnings
                .iter()
                .map(|warning| format!("{path}: {warning}")),
        );
        let children = graph
            .nodes
            .iter()
            .filter(|node| {
                node.label
                    .as_deref()
                    .is_some_and(|label| label.eq_ignore_ascii_case("graph"))
            })
            .collect::<Vec<_>>();
        if children.is_empty() {
            self.add(path, root, group);
            return true;
        }
        let wrapper = graph.edges.is_empty() && children.len() == graph.nodes.len();
        let overview = graph.edges.is_empty() && !wrapper;
        if !wrapper && !overview {
            let mut main_group = group.to_vec();
            if main_group.is_empty() {
                main_group.push("Main".to_owned());
            }
            self.add(path, root, &main_group);
        }
        ancestors.push(path.to_owned());
        let mut complete = true;
        let mut file_groups = None;
        for node in children {
            // Node metadata retains the argument count before the subgraph reference.
            let reference = node
                .metadata
                .first()
                .and_then(|count| count.parse::<usize>().ok())
                .filter(|count| *count > 0 && *count < node.metadata.len())
                .and_then(|_| node.metadata.get(1));
            let Some(reference) = reference else {
                self.result.warnings.push(format!(
                    "{path}: graph node {} has no subgraph reference",
                    node.index
                ));
                complete = false;
                continue;
            };
            let mut child_group = group.to_vec();
            let paths = if is_graph(reference) {
                child_group.push(format!("Section {}", node.index + 1));
                reference_path(path, reference).map(|path| vec![path])
            } else {
                child_group.push(reference.clone());
                let groups = file_groups.get_or_insert_with(|| self.file_groups(&graph));
                match groups {
                    Ok(groups) => groups
                        .get(reference)
                        .cloned()
                        .ok_or_else(|| format!("{path}: missing file group {reference}")),
                    Err(error) => Err(error.clone()),
                }
            };
            match paths {
                Ok(paths) if !paths.is_empty() => {
                    for child in paths {
                        complete &= self.visit(&child, root, &child_group, ancestors);
                    }
                }
                Ok(_) => {
                    self.result
                        .warnings
                        .push(format!("{path}: empty file group {reference}"));
                    complete = false;
                }
                Err(error) => {
                    self.result.warnings.push(error);
                    complete = false;
                }
            }
        }
        ancestors.pop();
        if overview {
            let mut overview_group = group.to_vec();
            overview_group.push("Overview".to_owned());
            self.add(path, root, &overview_group);
        }
        if wrapper && !complete {
            self.add(path, root, group);
        }
        complete
    }

    fn file_groups(&self, graph: &DgrLayoutGraph) -> Result<BTreeMap<String, Vec<String>>, String> {
        let master = graph
            .master_file
            .as_ref()
            .ok_or_else(|| format!("{}: missing master TSI", graph.logical_path))?;
        let master = reference_path(&graph.logical_path, master)?;
        let fields = parse_tsi_fields(&self.read_text(&master)?);
        let reference = fields
            .get("filegroups")
            .ok_or_else(|| format!("{master}: missing FileGroups declaration"))?;
        let groups_path = reference_path(&master, reference)?;
        parse_file_groups(&groups_path, &self.read_text(&groups_path)?)
    }
}

fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(':')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn is_graph(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    path.ends_with(".dgr") || path.ends_with(".tgr")
}

fn reference_path(source: &str, reference: &str) -> Result<String, String> {
    if reference.starts_with(['/', '\\']) || reference.contains(':') {
        return Err(format!("{source}: unsafe reference {reference}"));
    }
    resolve_reference(source, reference)
        .filter(|path| safe_path(path))
        .ok_or_else(|| format!("{source}: unsafe reference {reference}"))
}

fn parse_file_groups(path: &str, text: &str) -> Result<BTreeMap<String, Vec<String>>, String> {
    let mut lines = text
        .trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//"));
    if lines.next() != Some("version 1") {
        return Err(format!("{path}: unsupported file group version"));
    }
    let mut groups = BTreeMap::<String, Vec<String>>::new();
    let mut group = None;
    for line in lines {
        let tokens = tokenize_dgr_line(line);
        let Some(value) = tokens.first() else {
            continue;
        };
        if is_graph(value) {
            let name = group
                .as_ref()
                .ok_or_else(|| format!("{path}: graph outside a file group"))?;
            groups
                .get_mut(name)
                .expect("declared group")
                .push(reference_path(path, value)?);
        } else if line.starts_with('"') && tokens.len() == 1 {
            group = Some(value.clone());
            groups.entry(value.clone()).or_default();
        } else {
            return Err(format!("{path}: unsupported file group entry {line}"));
        }
    }
    Ok(groups)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, path: &str, text: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            text.encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    }

    fn graph(root: &Path, path: &str, nodes: &str, count: usize) {
        write(root, path, &format!("version 19\nSize: 7 7\nMasterFile: \"Metadata/Test/master.tsi\"\nNodes: {count}\nEdges: 0\n\"\"\n\"\"\n\"\"\nDefault%: 0 0 0\n{nodes}\n"));
    }

    fn leaf(root: &Path, path: &str) {
        graph(root, path, "1 1 0 \"room\" I 0 100 0 N", 1);
    }

    #[test]
    fn resolves_aliases_and_direct_paths_without_listing_unreferenced_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(
            root,
            "metadata/test/master.tsi",
            "version 3\nFileGroups \"filegroups.fgp\"\n",
        );
        write(root, "metadata/test/filegroups.fgp", "\u{feff}version 1\n\"Prison\"\nGraphs/first.dgr\nGraphs/second.dgr\n\"Unused\"\nGraphs/unrelated.dgr\n");
        graph(root, "metadata/test/graphs/wrapper.dgr", "1 1 0 \"graph\" (any) 3 \"Prison\" \"7\" \"7\" 100 0 N\n2 2 0 \"graph\" I 3 \"Metadata/Test/Graphs/boss.tgr\" \"7\" \"7\" 100 0 N", 2);
        for name in ["first.dgr", "second.dgr", "boss.tgr", "unrelated.dgr"] {
            leaf(root, &format!("metadata/test/graphs/{name}"));
        }
        let result = resolve_layout_candidates(root, &["metadata/test/graphs/wrapper.dgr".into()]);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert_eq!(result.candidates.len(), 3);
        assert_eq!(result.candidates[0].group, ["Prison"]);
        assert!(result.candidates[2].logical_path.ends_with("boss.tgr"));
    }

    #[test]
    fn keeps_mixed_parent_and_deduplicates_children_with_root_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let nodes = "1 1 0 \"entrance\" I 0 100 0 N\n2 2 0 \"graph\" I 3 \"Metadata/Test/Graphs/child.dgr\" \"7\" \"7\" 100 0 N";
        graph(root, "metadata/test/graphs/a.dgr", nodes, 2);
        graph(root, "metadata/test/graphs/b.dgr", nodes, 2);
        leaf(root, "metadata/test/graphs/child.dgr");
        let result = resolve_layout_candidates(
            root,
            &[
                "metadata/test/graphs/a.dgr".into(),
                "metadata/test/graphs/b.dgr".into(),
            ],
        );
        assert_eq!(result.candidates.len(), 3);
        assert_eq!(result.candidates[0].root_paths.len(), 2);
        assert_eq!(result.candidates[1].group, ["Overview"]);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn broken_references_and_cycles_keep_the_wrapper_visible_with_warnings() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        graph(root, "metadata/test/graphs/wrapper.dgr", "1 1 0 \"graph\" I 3 \"Metadata/Test/Graphs/wrapper.dgr\" \"7\" \"7\" 100 0 N\n2 2 0 \"graph\" I 3 \"MissingAlias\" \"7\" \"7\" 100 0 N", 2);
        let result = resolve_layout_candidates(root, &["metadata/test/graphs/wrapper.dgr".into()]);
        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.warnings.len(), 2);
    }

    #[test]
    fn rejects_unsafe_paths_and_resolves_safe_relative_references() {
        for path in ["../outside.dgr", "/outside.dgr", "c:/outside.dgr"] {
            assert!(!safe_path(path));
        }
        assert!(reference_path("metadata/test/master.tsi", "../../../outside.dgr").is_err());
        assert!(reference_path("metadata/test/master.tsi", "/outside.dgr").is_err());
        assert_eq!(
            reference_path("metadata/test/graphs/wrapper.dgr", "../child.dgr").unwrap(),
            "metadata/test/child.dgr"
        );
    }

    #[test]
    fn resolves_nested_wrappers_and_retains_group_order() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        graph(
            root,
            "metadata/test/graphs/root.dgr",
            "1 1 0 \"graph\" I 3 \"Metadata/Test/Graphs/nested.dgr\" \"7\" \"7\" 100 0 N",
            1,
        );
        graph(
            root,
            "metadata/test/graphs/nested.dgr",
            "1 1 0 \"graph\" I 3 \"Metadata/Test/Graphs/leaf.dgr\" \"7\" \"7\" 100 0 N",
            1,
        );
        leaf(root, "metadata/test/graphs/leaf.dgr");
        let result = resolve_layout_candidates(root, &["metadata/test/graphs/root.dgr".into()]);
        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].group, ["Section 1", "Section 1"]);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn missing_child_keeps_partial_results_and_the_parent() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        graph(root, "metadata/test/graphs/root.dgr", "1 1 0 \"graph\" I 3 \"Metadata/Test/Graphs/leaf.dgr\" \"7\" \"7\" 100 0 N\n2 2 0 \"graph\" I 3 \"Metadata/Test/Graphs/missing.dgr\" \"7\" \"7\" 100 0 N", 2);
        leaf(root, "metadata/test/graphs/leaf.dgr");
        let result = resolve_layout_candidates(root, &["metadata/test/graphs/root.dgr".into()]);
        assert_eq!(result.candidates.len(), 2);
        assert!(result.candidates[1].logical_path.ends_with("root.dgr"));
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    #[ignore = "requires the local Acts 1-5 raw cache"]
    fn resolves_cached_prison_and_pyramid() {
        let files = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.poe-layouts/raw/campaign-acts-1-5/files");
        for (path, count, first_group) in [
            (
                "metadata/terrain/act1/area7level2/graphs/prisonmain_1_1.dgr",
                11,
                "Prison",
            ),
            (
                "metadata/terrain/act2/area14level3/graphs/pyramid_main_1_1.dgr",
                12,
                "Level1",
            ),
        ] {
            let result = resolve_layout_candidates(&files, &[path.to_owned()]);
            assert!(result.warnings.is_empty(), "{path}: {:?}", result.warnings);
            assert_eq!(result.candidates.len(), count, "{path}");
            assert_eq!(result.candidates[0].group, [first_group]);
            assert!(result
                .candidates
                .iter()
                .all(|candidate| candidate.logical_path != path));
        }
    }

    #[test]
    fn connected_parent_remains_the_main_layout() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "metadata/test/graphs/main.dgr", "version 19\nSize: 7 7\nNodes: 2\nEdges: 1\n\"\"\n\"\"\n\"\"\nDefault%: 0 0 0\n1 1 1 0 \"room\" I 0 100 0 N\n2 2 1 0 \"graph\" I 3 \"Metadata/Test/Graphs/leaf.dgr\" \"7\" \"7\" 100 0 N\n0 1 0 100 0 \"rooms.et\" 0 0\n");
        leaf(root, "metadata/test/graphs/leaf.dgr");
        let result = resolve_layout_candidates(root, &["metadata/test/graphs/main.dgr".into()]);
        assert_eq!(result.candidates.len(), 2);
        assert_eq!(result.candidates[0].group, ["Main"]);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    }
}
