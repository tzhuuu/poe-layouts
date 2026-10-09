import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from xml.etree import ElementTree


ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location("audit", ROOT / "scripts/audit-layout-references.py")
audit = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(audit)


def sample_graph():
    return {
        "logical_path": "example<&.dgr",
        "environment": "indoor",
        "nodes": [
            {"index": 0, "x": 0, "y": 0, "label": "start"},
            {"index": 1, "x": 24, "y": 0, "label": None},
            {"index": 2, "x": 0, "y": 24, "label": "<end>"},
        ],
        "edges": [{"from": 0, "to": 1}, {"from": 0, "to": 2}, {"from": 0, "to": 999}],
        "node_transitions": [],
    }


class ReferenceAuditTests(unittest.TestCase):
    def test_html_tree_handles_void_elements_entities_and_nested_captions(self):
        document = audit.Document('<article class="markdown"><h1>Coast &amp; Sea</h1><table><tr><td>North<br><a><img src="/one.png"></a></td><td><img src="/two.png">South</td></tr></table></article>')
        article = next(node for node in document.root.walk() if node.tag == "article")
        cells = [node for node in article.walk() if node.tag == "td"]
        self.assertEqual(cells[0].text(), "North")
        self.assertEqual(cells[1].text(), "South")
        self.assertEqual([node.attrs["src"] for node in article.walk() if node.tag == "img"], ["/one.png", "/two.png"])
        self.assertEqual(next(node.text() for node in article.walk() if node.tag == "h1"), "Coast & Sea")

    def test_canonical_names_preserve_level_identity(self):
        self.assertEqual(audit.canonical("The Chamber of Sins Level 2"), audit.canonical("Chamber of Sins 2"))
        self.assertNotEqual(audit.canonical("Crypt 2"), audit.canonical("Crypt"))
        self.assertEqual(audit.ALIASES[(2, "Crypt")], "1_2_5_1")

    def test_projection_basis_and_graph_are_independent(self):
        graph = sample_graph()
        original = copy.deepcopy(graph)
        reflected = ElementTree.fromstring(audit.graph_svg(graph, True))
        unreflected = ElementTree.fromstring(audit.graph_svg(graph, False))
        reflected_points = reflected.findall("circle")
        old_points = unreflected.findall("circle")
        self.assertGreater(float(reflected_points[1].attrib["cx"]), 0)
        self.assertLess(float(reflected_points[1].attrib["cy"]), 0)
        self.assertLess(float(reflected_points[2].attrib["cx"]), 0)
        self.assertLess(float(reflected_points[2].attrib["cy"]), 0)
        self.assertGreater(float(old_points[2].attrib["cx"]), 0)
        self.assertGreater(float(old_points[2].attrib["cy"]), 0)
        self.assertEqual(len(reflected.findall("line")), 2)
        self.assertEqual(reflected.findall("text")[-1].text, "<end>")
        self.assertEqual(graph, original)

    def test_empty_and_single_node_graphs_have_valid_bounds(self):
        graph = sample_graph()
        graph["nodes"] = []
        self.assertEqual(audit.graph_svg(graph, True), "<p>No parsed nodes</p>")
        graph["nodes"] = [{"index": 0, "x": 0, "y": 0, "label": None}]
        element = ElementTree.fromstring(audit.graph_svg(graph, True))
        bounds = [float(value) for value in element.attrib["viewBox"].split()]
        self.assertGreater(bounds[2], 0)
        self.assertGreater(bounds[3], 0)

    def test_review_does_not_leak_to_new_pages_or_new_patches(self):
        review = {"reviewed_at": "2026-10-08", "patch_version": "one", "statuses": {"inconclusive": "Unknown"}, "pages": [{"act": 1, "title": "Coast", "status": "inconclusive", "note": "Sparse"}]}
        inventory = {"patch_version": "two", "pages": [{"act": 1, "title": "Coast"}, {"act": 1, "title": "New", "review": {"status_label": "Stale"}}]}
        audit.attach_review(inventory, review)
        self.assertFalse(inventory["review_patch_matches"])
        self.assertEqual(inventory["pages"][0]["review"]["status_label"], "Unknown")
        self.assertNotIn("review", inventory["pages"][1])
        review["pages"].append(review["pages"][0])
        with self.assertRaisesRegex(ValueError, "Duplicate"):
            audit.attach_review(inventory, review)

    def test_coverage_keeps_unreferenced_areas_explicit(self):
        inventory = {"pages": [{"zone_id": "one"}, {}]}
        manifest = {"selected_areas": [{"id": "one", "act": 1, "name": "Coast"}, {"id": "two", "act": 1, "name": "Fetid Pool"}]}
        audit.attach_coverage(inventory, manifest)
        self.assertEqual(inventory["uncovered_areas"], [manifest["selected_areas"][1]])

    def test_viewer_marks_missing_reference_failures_and_patch_mismatch(self):
        inventory = {
            "patch_version": "test", "review_patch_matches": False,
            "errors": [{"error": "missing page"}], "uncovered_areas": [{"name": "<pool>"}],
            "pages": [{"act": 1, "title": "<test>", "url": "https://example.com", "images": [], "graphs": [sample_graph()], "graph_errors": [{"error": "missing graph"}]}],
        }
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            audit.write_viewer(out, inventory)
            index = (out / "index.html").read_text()
            page = (out / "zone-00.html").read_text()
        self.assertIn("2 fetch/parse failures", index)
        self.assertIn("Review patch differs", index)
        self.assertIn("&lt;pool&gt;", index)
        self.assertIn("No reference images", page)
        self.assertIn("&lt;test&gt;", page)
        self.assertIn("data-mode=\"original\"", page)

    def test_checked_in_review_is_complete_and_valid(self):
        review = json.loads((ROOT / "docs/acts-1-5-reference-review.json").read_text())
        keys = [(page["act"], page["title"]) for page in review["pages"]]
        self.assertEqual(len(keys), 73)
        self.assertEqual(len(set(keys)), len(keys))
        self.assertEqual([sum(page["act"] == act for page in review["pages"]) for act in range(1, 6)], [15, 17, 18, 13, 10])
        for page in review["pages"]:
            self.assertIn(page["status"], review["statuses"])
            self.assertTrue(page["note"])


if __name__ == "__main__":
    unittest.main()
