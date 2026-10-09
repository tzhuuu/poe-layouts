"""Build a local, source-linked comparison inventory; do not download images."""

import argparse
import datetime
import html
from html.parser import HTMLParser
import json
import math
from pathlib import Path
import re
import urllib.parse
import urllib.request
from typing import Any


class Element:
    def __init__(self, tag: str, attrs=()):
        self.tag = tag
        self.attrs = dict(attrs)
        self.children = []

    def walk(self):
        yield self
        for child in self.children:
            if isinstance(child, Element):
                yield from child.walk()

    def text(self) -> str:
        return " ".join(child.text() if isinstance(child, Element) else child for child in self.children).strip()


class Document(HTMLParser):
    VOID = {"area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"}

    def __init__(self, source: str):
        super().__init__(convert_charrefs=True)
        self.root = Element("document")
        self.stack = [self.root]
        self.feed(source)

    def handle_starttag(self, tag, attrs):
        node = Element(tag, attrs)
        self.stack[-1].children.append(node)
        if tag not in self.VOID:
            self.stack.append(node)

    def handle_endtag(self, tag):
        for index in range(len(self.stack) - 1, 0, -1):
            if self.stack[index].tag == tag:
                del self.stack[index:]
                break

    def handle_data(self, data):
        self.stack[-1].children.append(data)


def fetch(url: str) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": "poe-layouts-reference-audit/1.0"})
    with urllib.request.urlopen(request, timeout=30) as response:
        return response.read()


def canonical(name: str) -> str:
    return re.sub(r"[^a-z0-9]", "", name.lower().replace("the ", "").replace("level ", ""))


ALIASES = {
    (1, "Climb"): "1_1_6", (1, "Ledge"): "1_1_5",
    (2, "Weaver's Chamber"): "1_2_10", (2, "Crypt"): "1_2_5_1",
    (2, "Crypt 2"): "1_2_5_2", (2, "Chamber of Sins 1"): "1_2_6_1",
    (2, "Chamber of Sins 2"): "1_2_6_2", (3, "Imperial Garden"): "1_3_15",
    (4, "Mines"): "1_4_3_1", (4, "Mines 2"): "1_4_3_2",
    (4, "Harvest"): "1_4_6_3", (4, "Belly of the Beast 1"): "1_4_6_1",
    (4, "Belly of the Beast 2"): "1_4_6_2",
}


def graph_svg(graph: dict[str, Any], reflected: bool) -> str:
    unit = 1 / math.sqrt(2)
    positions = {}
    for node in graph["nodes"]:
        x, y = node["x"], node["y"]
        positions[node["index"]] = ((x - y if reflected else x + y) * unit,
                                     (-x - y if reflected else y - x) * unit)
    if not positions:
        return "<p>No parsed nodes</p>"
    xs, ys = zip(*positions.values())
    span = max(max(xs) - min(xs), max(ys) - min(ys), 1)
    margin = span * 0.12
    min_x, min_y = min(xs) - margin, min(ys) - margin
    width, height = max(xs) - min(xs) + margin * 2, max(ys) - min(ys) + margin * 2
    font = span * 0.025
    parts = [f'<svg viewBox="{min_x} {min_y} {width} {height}" role="img" aria-label="{html.escape(graph["logical_path"])}">']
    for edge in graph["edges"]:
        if edge["from"] not in positions or edge["to"] not in positions:
            continue
        x1, y1 = positions[edge["from"]]
        x2, y2 = positions[edge["to"]]
        tile = edge.get("edge_tile") or ""
        color = "#38bdf8" if "shore" in tile or "river" in tile else "#c4a06a" if "cliff" in tile else "#a3a3a3"
        parts.append(f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{color}" stroke-width="{span * 0.004}"/>')
    transitions = {entry["node_index"]: entry["transitions"] for entry in graph.get("node_transitions", [])}
    for node in graph["nodes"]:
        x, y = positions[node["index"]]
        tag = node.get("label") or ""
        destinations = [t["destination"]["name"] for t in transitions.get(node["index"], []) if t.get("destination")]
        label = "/".join(destinations) or tag
        color = "#74e0af" if destinations else "#f6d572" if tag else "#b3b3b3"
        parts.append(f'<circle cx="{x}" cy="{y}" r="{span * 0.007}" fill="{color}"><title>{html.escape(str(node))}</title></circle>')
        if label:
            parts.append(f'<text x="{x}" y="{y - font * 0.7}" fill="{color}" font-size="{font}" text-anchor="middle">{html.escape(label)}</text>')
    parts.append("</svg>")
    return "".join(parts)


STYLE = """
body{margin:0;background:#fff;color:#181818;font:14px system-ui;letter-spacing:0}
header{padding:14px 24px;border-bottom:1px solid #bbb;display:flex;align-items:center;gap:18px;position:sticky;top:0;background:white;z-index:2}
h1{font-size:22px;margin:0}h2{font-size:17px;margin:18px 0 12px}a{color:#116746}main{padding:0 24px 24px}small{color:#555}
.refs{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:12px}.graphs{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:12px}
figure{margin:0;min-width:0}figcaption{padding:6px 0;font-size:12px;overflow-wrap:anywhere}
.refs img{width:100%;height:440px;object-fit:contain;background:#16191c}.graphs svg{width:100%;height:310px;background:#101414}
.graphs figure[data-mode]{display:none}body[data-mode=reflected] figure[data-mode=reflected],body[data-mode=original] figure[data-mode=original]{display:block}
select{padding:4px}.inventory-table{overflow-x:auto}table{border-collapse:collapse;width:100%;min-width:680px}th,td{text-align:left;padding:8px;border-bottom:1px solid #ccc}
@media(max-width:1000px){.refs,.graphs{grid-template-columns:repeat(2,minmax(0,1fr))}header{flex-wrap:wrap}}
@media(max-width:600px){.refs,.graphs{grid-template-columns:minmax(0,1fr)}header{padding:12px}main{padding:0 12px 12px}}
"""


def attach_review(inventory: dict[str, Any], review: dict[str, Any]) -> None:
    entries = {(entry["act"], entry["title"]): entry for entry in review["pages"]}
    if len(entries) != len(review["pages"]):
        raise ValueError("Duplicate zone reviews")
    inventory["reviewed_at"] = review["reviewed_at"]
    inventory["review_patch_matches"] = inventory["patch_version"] == review["patch_version"]
    for page in inventory["pages"]:
        page.pop("review", None)
        entry = entries.get((page["act"], page["title"]))
        if entry:
            page["review"] = {**entry, "status_label": review["statuses"][entry["status"]]}


def attach_coverage(inventory: dict[str, Any], manifest: dict[str, Any]) -> None:
    matched = {page.get("zone_id") for page in inventory["pages"]}
    inventory["uncovered_areas"] = [
        {"act": area["act"], "id": area["id"], "name": area["name"]}
        for area in manifest["selected_areas"] if area["id"] not in matched
    ]


def write_viewer(out: Path, inventory: dict[str, Any]) -> None:
    index_rows = []
    for number, page in enumerate(inventory["pages"]):
        name = f"zone-{number:02d}.html"
        review = page.get("review", {})
        status = html.escape(review.get("status_label", "Not reviewed"))
        note = html.escape(review.get("note", ""))
        index_rows.append(f'<tr><td>{page["act"]}</td><td><a href="{name}">{html.escape(page["title"])}</a></td><td>{len(page["images"])}</td><td>{len(page["graphs"])}</td><td>{status}</td></tr>')
        refs = []
        for index, image in enumerate(page["images"]):
            refs.append(f'<figure><img src="{html.escape(image["url"])}" alt="Reference {index + 1}" loading="eager"><figcaption>Reference {index + 1}: {html.escape(image["caption"] or image["alt"])}</figcaption></figure>')
        graphs = []
        for graph in page["graphs"]:
            for mode in ["reflected", "original"]:
                graph_name = Path(graph["logical_path"]).name
                graphs.append(f'<figure data-mode="{mode}">{graph_svg(graph, mode == "reflected")}<figcaption>{html.escape(graph_name)} | {graph.get("environment", "unknown")} | {len(graph["nodes"])} nodes</figcaption></figure>')
        previous = f'<a href="zone-{number - 1:02d}.html" aria-label="Previous zone">&larr;</a>' if number else ""
        following = f'<a href="zone-{number + 1:02d}.html" aria-label="Next zone">&rarr;</a>' if number + 1 < len(inventory["pages"]) else ""
        current_modes = {"reflected" if g.get("environment") == "outdoor" else "original" for g in page["graphs"]}
        current = ", ".join(sorted(current_modes)) or "none"
        content = f'''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>{html.escape(page["title"])} comparison</title><style>{STYLE}</style>
<body data-mode="reflected"><header><a href="index.html">Acts 1-5</a>{previous}{following}<h1>Act {page["act"]}: {html.escape(page["title"])}</h1><a href="{html.escape(page["url"])}">Source</a><label>Projection <select aria-label="Projection"><option value="reflected">Y reflected</option><option value="original">Y unreflected</option></select></label></header>
<main><h2>{status}</h2><p>{note}</p><h2>Reference Maps</h2><div class="refs">{"".join(refs) or "<p>No reference images on this page.</p>"}</div><h2>Graph Templates</h2><p><small>App basis at review: {current}. Terrain features, not final walkable outlines.</small></p><div class="graphs">{"".join(graphs)}</div></main>
<script>document.querySelector('select').addEventListener('change', e => document.body.dataset.mode = e.target.value);</script></body></html>'''
        (out / name).write_text(content)
    images = sum(len(page["images"]) for page in inventory["pages"])
    graphs = {graph["logical_path"] for page in inventory["pages"] for graph in page["graphs"]}
    uncovered = ", ".join(html.escape(area["name"]) for area in inventory.get("uncovered_areas", []))
    failures = len(inventory["errors"]) + sum(len(page["graph_errors"]) for page in inventory["pages"])
    patch_warning = "" if inventory.get("review_patch_matches", True) else "<p>Review patch differs from this inventory. Conclusions need rechecking.</p>"
    (out / "index.html").write_text(f'<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Acts 1-5 reference inventory</title><style>{STYLE}</style><body><header><h1>Acts 1-5 Reference Inventory</h1></header><main><p>{len(inventory["pages"])} pages, {images} images, {len(graphs)} top-level graph candidates, {failures} fetch/parse failures. Images include duplicates and detail shots, not distinct layout counts.</p><p>Manual macro review, patch {html.escape(inventory["patch_version"])}. No exact per-template generated-map validation is claimed.</p>{patch_warning}<div class="inventory-table"><table><thead><tr><th>Act</th><th>Zone</th><th>Images</th><th>Graphs</th><th>Review</th></tr></thead><tbody>{"".join(index_rows)}</tbody></table></div><h2>No linked zone page</h2><p>{uncovered or "None"}</p></main></body></html>')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--api", default="http://127.0.0.1:5176")
    parser.add_argument("--out", default=".poe-layouts/research/acts-1-5-reference-audit")
    parser.add_argument("--inventory", type=Path, help="Regenerate the viewer from saved inventory without fetching")
    parser.add_argument("--review", type=Path, default=Path("docs/acts-1-5-reference-review.json"))
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    out = root / args.out
    out.mkdir(parents=True, exist_ok=True)
    review = json.loads((root / args.review).read_text())
    raw = root / ".poe-layouts/raw/campaign-acts-1-5"
    manifest = json.loads((raw / "manifest.json").read_text())
    if args.inventory:
        inventory = json.loads((root / args.inventory).read_text())
        attach_coverage(inventory, manifest)
        attach_review(inventory, review)
        write_viewer(out, inventory)
        print(f"Regenerated viewer: {out}")
        return
    environments = {entry["logical_path"]: entry["environment"] for entry in json.loads((raw / "layout-environments.json").read_text())["layouts"]}
    topology = {entry["row_index"]: entry for entry in manifest["selected_topologies"]}
    selected = {entry["logical_path"] for entry in manifest["candidate_files"] if entry["kind"] == "graph"}
    inventory = {"patch_version": manifest["patch_version"], "retrieved_at": datetime.datetime.now(datetime.timezone.utc).isoformat(), "pages": [], "errors": []}
    site = "https://www.definitivguide.com"
    categories = ["act-1-1", "act-2-1", "act-3-1", "act-4-1", "act-5"]
    for act, category in enumerate(categories, 1):
        document = Document(fetch(f"{site}/docs/category/{category}").decode("utf-8"))
        links = dict.fromkeys(node.attrs["href"] for node in document.root.walk() if node.tag == "a" and urllib.parse.unquote(node.attrs.get("href", "")).startswith(f"/docs/PoE1/Act {act}/") and not node.attrs["href"].lower().endswith("/overview"))
        for link in links:
            url = urllib.parse.urljoin(site, urllib.parse.quote(urllib.parse.unquote(link), safe="/"))
            try:
                source = fetch(url).decode("utf-8")
                document = Document(source)
                article = next(node for node in document.root.walk() if "markdown" in node.attrs.get("class", "").split())
                title = next(node.text() for node in article.walk() if node.tag == "h1")
                page = {"act": act, "title": title, "url": url, "images": [], "headings": [], "graphs": [], "graph_errors": []}
                cells = {}
                for cell in article.walk():
                    if cell.tag in ["td", "th"]:
                        for image in cell.walk():
                            if image.tag == "img":
                                cells[id(image)] = cell.text()
                for node in article.walk():
                    if node.tag in ["h2", "h3"]:
                        page["headings"].append(node.text().replace("\u200b", ""))
                    elif node.tag == "img":
                        image_url = urllib.parse.urljoin(site, node.attrs["src"])
                        page["images"].append({"url": image_url, "alt": node.attrs.get("alt", ""), "caption": cells.get(id(node), ""), "section": page["headings"][-1] if page["headings"] else ""})
                area = next((a for a in manifest["selected_areas"] if a["act"] == act and (a["id"] == ALIASES.get((act, title)) or canonical(a["name"]) == canonical(title))), None)
                if area:
                    page["zone_id"] = area["id"]
                    paths = sorted({topology[index]["graph_file"].lower() for index in area["topology_indices"] if index in topology and topology[index].get("graph_file") and topology[index]["graph_file"].lower() in selected})
                    for path in paths:
                        query = urllib.parse.urlencode({"path": path, "zone": area["id"]})
                        try:
                            graph = json.loads(fetch(f"{args.api}/api/layout-graph?{query}"))
                            graph["environment"] = environments.get(path, "unknown")
                            page["graphs"].append(graph)
                        except Exception as error:
                            page["graph_errors"].append({"path": path, "error": str(error)})
                inventory["pages"].append(page)
                print(f'Act {act} {title}: {len(page["images"])} images, {len(page["graphs"])} graphs', flush=True)
            except Exception as error:
                inventory["errors"].append({"url": url, "error": str(error)})
                print(f"ERROR {url}: {error}", flush=True)
    attach_coverage(inventory, manifest)
    attach_review(inventory, review)
    (out / "inventory.json").write_text(json.dumps(inventory, indent=2))
    write_viewer(out, inventory)
    print(json.dumps({"pages": len(inventory["pages"]), "images": sum(len(p["images"]) for p in inventory["pages"]), "errors": inventory["errors"], "out": str(out)}, indent=2))


if __name__ == "__main__":
    main()
