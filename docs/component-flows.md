# Component Flows

This document tracks how the moving pieces fit together. Keep implementation
details in code; keep cross-component flow shape here.

## Current Components

```text
pather-cli
  -> local CLI runtime glue
     -> cache path policy
     -> temporary Node.js Oodle bridge
  -> poe-content
     -> patch CDN source
     -> disk cache
     -> bundle/index parsers
     -> logical file extraction
  -> poe-dat
     -> datc64 binary reader
     -> GraphQL dat schema interpreter
     -> typed dat table projection
  -> pather-core
     -> campaign Acts 1-5 scrape API
     -> local raw/model artifacts
  -> pather-layouts-server
     -> local HTTP API
     -> layout endpoint orchestration
     -> static app/dist serving
  -> browser app
     -> React app shell
     -> Pixi layout renderer
  -> scripts/ooz-decompress-bundle.mjs
     -> ooz-wasm
```

- `pather-cli` is the debugging and pipeline entrypoint. It owns CLI flags and
  low-level inspection command presentation.
- `pather-layouts-server` owns the local HTTP surface. It serves the built browser app,
  `layouts.bin`, layout JSON endpoints, and already-extracted raw corpus files
  backed by lower-level Rust crates.
- `poe-content` owns stable Rust APIs for patch versions, cache primitives,
  bundle header parsing, index parsing, path hashing, path reps unpacking,
  logical file extraction, and cache verification. It does not decide where an
  application workspace stores its cache.
- `poe-dat` owns generic PoE table readers: `.datc64` decoding, GraphQL DAT
  schema interpretation, typed table projection, and schema snapshot fetching
  through a caller-supplied cache.
- `pather-core` owns milestone-2 layout-domain APIs. It consumes
  `PatchClient`, `GraphqlDatSchema`, typed dat table projections, and a
  caller-provided `BundleDecompressor` to produce raw campaign scrape manifests
  high-level graph JSON, and app-facing FlatBuffers artifacts.
- `scripts/ooz-decompress-bundle.mjs` is the temporary Oodle bridge. It should
  stay thin: read bundle bytes, decode chunks with `ooz-wasm`, write bytes.
- The visualizer remains local-first. Vite is a frontend development and build
  tool only; the production app is served by `pather-layouts-server` on localhost.
- Web API handlers may expose local layout artifacts and extracted raw corpus
  files. They should not expose raw CDN, bundle, `.datc64`, or GraphQL schema
  mechanics directly.
- The browser app should stay focused on browsing and rendering layout artifacts.
  Pipeline, cache, DAT schema, and raw GGPK exploration commands live in
  `pather-cli` until we need a productized local control surface.
- The browser can request a parsed graph for an extracted `.dgr` file through
  `pather-layouts-server`; `pather-core` owns the text/UTF-16 DGR parser and the
  server only handles local file routing.

## `poe-content` Source Layout

```text
bundle.rs             bundle header/decompression primitives
cache.rs              shared disk cache, manifests, verification, clearing
ggpk.rs               low-level GGPK record scanning primitives
index_bundle.rs       Bundles2 index parsing, path reps, path hashing
patchcdn.rs           version resolution and patch CDN fetch orchestration
patch_client.rs       high-level patch index loading and logical file extraction
```

## `poe-dat` Source Layout

```text
dat_schema_client.rs  fetch/update local GraphQL DAT schema snapshots
dat_table.rs          typed table row projection over GraphQL dat rows
datc64.rs             generic .datc64 binary reader
dat_graphql.rs        GraphQL schema interpreter for named dat tables/columns
```

The dependency direction should stay one-way: `dat_graphql` may depend on
`datc64`, and `dat_table` may depend on `dat_graphql`, but `datc64` must not
know about GraphQL or specific tables. Layout, monster, item, or other
domain-specific crates should depend on `GraphqlDatSchema` plus
`TypedDatTableRow` for table reads and keep PoE-domain semantics outside these
generic readers.

`PatchClient` is the higher-level file access boundary for milestone 2. It owns
fetching cached CDN bundles, loading the decompressed index, resolving logical
paths, and extracting logical files. The caller supplies a `BundleDecompressor`
implementation; today the CLI implementation shells out to the temporary
Node.js `ooz-wasm` bridge.

`pather-schema` owns the app-facing FlatBuffers schema and small Rust writer/reader
helpers. Scrape/build code should pass typed model structs into this crate
instead of constructing FlatBuffers directly in CLI code.

## `pather-layouts-server` Source Layout

```text
main.rs  local HTTP API, layout endpoints, static serving, server runtime glue
```

Server code should stay practical and local-environment-shaped. It may know
about the repo workspace, `.poe-layouts`, default schema/output paths, and
process execution. It should call into `pather-core`, `poe-content`, and
`poe-dat` rather than reimplementing their parsing or domain rules.

## `pather-core` Source Layout

```text
lib.rs  Acts 1-5 scrape API, manifest/model conversion, layout DB inspection
```

The core crate is intentionally small right now. As milestone 2 grows, split it
around stable domain seams:

```text
campaign.rs       Acts 1-5 scope selection and scrape orchestration
layout_db.rs      manifest -> FlatBuffers model conversion
terrain.rs        graph/TSI/DGR/ARM dependency parsing
web_api.rs        optional DTO helpers for local web commands
```

The important boundary is that CLI and web handlers should call this crate rather than
duplicating campaign scrape behavior.

The scrape writes `high-level-graph.json` beside `manifest.json`. This graph is
the current presentation and handoff layer before room-level terrain parsing: it
links acts, campaign areas, referenced topology rows, table-declared graph/TSI
files, and the terrain folders used to build the raw corpus.

## Browser App Data Flow

```text
browser app
  -> pather-layouts-server
  -> /data/layouts.bin
  -> generated FlatBuffers TypeScript reader
  -> React/Pixi layout explorer

rendering pipeline or AI helper
  -> .poe-layouts/raw/campaign-acts-1-5/high-level-graph.json
  -> act/area/topology/terrain-file graph
  -> extracted raw corpus under .poe-layouts/raw/campaign-acts-1-5/files

browser or AI helper
  -> /api/raw-files?prefix=metadata/terrain/act1/area1
  -> /raw-files/metadata/terrain/act1/area1/example.dgr
  -> extracted bytes from .poe-layouts/raw/campaign-acts-1-5/files

browser app
  -> /api/layout-graph?path=metadata/terrain/act1/area1/graphs/example.dgr
  -> pather-core DGR parser
  -> node/edge graph JSON rendered in the Layouts tab

browser app
  -> POST /api/layout-rooms { paths: extracted graph candidates for the selected zone }
  -> server reads and parses each distinct DGR/TGR path
  -> pather-core summarize_layout_rooms
  -> room-label list with distinct-layout counts and node indices per layout
  -> searchable room picker, selected ARM variant, and room details workspace
```

The app reads the compiled FlatBuffers artifact directly. Local scrape, cache,
and live CDN operations are intentionally kept in `pather-cli` while the data
model is still moving. The server may browse and serve files that the pipeline
has already extracted into the local raw corpus.

The room list groups exact nonempty graph node labels. These labels constrain
room selection; they are not resolved `.arm` room assets. Each graph path counts
once per label, even when several nodes use that label. The list is scoped to
the selected zone's extracted, table-declared graph candidates. Failed files
and parser warnings are surfaced in the list; the denominator counts graphs
that parsed successfully. Counts describe template presence, not spawn odds.
Room data lives in the Rooms tab beside Layouts and Files. Room search and
selection persist across tab changes. Selecting a room or ARM variant does not
highlight graph nodes or change the selected layout. The Layouts tab gives the
graph the full remaining workspace height. A separate layout picker beside the workspace
tabs opens downward on hover or click. It highlights the current graph,
scrolls long candidate lists, disables unavailable graphs, and closes after
selection. It remains available in the Files tab. In the Rooms tab, the room
selection/search picker replaces the layout picker on the same row. Shared
workspace state preserves both selections when switching tabs.

The Rooms workspace uses that searchable hover/click picker beside the tabs,
with a large left preview and a scrollable right inspector below.
`POST /api/layout-rooms` also calls
`pather-core inspect_layout_rooms` for the layouts' distinct master sources:
graph MasterFile -> TSI RoomSet -> active, deduplicated ARM references -> ARM
asset-table header, dimensions, version, and internal label. The UI joins by
exact internal label and deduplicates variants across room sets. Disabled
room-set lines are excluded; missing files and unsupported headers produce
partial-coverage warnings. Candidates are possibilities, not spawn odds or a
claim that every ARM satisfies all graph tags/rotation constraints.
Reported sizes are the numeric ARM header pair immediately before the label;
they are not inferred world-space bounds or decoded terrain geometry.

`GET /api/room-variants?layout=...` exposes the same catalog for one graph.
`app/src/render/RoomPreview.tsx` is the renderer integration boundary, receiving
the selected `RoomVariant` (including its logical ARM path) and room label.
It currently displays an explicit pending-render state; no fabricated room
geometry is drawn. The inspector shows selectable ARM variants, dimensions,
version, asset-entry count, source paths, distinct-layout counts, and node
occurrences in the displayed and other candidate graphs.

Node colors are presentation heuristics derived from labels and metadata tags:
waypoint, side area, boss, exit, entrance, other named room, structural node,
and trailing DGR `V` (void) flag. Disabled metadata tags do not assign a role.
The graph preserves the raw label, rotation, and metadata in hover titles.
Rooms with boss labels or active boss metadata also get a small linked Boss
marker, independently of their primary color role. The room catalog now also
reads ARM spawn-hook rows with three finite numeric coordinates/rotation and
the exact case-insensitive `mapboss` tag, ignoring asset names and disabled
hooks. The graph API joins these active ARM candidates by internal room label
and returns `node_bosses` with source paths, tags, and candidate counts. Such
nodes display a Map boss satellite even when their graph label is `camp`.
Hover titles describe candidate coverage, not guaranteed runtime spawns or a
resolved monster identity. Missing ARM files produce boss-coverage warnings.
Auto-fit includes the markers; parsed graph node and edge counts are unchanged.

Outdoor graph previews default to a 24-unit grid overlay, toggleable in the
graph toolbar. Classification comes from the selected graph's active RoomSet,
not its suffix. The grid shares the graph's raw-coordinate transform, scale,
origin, and 45-degree counterclockwise rotation. It moves and zooms with the
graph. This is a visual ruler over authored graph coordinates, not decoded
terrain cells: node positions are not assumed to be exact multiples of 24,
snapped, or replaced with fabricated grid nodes. Indoor and unknown layouts
do not show the grid or its toggle.

## Layout Environment Flow

```text
Acts 1-5 scrape (or pather classify-layout-environments for an existing cache)
  -> pather-core layout_environment
  -> graph MasterFile header -> TSI RoomSet field
  -> active room-set filename determines indoor/outdoor
  -> manifest.json layout_environments and layout_environment_warnings
  -> layout-environments.json beside the raw manifest
  -> GET /api/layout-environments
  -> environment badge beside the zone name in the workspace header
```

Classification follows the active TSI room set: `generate.rs` and
`generate_*.rs` mean Indoor; `room_tiles.rs` and `room_nodes.rs` mean Outdoor.
Missing or unrecognized RoomSet fields remain Unknown. The referenced room-set
file need not be extracted to classify its filename. Boundary types, local
`rooms/` inventory, graph suffixes, and area names do not affect the category.
The classifier no longer emits Mixed.

Each classification retains the resolved master/room-set paths, raw outer
ground type, terrain-root file flags, and evidence strings. The classification
index is versioned with the cached patch; the app rejects a mismatched patch.
The zone badge uses its declared TSI classification, falling back to the first
extracted layout's classification when that TSI is unavailable.
This adds a JSON artifact without changing the existing FlatBuffers schema.

## Layout Entrance Flow

```text
Acts 1-5 scrape (or pather scrape-layout-transitions for an existing cache)
  -> WorldAreas Id, Name, ordered Connections_WorldAreasKeys
  -> graph active entranceN metadata tags
  -> connection slot N-1 -> destination WorldAreas row
  -> TSI active RoomSet -> extracted ARM candidates by internal room label
  -> door entity references from placed room objects
  -> manifest layout_transitions + layout_transition_warnings
  -> layout-transitions.json
  -> GET /api/layout-graph?path=...&zone=...
  -> destination labels and complete evidence in graph hover titles
```

Entrance numbering is one-based. The connection array is not sorted or
deduplicated: two entrance slots can lead to the same zone. Only the declared
graph metadata tag block is inspected, and disabled tags do not resolve.
Destination lookup is zone-specific because a layout can be reused by different
WorldAreas rows. Missing slots remain Unresolved rather than becoming doors.

Door evidence is a placed entity path named Door, ending in Door, or starting
with Door_; decorative `.ao` filenames are not used to classify it. Active RS
entries are joined to ARM internal labels, not filenames. Disabled RS entries
are ignored. Since a room label may select several ARM variants, these are
possible door candidates, not a claim that every generated room has that door.
Missing shared-room dependencies produce partial-coverage warnings. The scan
does not yet follow object inheritance or scripts to identify every door type.

Ancient Pyramid graphs under `act2/area14level3` retain numbered entrance tags
but mark their routing Deferred. Their internal storey/subgraph numbering needs
separate handling before joining those tags to external zone connections.
The server rejects entrance indexes from a different dataset patch. Raw room
labels, metadata, and transition evidence remain available in hover titles.

The Acts 1-5 scrape treats table-declared `WorldAreas.TSIFile` and
`Topologies.DGRFile` values as first-order terrain candidates only when the
normalized logical path exists in the patch index. It does not invent sibling
`.arm` or `.dgr` paths; those should come from parsing graph/terrain metadata
files in a later pass.

Map-format reverse-engineering notes, including current terrain-root
indoor/outdoor heuristics, live in `docs/map-format-notes.md`.

## Milestone 1 Outcome: Parser Input Foundation

Milestone 1 is achieved. The repo now has a reusable Rust foundation for getting
current PoE1 parser inputs onto disk and reading the first table files needed by
the Acts 1-5 scraper.

Delivered outcomes:

- Latest PoE1 patch resolution through the version endpoint.
- Release-line-aware disk cache under `.poe-layouts/cache`.
- Cache clearing, prefetch manifests, and offline verification.
- Patch CDN bundle index fetching and header parsing.
- Oodle bundle decompression via the temporary Node.js `ooz-wasm` bridge.
- Decompressed index parsing, path hash lookup, and path reps unpacking.
- Logical file extraction by path, including `data/worldareas.datc64` and
  `data/topologies.datc64`.
- GraphQL dat schema fetching from `poe-tool-dev/dat-schema`, validation, and a
  local DAT schema snapshot.
- Two-layer table reading:
  - `datc64` is the generic PoE file reader for `.datc64` envelopes, primitive
    field layouts, UTF-16 strings, row keys, arrays, and projected rows.
  - `dat_graphql` is the schema-specific layer for GraphQL table definitions,
    effective column names, column layouts, and named projections.
- Typed table projection through `dat_table`, where domain crates define small
  `TypedDatTableRow` adapters and reuse shared row access/type validation.
- CLI inspection commands for the above flows.
- Rust tests and live-current table validation for the initial layout-critical
  tables.

The milestone intentionally does not include campaign filtering, topology
dependency expansion, or terrain parsing. Those belong to the next milestone:
the Acts 1-5 scrape scope.

## Table Schema Flow

```text
update-dat-schema
  -> DatSchemaClient
  -> shared disk cache
  -> poe-tool-dev/dat-schema _Core.gql
  -> validate required table types exist
  -> data/cache/dat-schema/_Core.gql
  -> data/cache/dat-schema/schema-manifest.json
```

The GraphQL schema snapshot is the contract for interpreting extracted table
files. The current scraper milestone requires at least `WorldAreas` and
`Topologies`; future table parsing should use the local snapshot by
default and refresh it explicitly when we need upstream schema changes. Schema
refreshes force a network fetch by default; `--offline` rebuilds the local
snapshot from cached bytes.

## Version And Cache Flow

```text
latest-versions
  -> https://poe-versions.obsoleet.org
  -> PatchCdnSource
  -> release-line cache namespace
```

Default CLI behavior queries the latest PoE1 patch version. Explicit
`--patch-version` makes commands reproducible.

Cache keys are grouped by release line and exact patch:

```text
.poe-layouts/cache/
  poe1/<release-line>/patches/<patch-version>/Bundles2/<bundle>
```

The release-line grouping lets us clear or inspect a whole game line while the
exact patch path keeps bytes reproducible.

## Index Inspection Flow

```text
inspect-index
  -> fetch/cache Bundles2/_.index.bin
  -> ooz bridge decompresses _.index.bin
  -> poe-content parses index sections
  -> ooz bridge decompresses nested path reps bundle
  -> poe-content unpacks logical paths
  -> CLI prints summary, root dirs, prefix samples, optional lookup
```

Rust owns the structure and validation around the index. The bridge only decodes
bundle chunks.

## Logical File Extraction Flow

```text
extract-file --logical-path data/worldareas.datc64
  -> PatchClient loads/decompresses/parses _.index.bin
  -> PatchClient decompresses path reps
  -> hash logical path with Murmur64A
  -> resolve bundle name, offset, size
  -> fetch/cache containing bundle
  -> ooz bridge decompresses requested slice
  -> write output file
```

Logical paths from path reps are lowercase. Callers should prefer lowercase
paths when listing or extracting files.

## Logical Folder Browsing Flow

```text
browse-files --prefix Metadata/Terrain/Act1/Area1
  -> PatchClient loads index and logical paths
  -> match descendants under the folder prefix
  -> print immediate child folders plus matching files

browse-files --prefix Metadata/Terrain/Act1/Area1 --recursive --extension dgr
  -> print every matching .dgr descendant, capped by --limit
```

`browse-files` is the CLI affordance for walking the GGPK logical namespace. It
uses path reps from `_.index.bin`; it does not extract file bytes. Prefix
matching is case-insensitive so callers can use either the original GGPK casing
or lowercased paths.

## Logical Folder Extraction Flow

```text
extract-folder --prefix Metadata/Terrain/Act1/Area1
  -> PatchClient loads index and logical paths
  -> match every descendant under the folder prefix
  -> resolve each logical file to bundle, offset, and size
  -> fetch/cache containing bundles
  -> group requested slices by containing bundle
  -> ooz bridge decompresses each bundle batch and writes all requested slices
  -> write files under .poe-layouts/raw/files preserving logical paths
  -> write .poe-layouts/raw/extract-folder-manifest.json
```

This command is the general raw-corpus escape hatch for AI-assisted reverse
engineering: extract a whole content folder, keep the logical hierarchy, and use
the manifest as the durable inventory tying local files back to CDN bundles.

The Acts 1-5 scrape now performs the same kind of folder extraction for each
table-declared terrain candidate's broader terrain area folder. For example, a
`Topologies.DGRFile` anywhere under `metadata/terrain/act1/area1/...` causes
the scrape to extract every indexed file under
`metadata/terrain/act1/area1` into
`.poe-layouts/raw/campaign-acts-1-5/files/metadata/terrain/act1/area1`.
The scrape can also include explicit raw folder roots via CLI flags when we
want to inspect a folder that no selected table-declared file points into.

## Raw Corpus HTTP Flow

```text
GET /api/raw-files?prefix=metadata/terrain/act1/area1
  -> list immediate child folders and files from the extracted raw corpus

GET /api/raw-files?prefix=metadata/terrain/act1/area1&recursive=true&extension=dgr
  -> list matching extracted descendants

GET /raw-files/metadata/terrain/act1/area1/example.dgr
  -> stream the extracted file bytes
```

This surface is intentionally filesystem-backed. It does not fetch from the
patch CDN or extract missing files on demand; rerun the scrape or use
`extract-folder` when the local raw corpus needs more bytes.

Cold scrape performance depends mostly on Oodle decoding. `PatchClient` batches
folder-corpus extraction by containing bundle so the temporary Node bridge reads
and decodes each bundle once per batch instead of once per logical file. Reruns
also skip files whose existing output size already matches the index entry.

## Dat Table Reader Flow

```text
inspect-dat-table
  -> read data/cache/dat-schema/_Core.gql
  -> GraphqlDatSchema parses schema tables once
  -> dat_graphql maps GraphQL columns to generic datc64 columns
  -> parse local .datc64 fixed/variable sections
  -> datc64 decodes primitive values and projected rows
  -> generate stable names for anonymous schema fields
  -> project requested columns or all effective columns
  -> print rows as JSON
```

The reader currently targets `.datc64`, which is enough for `WorldAreas` and
`Topologies`. Generic binary reading is isolated in `datc64`; GraphQL schema
interpretation is isolated in `dat_graphql`. The library boundary for
table-aware scrape/build code is `GraphqlDatSchema`: construct it once from the
local GraphQL schema, then reuse it across raw table files after extracting
them from the patch CDN.

Typed domain reads use the same lower layers:

```text
read_typed_graphql_table::<WorldAreaRow>
  -> WorldAreaRow::TABLE_NAME / COLUMNS select a stable projection
  -> GraphqlDatSchema reads the projected datc64 rows
  -> DatRowView validates required column value types
  -> WorldAreaRow::from_dat_row maps raw cells into layout-domain fields
```

The generic API lives in `poe-dat` so later scrapers can define their own typed
rows without duplicating value lookup, row index handling, or type mismatch
errors. `pather-core` currently defines `WorldAreaRow` and `TopologyRow`
as private adapters because those table meanings belong to the Acts 1-5 scrape
flow, not the generic file reader.

## Offline Flow

```text
prefetch-bundles
  -> download required bundle files
  -> write manifest with size and blake3

verify-cache
  -> read manifest
  -> check local files without network
```

Future scrape/build commands should verify required inputs exist locally before
parsing when run in offline mode.

## Next Milestone: Acts 1-5 Scrape Scope

```text
scrape campaign-acts-1-5
  -> PatchClient loads index and logical paths
  -> extract data/worldareas.datc64
  -> extract data/topologies.datc64
  -> GraphqlDatSchema reads WorldAreas and Topologies
  -> select main Acts 1-5 campaign zones by WorldAreas id prefix and act
  -> resolve table-declared topology graph and TSI candidates
  -> exclude league-specific terrain under metadata/terrain/leagues
  -> extract candidates that are present and available in cache/network mode
  -> extract each candidate's parent terrain folder into the raw corpus
  -> write raw cache and manifest with missing paths/warnings
  -> convert scrape summary into poe_layouts.fbs model structs
  -> write app/public/data/layouts.bin
```

The goal of this milestone is not full layout graph modeling yet. It produces a
repeatable raw corpus, JSON manifest, and first app-facing FlatBuffers artifact
that later parser/model milestones can enrich.

Current first pass: `scrape-campaign-acts-1-5` writes
`.poe-layouts/raw/campaign-acts-1-5/manifest.json` and
`app/public/data/layouts.bin`. It selects main campaign areas from `WorldAreas`
and records table-declared terrain candidates from `WorldAreas.TSIFile` and
`Topologies.DGRFile`, marking missing files rather than failing the scrape. It
also extracts the parent terrain folders for those candidates and records the
resulting files separately as `folder_files` in the manifest.

`inspect-layout-db` reads the FlatBuffers artifact back and prints counts plus a
small zone sample. Use it as the quick self-validation step after scraping.

The browser app consumes the generated artifact through `pather-layouts-server`;
raw scrape/build iteration remains a CLI concern for now, while browsing the
already-extracted raw corpus is available through local HTTP.

## Refactor Notes

The next useful cleanup is to split `pather-core/src/lib.rs` into domain modules
once terrain dependency parsing grows. The shape should remain:

```text
PatchClient
  -> fetch/load/extract logical files

GraphqlDatSchema
  -> read named dat tables

CampaignLayoutScraper
  -> read WorldAreas/Topologies
  -> select campaign scope
  -> produce raw corpus manifest
```

That future scraper layer should expose methods like `scrape_campaign_acts_1_5`
and typed records for `WorldArea`, `Topology`, and `TerrainCandidate`. It should
depend on `PatchClient` and `GraphqlDatSchema`, while those milestone-1 layers
stay generic.
