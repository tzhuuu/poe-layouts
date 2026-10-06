# Map Format Notes

Working notes for campaign terrain and map-format reverse engineering. Keep
these observations evidence-backed and revise them as parser support grows.

## Terrain Root Shape

Validated against the local Acts 1-5 raw corpus at
`.poe-layouts/raw/campaign-acts-1-5` from PoE1 patch `3.29.3.3`.

The terrain `.rs` files in the raw corpus are data files, not Rust source. The
sampled files are UTF-16 text with `version 2` headers and quoted `.arm` room
paths. They appear to act as room-selection lists:

- `generate.rs` usually lists weighted `.arm` candidates from broader shared
  indoor tilesets, such as `Metadata/Terrain/Cave/Rooms/...` or
  `Metadata/Terrain/Dungeon/Rooms/...`.
- `room_tiles.rs` and `room_nodes.rs` usually list local `.arm` room candidates
  under the same terrain root's `Rooms` folder.

The active TSI `RoomSet`, rather than folder contents, determines the displayed
environment: `generate.rs` / `generate_*.rs` are Indoor, and `room_tiles.rs` /
`room_nodes.rs` are Outdoor. Roots can contain both families without being
classified as Mixed.

Corpus counts for terrain-like roots containing `master.tsi`, `generate.rs`,
`room_tiles.rs`, `room_nodes.rs`, or a direct `rooms/` folder:

| Shape | Count | Notes |
| --- | ---: | --- |
| `generate.rs` and no `rooms/` | 41 | Mostly cave, prison, dungeon, temple, mine, tower, and other enclosed roots. |
| `rooms/` and no `generate.rs` | 19 | Mostly early open outdoor roots such as Act 1 beaches/forest and Act 2 outdoor areas. |
| Both `generate.rs` and `rooms/` | 21 | Both inventories are present, common in Act 5, towns, and subgraphs. |
| Neither, but has `room_nodes.rs` | 2 | `act3/area9` and `act4/area1`; keep as local room-list roots for now. |

Representative roots:

- Indoor/enclosed-style `generate.rs` roots:
  `act1/area11level1`, `act1/area7level1`, `act2/area14level3`,
  `act3/area18level1`, `act4/area6level3`.
- Outdoor/local-list `rooms/` roots without `generate.rs`:
  `act1/area1`, `act1/area2`, `act1/area3`, `act2/area1`,
  `act2/area4`, `act3/area1`.
- Roots containing both inventories:
  `act1/town`, `act2/area13`, `act2/area15/subgraph`, `act3/area2`,
  `act4/area7`, `act5/area1` through `act5/area8`, and `act5/town`.

The classifier retains inventory metadata for inspection, without using it to
override the active RoomSet classification. Fields include
direct file flags (`has_generate_rs`, `has_rooms_dir`, `has_room_tiles_rs`,
`has_room_nodes_rs`), referenced room path families, and whether graph/TSI
dependencies point into shared terrain roots or local `Rooms` folders.

## `.tgr` Graph Files

Validated against 178 local `.tgr` files in the same Acts 1-5 raw corpus.
These files are UTF-16LE text. The local corpus has 111 `version 19` files,
one `version 21` file, and 66 `version 25` files; all matched the same
first-pass header/node/edge boundary shape.

`.tgr` files mostly live under `graphs/` folders for outdoor/local-list terrain
roots. The corpus distribution is concentrated in roots such as
`act1/area9`, `act1/area8`, `act1/area5`, `act1/area2a`, `act2/area8`, and
`act2/area7`. This contrasts with many indoor/enclosed roots, where `.dgr`
files are more common.

Observed high-level structure:

```text
version 19
Size: <width> <height>
MasterFile: "<terrain root>/master.tsi"
Nodes: <node-count>
Edges: <edge-count>
"<ground type slot 0>"
"<ground type slot 1>"
"<ground type slot 2>"
<node rows...>
<edge rows...>
```

The three quoted rows after the `Edges` header appear to be ground-type slots.
They may be empty strings. After those rows, there are exactly `Nodes` node
rows followed by exactly `Edges` edge rows.

Node rows look like:

```text
<x> <y> <link-count> <linked-node-index...> "<label>" <rotation> <metadata...>
```

Useful observed node fields:

- `x`, `y`: graph-space coordinates. They can be much larger than the `Size`
  values, so `Size` is probably tile/grid extent rather than the coordinate
  scale used by node rows.
- `link-count` and following integers: adjacency list pointing at other node
  row indices.
- `label`: empty for ordinary connector nodes, named for authored features
  such as `townentrance`, `washedup`, `tutorial`, `crossroad`, `waypoint`,
  `bridge`, `sidearea`, or `portal`.
- `rotation`: values include `(any)`, `I`, `FI`, `R90`, `R180`, `R270`, and
  combinations such as `FR270` in `.dgr`; likely orientation/flip constraints
  for fitting rooms or authored features.
- Remaining metadata commonly includes counts and tags such as `default`,
  `entrance1`, `AutoWaypoint`, `sidearea1`, and `unique1`; preserve these as
  tokens until their positions are known.

Edge rows look like:

```text
<from-node-index> <to-node-index> <flags...> "<edge-tile.et>" <metadata...>
```

The quoted `.et` path is the strongest known field. It points at an edge-tile
definition such as shoreline, road, river, cliff, wall, or ravine joins. The
first two integers align with node row indices and describe graph edges. The
remaining numeric fields look like weights, direction/flip flags, and width or
range parameters, but the exact meanings are still tentative.

Comparison with `.dgr`: `.dgr` files share the same broad header and row idea,
but include a `Default%:` row before nodes and carry additional node/edge
tokens, including trailing `N`/`V` node flags and `P`/`I` plus `N`/`V` edge
flags. Treat `.tgr` as the simpler terrain graph template form for now, not as
an entirely unrelated format.

Parser implication: a first parser can safely extract header fields, ground
type slots, nodes with coordinates/links/labels/rotations, and edges with
endpoints plus `.et` paths. Do not hard-code the trailing numeric metadata yet;
store it verbatim until `.et`, `.tsi`, and room placement parsing explains it.

The current working model is that `master.tsi` selects the room universe through
`RoomSet`, and the `.tgr`/`.dgr` node labels constrain that room universe rather
than listing rooms directly. In `act1/area1`, `room_tiles.rs` lists `.arm`
files whose internal room labels include `townentrance`, `washedup`, `wall`,
`connection`, `nests`, `coast`, `streams`, and `tutorial`; the local `.tgr`
variants use subsets of those labels such as `townentrance`, `washedup`,
`tutorial`, and `nests`.

Empty graph labels (`""`) probably do not mean "choose any room." In sampled
`.tgr` files, empty-label nodes usually have `(any)` rotation and no explicit
room-set tags after the label, while named nodes often carry tags like
`default`, `entrance1`, `AutoWaypoint`, `sidearea1`, or `unique1`. Treat empty
nodes as structural graph vertices for now: they define coordinates,
connectivity, edge-tile joins, and fill/terrain shape, but may not correspond to
an authored `.arm` room unless later metadata proves otherwise.

## Initial Environment Classifier

The scraper now resolves each DGR/TGR `MasterFile` and parses the TSI's active
`RoomSet` and `OuterGroundType`. The local corpus has 98 TSI files; all expose
`RoomSet`, but no direct indoor/outdoor flag was found among their fields.

The classifier maps the active `generate.rs` / `generate_*.rs` filename to
Indoor, and `room_tiles.rs` / `room_nodes.rs` to Outdoor. Missing or unrecognized
RoomSet fields remain Unknown. Neither `OuterGroundType` nor local room
directories influence this rule, and classification does not require the
referenced `.rs` file to have been extracted.

These labels describe the chosen generation family. Raw paths, ground types,
and inventory flags are retained for inspection. The standalone backfill reads
only graph and TSI paths recorded in the existing scrape manifest.

## Entrance Slots

`WorldAreas.Connections_WorldAreasKeys` is ordered. The graph's active
`entranceN` tag maps to entry `N-1`, preserving duplicate destination rows.
For example, The Coast's list is Lioneye's Watch, The Mud Flats, The Tidal
Island; its `townentrance` node carries `entrance1` and `toisland` carries both
`entrance2` and `entrance3`. Flooded Depths contains two slots pointing to The
Submerged Passage. Do not infer destination order from the node label or from
decorative LevelTransitions asset names.

ARM files expose an internal label after their asset table and room dimensions.
Placed-object rows carry an art path followed by an entity metadata path. Door
candidates are identified from the entity path, not from door-shaped art.
Missing/unmapped entrance slots are not proof of a door. Ancient Pyramid
storey graphs require a separate routing model and remain deferred.

## ARM Room Plans

Validated the room-plan parser against all 1,658 cached Acts 1-5 ARM files.
The ARM asset dictionary uses one-based edge/ground references; zero is an
unspecified/default reference. The numeric pair before the room label is not
necessarily the root tile footprint. `k` root keys carry the actual grid size;
single-token roots use the earlier base-size pair instead.

Tile-key rows contain one entry per grid cell. `k` has 23 integer fields through
version 18 and 24 afterwards; `s`, `n`, and `o` have no payload, and `f` has one
feature reference. Preserve key spans and origins: large keys may be anchored
near a boundary, so grid coordinates alone do not justify extending rectangles
from those anchors. The preview colors the recorded key cells rather than
pretending to assemble resolved ground meshes.

ARM point sections change with version: old files use count-prefixed lists;
version 32+ uses `-1` terminators. Versions 35/36 have an additional quoted row
before the grid. Placed objects follow the grid, with coordinates and separate
art/entity paths. Spawn hooks retain their tags, including `mapboss`. Later
decal and auxiliary layers are deliberately not rendered.

The versioned section and tile-key model was cross-checked against
[poeformats' ARM reader](https://github.com/annalithic/poeformats/blob/master/Arm.cs)
and verified on the local corpus. The preview is a schematic source inspection,
not collision geometry or a reconstruction of the game's rendered minimap.

## Open Questions

- Are `generate.rs`, `room_tiles.rs`, and `room_nodes.rs` the same line format
  with different generation roles, or do their numeric weights and disabled
  lines have file-specific meaning?
- Do `.dgr` or `.tsi` files explicitly identify the generator mode, making the
  folder-shape heuristic redundant once parsed?
- Are `.tgr` `Size` values tile/grid extents, and what scale maps them to node
  row coordinates?
- Are `.tgr` node labels direct room/node ids, spawn hooks, or semantic
  topology constraints?
- Which `.tgr` edge metadata fields control edge-tile orientation, width,
  probability, and optionality?
- Should towns be classified separately from indoor/outdoor, since several town
  roots combine `generate.rs`, `rooms/`, and local `master.tsi` data?
- Are the Act 5 mixed roots genuinely hybrid, or did the scrape's terrain-root
  grouping collapse multiple generation modes into one folder-level view?
