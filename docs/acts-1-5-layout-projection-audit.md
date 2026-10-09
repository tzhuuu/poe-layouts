# Acts 1-5 Layout Projection Audit

Reviewed 2026-10-08 against local patch `3.29.3.3` and every zone page linked
from CyclonDefinitiv's Definitiv Guide Act 1-5 categories. This is a manual
macro-layout audit, not an automated image matcher or generated-map validation.

## Coverage

| Act | Guide zone pages | Loaded reference images | Top-level graph candidates |
| --- | ---: | ---: | ---: |
| [1](https://www.definitivguide.com/docs/category/act-1-1) | 15 | 89 | 119 |
| [2](https://www.definitivguide.com/docs/category/act-2-1) | 17 | 59 | 93 |
| [3](https://www.definitivguide.com/docs/category/act-3-1) | 18 | 81 | 93 |
| [4](https://www.definitivguide.com/docs/category/act-4-1) | 13 | 48 | 59 |
| [5](https://www.definitivguide.com/docs/category/act-5) | 10 | 17 | 31 |
| Total | 73 | 294 | 395 |

All 73 pages matched a cached zone. All 395 selected graph candidates loaded
through the Rust graph API without fetch/parse errors. All 294 reference images
loaded in the browser and were visually reviewed alongside those candidates.
Image counts include duplicate outcomes, separate floors/parts, room closeups
and skip examples. They are **not distinct topology counts**. An inventory match
or successful parse does not establish geometric correctness.

The guide has no linked zone pages for Fetid Pool, Den, Dread Thicket or the five
Act 1-5 towns in our manifest. Oriath Square has a linked page but no images.
These cannot be visually confirmed against this source.

The per-zone observations and verdicts are in
[`acts-1-5-reference-review.json`](acts-1-5-reference-review.json). They contain:

- 8 zones with directional evidence favoring Y reflection.
- 3 zones with directional evidence conflicting with the current indoor basis.
- 38 zones with broadly compatible families but no identified per-template match.
- 12 inconclusive comparisons.
- 6 zones blocked by unexpanded nested graphs.
- 5 zones blocked by active-room or disconnected placement/composition.
- 1 zone without a reference image.

None of these categories means all generated layouts for a zone are validated.

## Projection Findings

The reflected view uses `x = (rawX - rawY) / sqrt(2)` and
`y = -(rawX + rawY) / sqrt(2)`. The previous view uses
`x = (rawX + rawY) / sqrt(2)` and `y = (rawY - rawX) / sqrt(2)`.
These compare authored node centers only, not ARM origins or final map bounds.

[Crossroads](https://www.definitivguide.com/docs/PoE1/Act%202/Crossroads)
provides the clearest asymmetric multi-landmark check: Sins NW, Broken Bridge
NE, Fellshrine SE and Old Fields SW agree with reflection. Coast shoreline
handedness also supports the earlier correction. This does not establish
runtime rotation/flip constraints or a bijection between screenshots and files.

The `Indoor`/`Outdoor` tag describes the active RoomSet filename convention;
it does not define a coordinate basis or literal physical setting. In particular:

- [Battlefront](https://www.definitivguide.com/docs/PoE1/Act%203/Battlefront):
  reflected candidates put Marketplace SE and Docks NW. The current indoor
  projection instead puts Marketplace NW and Docks south, contrary to the
  pictured route and the guide's northern Docks directions.
- [Dried Lake](https://www.definitivguide.com/docs/PoE1/Act%204/Dried%20Lake):
  all eight reflected candidates put Highgate NW as in the four references.
  The current indoor basis puts that entrance SE. Interior obstacle and boss
  placement still require generated room geometry.
- [Ascent](https://www.definitivguide.com/docs/PoE1/Act%204/Ascent): the reflected
  northward route toward the NW terminal branch resembles the stitched image;
  the current indoor view reverses the traversal. The guide's northward then
  westward instructions support this coarse reading, not two exact matches.

The stitched images for Ebony Barracks and Cathedral Rooftop appear to favor
SE-to-NW traversal under reflection, but both pages' prose says NE. They are
marked inconclusive, not used as decisive evidence. A stitched image is not a
calibrated full-map coordinate frame. Cathedral also demonstrates why matching
an undirected NW-SE axis alone is insufficient: both projections preserve it
while swapping endpoints. Direction words here describe image positions unless
explicitly tied to the source prose.

This audit does not change the production projection. The current outdoor-only
rule cannot be declared globally correct, and a blanket flip of all indoor
graphs is not proven by these comparisons either.

## Missing Composition

The findings below describe the top-level picker at audit time. The subsequent
reachable-candidate picker resolves cached child graphs into grouped layout
choices (including Upper Prison and Ancient Pyramid), but does not assemble
their parent transforms, floor transitions or terrain geometry. The comparison
gallery and per-zone review remain the original top-level audit snapshot.

Upper Prison, Fellshrine, Ancient Pyramid, Lunaris 1 and both Sceptre zones
exposed wrapper graphs in that snapshot. For example:

- Upper Prison's `prisonmain_1_1.dgr` has two unconnected `graph` nodes referencing
  `Prison` and `Warden`. Its `filegroups.fgp` resolves those to nine prison
  graphs and two boss graphs. One table topology is not one generated floor.
- Fellshrine's three selected reaper-shrine wrappers point directly to
  `macro_ruins.tgr`, `macro_ruins2.tgr` and `macro_ruins3.tgr`.
- Ancient Pyramid's four graph nodes represent floor groups plus an apex graph.
  Its file groups contain 3, 4 and 4 candidates for the first three floors.
- Sceptre's `Level1`, `Level2`, `Level3` aliases resolve through `filegroups.fgp`;
  the guide's floor images are not comparable to the three wrapper anchors.

Other blockers include alternative cave/entrance anchors drawn simultaneously,
teleport-linked room regions and room-local exits currently labeled at one
graph center. Random corridor/tile filling, height layers, ET/GT boundaries and
ARM placement are still needed for walkable outlines. Sparse graphs can resemble
multiple screenshots under different projections, especially complete cardinal
rotation families. No layout should be excluded merely because a screenshot
does not identify it. Cache/reference patch differences remain another possible,
unproven contributor.

## Reproduce

With the rebuilt Rust app server running on port 5176:

```sh
python3 scripts/audit-layout-references.py --api http://127.0.0.1:5176
python3 -B -m unittest discover -s scripts -p 'test_audit_layout_references.py'
target/debug/pather-layouts-server --addr 127.0.0.1:5177 \
  --static-root .poe-layouts/research/acts-1-5-reference-audit
```

The [comparison gallery](http://127.0.0.1:5177/index.html) includes every reviewed
page, its verdict, source link, remote image URLs and both projection choices.
It defaults to the reflected hypothesis and separately names the app basis at
review. It is a research surface, not a second production renderer. Images are
embedded from the source, not downloaded or copied into the repository.

Inventory and HTML are ignored local outputs. Regenerate just the viewer without
network requests:

```sh
python3 scripts/audit-layout-references.py \
  --inventory .poe-layouts/research/acts-1-5-reference-audit/inventory.json
```

The checked-in review is patch-scoped; changed pages/new patches need a new
manual review.
