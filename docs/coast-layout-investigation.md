# Coast Layout Comparison

Investigation on 2026-10-07 against the local PoE1 `3.29.3.3` cache and
[CyclonDefinitiv's Coast guide](https://www.definitivguide.com/docs/PoE1/Act%201/Coast).
The guide shows in-game map outlines, whereas this app currently renders authored
graph templates. Similarity is expected at the macro-landmark level, not at the
final terrain-outline level.

## Display Handedness

The original preview rotated raw `(x, y)` by -45 degrees in SVG's Y-down space:

```text
screenX = (x + y) / sqrt(2)
screenY = (y - x) / sqrt(2)
```

In `macro_terraces1_1_1.tgr`, inland terraces lie on the larger-Y side of the
shoreline. This transform puts them southeast of the shore; the guide's Simple
North-East screenshot shows them northwest, with sea to the southeast.

The outdoor preview now reflects raw Y before the -45-degree rotation:

```text
screenX = (x - y) / sqrt(2)
screenY = -(x + y) / sqrt(2)
```

Translation, fitting, and the viewer's common scale are omitted above. The grid
and manual-rotation markers use the same basis. Source coordinates and source
room rotation constraints are not changed or automatically applied.

A second check is `macro_terraces1_4_2.tgr`: townentrance is `(1140, 1342)` and
toisland is `(100, 109)`. The old projection makes the entrance-to-exit delta
predominantly west; the corrected projection makes it predominantly south,
consistent with the guide's South Variant. These comparisons support a display
correction, not a complete reconstruction of the game's camera or generator.
Indoor/unknown graph previews and standalone ARM plans remain unchanged pending
their own reference validation. Camera aspect compression is also uncalibrated.

## Graph Connectivity

Node `links` contain incident edge row indices, not neighboring node indices.
For example, Coast node 3 links to edges `[24, 25, 13]` in a graph of 23 nodes.
All nine Coast TGRs already parsed consistently, and the renderer already used
the explicit edge endpoints, so this interpretation correction does not change
Coast connectivity.

The broader audit found 64 DGR files with an extra standalone `0` after
`Default%:`. Treating that as node 0 shifted the real node IDs, consumed the last
node as edge 0, and dropped the final edge. The parser now skips the observed
empty preamble and rejects unknown nonzero preamble counts. After the fix, all
178 TGRs and 362 DGRs in the cached directory parsed without node/edge warnings,
and every node's incident edge IDs matched the explicit edge endpoints.

Repeat the source-level audit without a running server:

```sh
cargo test -p pather-core validates_local_graph_corpus -- --ignored --nocapture
```

The cache-backed test is ignored in normal CI because the raw corpus is local.
These counts include every cached graph, not only table-selected candidates in
the latest manifest. Regression fixtures cover both older DGR headers and the
empty-preamble case, including a real node with X = 0.

After changing parser indices, rebuild the CLI and refresh cached transition
indices before using the rebuilt server:

```sh
cargo run -p pather-cli -- scrape-layout-transitions
```

## Missing Terrain Assembly

Coast edge assets include `beach_shoreline.et`, `beach_large_cliff.et`,
`beach_small_cliff.et`, and `beach_sand_dune.et`. They describe terrain features
and boundaries, not uniformly walkable routes. Rendering every edge as an
identical straight line loses that distinction. The current viewer does not
assemble their tiles, ground-type regions, room footprints, height layers, or
the final walkable outline.

`master.tsi` also references the RoomSet, fill tiles, tileset, and other terrain
assets. Graph trailing numeric metadata and generator behavior are not fully
decoded. Source room rotations are retained as constraints; the global-canvas
rotation buttons are only manual experiments, not an implementation of room
placement.

## Multiple Exits Inside One Room

Coast's `Rooms/toisland.arm` is 21 by 12 tiles and has room-local markers:

| Tag | Local Position | Resolved Zone |
| --- | --- | --- |
| `entrance2` | `(57, 148)` | The Mud Flats |
| `entrance3` | `(257, 69)` | The Tidal Island |

The ARM also contains a waypoint asset reference. In the first Coast graph,
both exit tags attach to node 5, `toisland`, at `(1461, 103)` with source `R270`.
We currently label both destinations at that node's anchor. The guide shows
separate transition positions in the generated room. Matching those requires
placing the ARM at the graph node with the correct room-local anchor, rotation,
and flip semantics, then projecting each marker. Those placement semantics are
not established by this investigation.

## Candidate Selection Is Still Unresolved

WorldAreas row 6 (`1_1_2`) references 18 topology rows: nine regular Coast
templates (4-12) and nine Deepwater league counterparts (2629-2637). The scraper
excludes league terrain, leaving the nine normal candidates shown in the app.
The guide presents eight visual examples, but that is not evidence that the
ninth raw topology is unused or that examples map one-to-one to file names.

All nine normal Topologies rows have the same unnamed column values:
`105`, `100`, `100`, `[0]`, and `60`. No obvious zero/inactive discriminator was
found; these unnamed fields must not be described as spawn weights yet.
[The author's announcement](https://www.pathofexile.com/forum/view-thread/3869230)
describes collecting the original screenshots for 3.27. Our cache is 3.29.3.3;
version differences are possible, but not proven to explain the extra candidate.

## Next Useful Work

1. Distinguish terrain-edge kinds visually so cliffs are not mistaken for paths.
2. Establish ARM anchors and room-local rotation/flip semantics using Coast's
   townentrance and toisland rooms; render their actual footprint and markers.
3. Decode ET/GT composition and height metadata before claiming generated terrain.
4. Confirm topology eligibility independently of the guide's example count.
