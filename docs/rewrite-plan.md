# Rewrite Plan: FlatBuffers-First Layout Visualizer

This document describes the planned rewrite for a local-first Path of Exile 1
layout visualizer. The current implementation is a useful prototype, but the
rewrite should make the data pipeline reproducible, schema-driven, and easy to
iterate on while we reverse-engineer layout generation.

## Current Scope

The active target is campaign Acts 1-5 only.

Maps/Atlas data can remain a future extension, but implementation decisions
should be evaluated against Acts 1-5 first. The app should make it easy to
inspect all known layout possibilities for those zones, understand where every
node/edge came from, and quickly update the visualization as parser knowledge
improves.

## Design Goals

- Use Rust for GGPK reading, extraction, parsing, validation, and artifact
  generation.
- Use a single canonical schema shared by Rust and TypeScript.
- Use FlatBuffers as the primary app data artifact.
- Keep the Tauri app local-first with no required network calls.
- Use Pixi.js for 2D layout visualization.
- Preserve enough raw provenance to explain every rendered node, edge, room, and
  warning.
- Keep iteration fast: extract, parse, generate, render, inspect, repeat.

## Proposed Library Set

This section is intentionally explicit so the implementation stack can be
audited before the rewrite starts. Exact patch versions should be pinned in
`Cargo.lock` and `app/package-lock.json` during implementation. The choices
below describe the intended libraries and why they belong in the design.

### Rust Workspace Libraries

| Area | Library | Intended Use | Notes |
| --- | --- | --- | --- |
| CLI | `clap` | Derive-based command parsing for `discover`, `scrape`, `build-layout-db`, `validate`, and debug commands. | Prefer one typed `Args`/`Subcommand` tree over manual flag parsing. |
| Errors | `thiserror` | Domain error enums in library crates. | Good for auditable error categories like `GgpkError`, `ParseError`, `PipelineError`. |
| Errors | `anyhow` | Top-level CLI error plumbing only. | Avoid leaking `anyhow::Error` from reusable crates. |
| Logging | `tracing`, `tracing-subscriber` | Structured logs for extraction, parsing, validation, and app artifact generation. | Use spans for zone/topology/graph processing. |
| Serialization | `serde`, `serde_json` | Debug reports and fixture manifests. | FlatBuffers is canonical; JSON is for inspection only. |
| FlatBuffers | `flatbuffers` | Rust runtime for writing and optionally reading `layouts.bin`. | Generated Rust bindings live in `poe-schema`. |
| FlatBuffers generation | `flatc` binary invoked from `crates/poe-schema/build.rs` | Generate Rust and TypeScript bindings from `schema/poe_layouts.fbs`. | `build.rs` is the canonical generation path. Prefer checking generated TypeScript into `app/src/generated` only if build ergonomics require it. |
| Hashing | `blake3`, only if/where needed | Simple stable content hashes for manifests, cache checks, and final artifact identity. | Do not build an elaborate hash abstraction. Path + size + one stable content hash is enough when a hash is useful. |
| Binary parsing | `byteorder` | Little-endian integer reads for GGPK records and binary table/index data. | Keep GGPK parsing explicit rather than macro-heavy. |
| Parsing helpers | `winnow` | Text-ish parser combinators for `.dgr`, `.tsi`, `.arm`, `.et` when line splitting gets brittle. | Use only where it improves clarity; simple line scanners are fine. |
| Pattern matching | `globset` | Efficient include/exclude matching for scraper scopes. | Useful for `Metadata/Terrain/...` dependency expansion. |
| Walking dirs | `walkdir` | Traversing extracted raw cache and fixtures. | Avoid custom recursive traversal. |
| Time | `time` | `generated_at` timestamps and report metadata. | Prefer one time library over ad hoc formatting. |
| Temp files | `tempfile` | Atomic artifact writes and tests. | Write `layouts.bin.tmp` then rename. |
| Tests | `insta` | Snapshot tests for parser outputs and generated summaries. | Useful for reverse-engineering formats. |
| Tests | `pretty_assertions` | Easier diff output in parser/model tests. | Dev dependency only. |

### Rust Libraries To Avoid Initially

| Library/Approach | Reason to Avoid for First Rewrite |
| --- | --- |
| `memmap2` | Mapping a multi-GB GGPK may be useful later, but `File + Read + Seek` is easier to audit first. Add mmap only if profiling proves it matters. |
| SQLite as canonical storage | Useful for ad hoc analysis, but FlatBuffers should be the app contract. Add SQLite later as a derived debug index if needed. |
| A web server/backend inside Tauri | The app should load a local generated artifact directly. Rust commands generate data; frontend renders it. |
| Three.js | The visualizer is a 2D topology explorer. Pixi is enough for the first complete rewrite. |

### TypeScript/Tauri Libraries

| Area | Library | Intended Use | Notes |
| --- | --- | --- | --- |
| App shell | Tauri 2 | Native desktop shell and local file permissions. | Keep Rust backend minimal at runtime; pipeline runs as CLI. |
| Build | Vite | Fast frontend dev server and production build. | Already in the prototype. |
| Language | TypeScript | UI/query/render code. | Strict types around generated FlatBuffers accessors and UI view models. |
| Rendering | Pixi.js 8 | 2D graph rendering, hit testing, pan/zoom transforms, hover states. | Keep all graph rendering in one renderer module. |
| FlatBuffers runtime | `flatbuffers` npm package | Read generated `layouts.bin` from TypeScript. | Generated TypeScript bindings should depend on this runtime. |
| UI framework | `react`, `react-dom` | App-level shell, panels, search, lists, controls, and detail views. | Pixi remains responsible for the graph canvas and direct graph interaction. |
| Icons | `lucide-react` where useful for React chrome; Pixi-rendered glyphs for canvas content | Toolbar icons and non-canvas app controls can use React icons. | Node/edge/world icons inside the visualization should be rendered by Pixi. |
| Browser validation | `playwright-core` | Screenshot and layout validation against Vite. | Already in the prototype. |

### Generated Code Policy

- `schema/poe_layouts.fbs` is the canonical schema.
- Rust generated code is produced by `crates/poe-schema/build.rs`.
- TypeScript generated code is also produced through the `build.rs` generation
  path and written under `app/src/generated`.
- Generated files may be committed if that makes local setup easier, but the
  generator command must be reproducible and documented.
- Manual wrappers around generated bindings should live outside generated
  folders:

```text
crates/poe-schema/src/lib.rs
app/src/data/layoutDatabase.ts
```

Generated code should not contain application logic. Keep query helpers and
rendering adapters handwritten.

### Dependency Audit Policy

- Add dependencies only in the crate or package that uses them.
- Avoid workspace-wide dependency sprawl until multiple crates genuinely need
  the same library.
- Prefer small, focused crates over broad frameworks.
- Keep `unsafe_code = "forbid"` for workspace crates. Dependencies may contain
  unsafe internally, but our crates should not.
- Every new runtime dependency should have a short justification in the relevant
  crate README or this plan.
- Keep exact versions pinned by lockfiles.
- Run dependency review before implementation milestones that introduce new
  libraries:

```sh
cargo tree
npm ls --depth=0
```

- For generated code, audit the generator and the generated output separately:
  - `flatc` version used
  - `crates/poe-schema/build.rs`
  - generated Rust bindings location
  - generated TypeScript bindings location
  - runtime package versions
- Do not add network/runtime service dependencies to the app. The app should
  open a local artifact and render it.

## High-Level Pipeline

```text
PoE install / Content.ggpk
  -> Rust GGPK reader
  -> campaign Acts 1-5 scraper
  -> low-level file parsers
  -> semantic layout model builder
  -> FlatBuffers artifact
  -> Tauri + React + Pixi visualizer
```

The pipeline output should eventually be a single binary artifact:

```text
app/public/data/layouts.bin
```

During reverse-engineering, we should also keep debug exports:

```text
.poe-layouts/reports/.../*.json
.poe-layouts/reports/.../*.md
.poe-layouts/reports/.../*.html
```

The FlatBuffers artifact is the product path. JSON/Markdown/HTML reports are
debugging aids.

## Proposed Repo Shape

```text
schema/
  poe_layouts.fbs

crates/
  poe-schema/
    build.rs FlatBuffers generation for Rust
    generated Rust bindings

  poe-ggpk/
    GGPK and bundle/index reading
    file listing and logical-path lookup
    raw file extraction

  poe-extract/
    scope-aware extraction
    campaign Acts 1-5 scraper
    dependency traversal for referenced terrain files

  poe-formats/
    low-level parsers for PoE formats
    .dgr, .tsi, .arm, .et, table exports

  poe-model/
    semantic Rust structs used before FlatBuffers serialization
    layout classification helpers

  poe-pipeline/
    extract -> parse -> validate -> write FlatBuffers
    report generation

  poe-cli/
    user-facing commands for discovery, extraction, builds, validation

app/
  src/generated/
    generated TypeScript FlatBuffers bindings
  src/data/
    artifact loading and query helpers
  src/render/
    Pixi layout renderer
  src/ui/
    search, sidebars, details, filters
```

This can be implemented incrementally. The current crates can either be renamed
into this shape or replaced in place during the rewrite.

## Crate Implementation Design

This section defines crate boundaries tightly enough to audit before
implementation.

### `poe-schema`

Purpose: own generated FlatBuffers bindings and safe handwritten helpers around
the generated API.

Dependencies:

- `flatbuffers`
- `thiserror`
- build-time access to `flatc`

Responsibilities:

- Compile `schema/poe_layouts.fbs` for Rust.
- Expose generated modules with a stable crate path.
- Provide small validation helpers for reading a `LayoutDatabase` from bytes.
- Avoid depending on parser, GGPK, extractor, or app crates.

Public API sketch:

```rust
pub mod generated;

pub fn root_layout_database(
    bytes: &[u8],
) -> Result<generated::poe_layouts::LayoutDatabase<'_>, SchemaError>;
```

Implementation notes:

- The schema crate should not build semantic models. It should only expose the
  FlatBuffers contract.
- Schema version should be stored in the artifact and checked by the app.

### `poe-ggpk`

Purpose: read local PoE archive data and expose logical files.

Dependencies:

- `byteorder`
- `blake3` only if this crate computes file hashes directly
- `thiserror`
- `tracing`

Responsibilities:

- Discover `Content.ggpk` from `--poe-dir` or direct `--ggpk`.
- Parse the GGPK directory tree.
- List logical paths.
- Read file bytes by logical path.
- Return provenance metadata: logical path, byte size, optional simple content
  hash, archive offset if known.

Public API sketch:

```rust
pub struct GgpkArchive {
    // owns File and parsed index
}

pub struct GgpkFileEntry {
    pub logical_path: String,
    pub byte_size: u64,
    pub hash: Option<String>,
}

impl GgpkArchive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, GgpkError>;
    pub fn list(&self) -> impl Iterator<Item = &GgpkFileEntry>;
    pub fn read_file(&mut self, logical_path: &str) -> Result<Vec<u8>, GgpkError>;
    pub fn read_matching(&mut self, matcher: &PathMatcher) -> Result<Vec<RawFile>, GgpkError>;
}
```

Implementation notes:

- Start with `File + Seek + Read`. Do not use mmap until necessary.
- Keep path normalization explicit: archive paths should use `/`; lookups should
  be case-insensitive only if the GGPK behavior requires it.
- Do not decode PoE-specific formats in this crate.
- Add fixture tests for index parsing and file extraction.

### `poe-extract`

Purpose: decide which logical files are needed for a scope and extract them into
a raw cache.

Dependencies:

- `poe-ggpk`
- `poe-formats` for shallow dependency discovery
- `globset`
- `walkdir`
- `blake3` only if the extractor writes hash fields into the manifest
- `serde`
- `serde_json`
- `thiserror`
- `tracing`

Responsibilities:

- Define extraction scopes, starting with `campaign-acts-1-5`.
- Extract required tables.
- Parse enough table/terrain metadata to discover dependent files.
- Write raw files to a deterministic cache layout.
- Write an extraction manifest.

Raw cache layout:

```text
data/raw/
  files/
    metadata/terrain/...  # normalized logical paths for inspectability
  manifest.json
```

Public API sketch:

```rust
pub enum ExtractScope {
    CampaignActs { min_act: i32, max_act: i32 },
}

pub struct ExtractOptions {
    pub scope: ExtractScope,
    pub raw_out: PathBuf,
}

pub struct ExtractReport {
    pub files: Vec<ExtractedFile>,
    pub missing: Vec<MissingFile>,
    pub warnings: Vec<ExtractWarning>,
}

pub fn extract_scope(
    archive: &mut GgpkArchive,
    options: &ExtractOptions,
) -> Result<ExtractReport, ExtractError>;
```

Implementation notes:

- Dependency discovery should be staged:
  1. Tables.
  2. Topology graph files.
  3. Master `.tsi` files from `.dgr`.
  4. Room files under terrain roots.
  5. Edge/auxiliary files discovered during parser work.
- Missing dependencies should be report issues, not panics.
- The extractor can accept an already-extracted fixture directory while GGPK
  support is being finished. That path should use the same manifest shape.

### `poe-formats`

Purpose: parse individual PoE file/table formats into low-level structs.

Dependencies:

- `thiserror`
- `serde` for debug output
- `winnow` when useful
- `byteorder` for binary table/index parsing
- `tracing`

Responsibilities:

- Decode UTF-16 LE text files when needed.
- Parse `.dgr` graph files.
- Parse `.tsi` master files.
- Parse `.arm` room files.
- Parse `.et` edge-tile files once edge details matter.
- Parse or adapt table data for `WorldAreas` and `Topologies`.

Public API sketch:

```rust
pub mod terrain {
    pub fn parse_dgr(path: &str, bytes: &[u8]) -> Result<TerrainGraph, ParseError>;
    pub fn parse_tsi(path: &str, bytes: &[u8]) -> Result<TerrainMaster, ParseError>;
    pub fn parse_arm(path: &str, bytes: &[u8]) -> Result<TerrainRoom, ParseError>;
}

pub mod tables {
    pub fn parse_world_areas(bytes: &[u8]) -> Result<Vec<WorldAreaRow>, ParseError>;
    pub fn parse_topologies(bytes: &[u8]) -> Result<Vec<TopologyRow>, ParseError>;
}
```

Implementation notes:

- Low-level structs should include unknown fields where possible.
- Parsers should return partial context in errors: path, line number, field name.
- Parser tests should include real small fixtures and snapshot outputs.
- Keep semantic decisions out of this crate. For example, parser returns room
  tags; model builder decides what those tags mean.

### `poe-model`

Purpose: hold handwritten semantic Rust structs and classification logic before
serializing to FlatBuffers.

Dependencies:

- `serde`
- `thiserror`
- `tracing`
- `poe-formats`

Responsibilities:

- Represent zones, topologies, graphs, nodes, edges, rooms, candidates, and
  issues in ergonomic Rust types.
- Resolve fixed room candidates.
- Group room variants.
- Classify environment kind with evidence.
- Provide conversion-ready structures for `poe-pipeline`.

Public API sketch:

```rust
pub struct LayoutModel {
    pub schema_version: String,
    pub game_version: String,
    pub scope: String,
    pub zones: Vec<ZoneModel>,
    pub source_files: Vec<SourceFileModel>,
    pub issues: Vec<ModelIssue>,
}

pub fn build_layout_model(input: ParsedCorpus) -> LayoutModel;
```

Implementation notes:

- Do not expose FlatBuffers generated types here.
- The model can be richer or messier than the final schema while we iterate.
- Classifications should be evidence-based:

```rust
pub struct EnvironmentClassification {
    pub kind: EnvironmentKind,
    pub confidence: f32,
    pub evidence: Vec<String>,
}
```

### `poe-pipeline`

Purpose: orchestrate extraction, parsing, semantic model building, validation,
and artifact writing.

Dependencies:

- `poe-ggpk`
- `poe-extract`
- `poe-formats`
- `poe-model`
- `poe-schema`
- `flatbuffers`
- `blake3` if artifact/raw hashes are emitted
- `serde_json`
- `time`
- `tempfile`
- `thiserror`
- `tracing`

Responsibilities:

- Provide the main `build-layout-db` operation.
- Convert `LayoutModel` into `layouts.bin`.
- Write debug reports.
- Validate high-level counts and issue severity.
- Write artifacts atomically.

Public API sketch:

```rust
pub struct BuildOptions {
    pub poe_dir: PathBuf,
    pub scope: ExtractScope,
    pub out: PathBuf,
    pub report_dir: PathBuf,
}

pub struct BuildReport {
    pub zones: usize,
    pub topologies: usize,
    pub parsed_graphs: usize,
    pub warnings: usize,
    pub errors: usize,
    pub artifact_hash: Option<String>,
}

pub fn build_layout_database(options: &BuildOptions) -> Result<BuildReport, PipelineError>;
```

Implementation notes:

- Artifact writes should be atomic: write temp file, fsync if practical, rename.
- Reports should be generated from the same model/artifact, not a parallel path.
- Pipeline must be deterministic for the same input files.

### `poe-cli`

Purpose: user-facing binary only.

Dependencies:

- `clap`
- `anyhow`
- `tracing-subscriber`
- workspace crates

Responsibilities:

- Parse arguments.
- Configure logging.
- Call library crates.
- Print concise summaries.
- Return useful exit codes.

Implementation notes:

- Keep command handlers thin.
- Prefer typed options passed into `poe-pipeline`, `poe-extract`, or `poe-ggpk`.

### Tauri App

Purpose: inspect the generated layout artifact.

Dependencies:

- `@tauri-apps/api`
- `react`
- `react-dom`
- `pixi.js`
- `flatbuffers`
- optional `lucide-react` for React toolbar/chrome icons
- TypeScript/Vite
- `playwright-core` for validation

Module design:

```text
app/src/
  generated/
    poe_layouts.ts

  data/
    loadLayoutDatabase.ts
    layoutQueries.ts
    viewModels.ts

  render/
    PixiLayoutRenderer.ts
    graphGeometry.ts
    hitTesting.ts
    colors.ts

  components/
    App.tsx
    SearchBox.tsx
    ZoneList.tsx
    TopologyList.tsx
    DetailsPanel.tsx
    Toolbar.tsx
```

Implementation notes:

- The generated FlatBuffers API is awkward for direct UI use. Add query/view
  model helpers that expose stable arrays and strings to renderer code.
- React owns the app shell: search, lists, tabs, toolbar controls, detail
  panels, and selected/hovered state.
- Keep Pixi state isolated in `PixiLayoutRenderer`.
- Pixi owns all graph/world rendering and in-canvas glyph/icon rendering.
- Prefer Pixi-rendered glyphs for anything attached to nodes, edges, rooms, or
  world geometry.
- Do not put parser/model assumptions directly in rendering code; the renderer
  consumes view models.
- Maintain a small app state:

```ts
type AppState = {
  selectedZoneId: string;
  selectedTopologyId: string;
  selectedNodeIndex: number | null;
  selectedEdgeIndex: number | null;
  query: string;
  rotationDegrees: number;
  zoom: number;
  pan: { x: number; y: number };
  detailMode: "summary" | "raw" | "issues";
};
```

## Implementation Design Details

### Data Flow

The rewrite should have one directional flow:

```text
Archive bytes
  -> RawFile
  -> ParsedFile
  -> ParsedCorpus
  -> LayoutModel
  -> FlatBuffers LayoutDatabase
  -> App view models
  -> Pixi display objects
```

Data should not flow backward. The app does not parse PoE files. The renderer
does not inspect raw GGPK data. Debug exports are generated from pipeline/model
state.

### Provenance Model

Every app-visible object should be explainable by source file id:

- `Topology.graph_path` points to the logical `.dgr`.
- `GraphNode.source_file` points to the `.dgr`.
- `GraphEdge.source_file` points to the `.dgr` or `.et` once `.et` is parsed.
- `RoomCandidate.source_file` points to the `.arm`.
- `Issue.source_file` points to the most specific known source file.

The UI can then display provenance consistently without knowing file-system
paths outside the artifact.

### Error and Issue Model

Use two concepts:

- Rust `Result<T, Error>` for infrastructure failures that prevent progress:
  unreadable archive, invalid output path, malformed schema artifact.
- Artifact `Issue` records for data problems that should be visible but should
  not stop the whole build: missing graph, missing room candidate, unknown
  terrain field, low-confidence classification.

Issue codes should be stable strings:

```text
missing_topology
missing_graph_file
parse_dgr_failed
missing_room_candidate
unknown_environment
disconnected_graph
```

Stable codes make filtering and regression tests easier.

### FlatBuffers Write Strategy

Rust writer design:

1. Build all strings/vectors bottom-up using `FlatBufferBuilder`.
2. Convert source files first so ids are stable.
3. Convert room candidates before graph nodes where possible.
4. Convert graph nodes/edges.
5. Convert layout graphs.
6. Convert topologies.
7. Convert zones.
8. Finish `LayoutDatabase`.

Keep this code in one serializer module:

```text
crates/poe-pipeline/src/flatbuffer_writer.rs
```

### FlatBuffers Read Strategy

TypeScript reader design:

1. Fetch `layouts.bin` as `ArrayBuffer`.
2. Wrap with `flatbuffers.ByteBuffer`.
3. Read root `LayoutDatabase`.
4. Build lightweight search indexes:
   - zone id -> zone index
   - lowercase search text -> zone index
   - selected topology id -> topology accessor
5. Build per-selected-layout view models lazily.

Do not eagerly copy the entire FlatBuffers artifact into plain JavaScript
objects unless profiling shows accessor overhead is a problem.

### Search and Indexing

Initial search is client-side:

- zone id
- zone name
- act
- topology id
- graph file basename

Search index can be built once after artifact load:

```ts
type ZoneSearchEntry = {
  zoneIndex: number;
  text: string;
};
```

If artifact size grows substantially later, add precomputed search tokens to the
FlatBuffers schema.

### Rendering Design

Coordinate stages:

1. Raw graph coordinates from artifact.
2. Optional world rotation, default `-28` degrees.
3. Bounds fit into viewport.
4. User zoom/pan transform.
5. Pixi object positions.

Renderer layers:

```text
stage
  graphLayer
    edgeLayer
    nodeLayer
    labelLayer
  hoverLayer
  selectionLayer
```

Hit testing:

- Nodes use circular hit areas.
- Edges use distance-to-segment checks in screen space.
- Hover state is converted back into app state so details panels update.

Color semantics:

- random/anonymous node
- fixed room node with exactly one candidate
- fixed room node with multiple candidates
- missing candidate node
- selected/hovered node
- highlighted issue node

### Validation Design

Validation should exist at three levels:

1. Parser tests:
   - Given fixture file bytes, parsed low-level output matches snapshot.
2. Pipeline tests:
   - Given fixture corpus, generated counts/issues match snapshot.
3. UI smoke tests:
   - Given generated artifact, app renders nonblank graph and no overflow.

Validation command should read the artifact and report:

```text
zones
topologies
graphs
nodes
edges
source files
warnings
errors
missing room candidates
low confidence classifications
```

This keeps FlatBuffers honest as the single contract.

### Versioning Strategy

Artifact fields:

- `schema_version`: version of `poe_layouts.fbs`.
- `game_version`: detected PoE version/build if available.
- `generated_at`: timestamp.
- `scope`: `campaign-acts-1-5`.

Compatibility rules:

- The app should refuse to load unknown major schema versions.
- The app can warn on older minor versions if fields are missing.
- The CLI should always write the current schema version.

## Canonical Schema

FlatBuffers should be the shared contract between Rust and TypeScript. The Rust
pipeline writes the artifact; the Tauri frontend reads it directly.

Initial schema file:

```text
schema/poe_layouts.fbs
```

Draft schema:

```fbs
namespace PoeLayouts;

table LayoutDatabase {
  schema_version: string;
  game_version: string;
  generated_at: string;
  scope: string;
  zones: [Zone];
  source_files: [SourceFile];
  issues: [Issue];
}

table Zone {
  id: string;
  name: string;
  act: int;
  area_level: int;
  topologies: [Topology];
  tags: [string];
  issues: [Issue];
}

table Topology {
  id: string;
  graph_path: string;
  layout_kind: LayoutKind;
  environment: EnvironmentClassification;
  graph: LayoutGraph;
  issues: [Issue];
}

table LayoutGraph {
  graph_version: int;
  width: int;
  height: int;
  room_unit_size: int;
  nodes: [GraphNode];
  edges: [GraphEdge];
  room_variants: [RoomVariantSet];
  issues: [Issue];
}

table GraphNode {
  index: int;
  x: int;
  y: int;
  room_name: string;
  orientation: string;
  tags: [string];
  candidates: [RoomCandidate];
  source_file: uint;
  raw: string;
}

table GraphEdge {
  index: int;
  from_node: int;
  to_node: int;
  edge_tile: string;
  source_file: uint;
  raw: string;
}

table RoomCandidate {
  room_name: string;
  source_path: string;
  source_file: uint;
  tags: [string];
  width: int;
  height: int;
  raw: string;
}

table RoomVariantSet {
  room_name: string;
  count: int;
  candidates: [RoomCandidate];
}

table EnvironmentClassification {
  kind: EnvironmentKind;
  confidence: float;
  evidence: [string];
}

table SourceFile {
  id: uint;
  logical_path: string;
  hash: string;
  kind: FileKind;
  byte_size: ulong;
}

table Issue {
  severity: IssueSeverity;
  code: string;
  message: string;
  source_file: uint;
}

enum LayoutKind: byte {
  Unknown,
  Campaign,
  Town,
  Boss,
  SubArea,
  Map
}

enum EnvironmentKind: byte {
  Unknown,
  Indoor,
  Outdoor,
  Hybrid
}

enum FileKind: byte {
  Unknown,
  Ggpk,
  Dat,
  Dgr,
  Tsi,
  Arm,
  Et
}

enum IssueSeverity: byte {
  Info,
  Warning,
  Error
}

root_type LayoutDatabase;
```

The exact schema will evolve, but this should be the single source of truth once
the rewrite begins. Rust parser structs can remain messy and temporary; promoted
concepts go into the FlatBuffers schema.

## GGPK Reader

The `poe-ggpk` crate should own local game archive access.

Responsibilities:

- Discover a PoE install or direct `Content.ggpk` path.
- Parse GGPK records and directory structure.
- List logical file paths.
- Extract a file by logical path.
- Support prefix/glob-like lookups needed by the scraper.
- Preserve size/provenance metadata for each extracted file, with a simple
  stable hash where it is useful for cache checks or patch diffs.

Target commands:

```sh
poe-layouts discover --poe-dir /path/to/Path\ of\ Exile
poe-layouts list --poe-dir /path/to/Path\ of\ Exile --prefix Metadata/Terrain/
poe-layouts extract-file --poe-dir /path/to/Path\ of\ Exile --path Metadata/...
```

Implementation note: port known working GGPK/bundle-reading methods rather than
inventing the format handling from scratch. Keep compatibility tests against a
small fixture archive or recorded directory manifest where possible.

## Acts 1-5 Scraper

The scraper should be scope-aware. For now, the primary scope is:

```text
campaign-acts-1-5
```

Responsibilities:

- Read required table data, especially `WorldAreas` and `Topologies`.
- Select non-town campaign areas for Acts 1-5.
- Match each selected area to topology graph files.
- Extract every referenced `.dgr`.
- Parse `.dgr` enough to discover referenced `master.tsi`.
- Extract referenced `.tsi`.
- Extract room `.arm` files under the relevant terrain root.
- Extract edge-tile and auxiliary files as we learn which ones matter.
- Store raw files in a local cache by normalized logical path. Add simple stable
  hashes in the manifest where useful; do not make content addressing the core
  design.

Target command:

```sh
poe-layouts scrape \
  --poe-dir /path/to/Path\ of\ Exile \
  --scope campaign-acts-1-5 \
  --raw-out data/raw
```

The scraper should produce a manifest of what it extracted and what it could not
resolve.

## Parsers

The `poe-formats` crate should stay focused on low-level format parsing.

Initial parser targets:

- `WorldAreas`
- `Topologies`
- `.dgr` terrain graphs
- `.tsi` terrain master files
- `.arm` rooms
- `.et` edge tiles, once needed for richer edge details

Each parser should preserve enough raw text or raw fields to support UI
inspection. When a field is unknown, keep it visible in reports rather than
dropped.

## Model Builder

The semantic model builder converts parsed files into `LayoutDatabase`.

Responsibilities:

- Build zones from selected world areas.
- Attach topology variants.
- Build graph nodes and edges.
- Resolve fixed room node candidates.
- Group room variants by room name.
- Attach source file ids to every node, edge, room, and issue.
- Compute derived classifications, such as indoor/outdoor.
- Emit validation issues without stopping the entire build.

Important derived data:

- Parsed graph count.
- Missing graph files.
- Parser failures.
- Missing fixed room candidates.
- Nodes with multiple candidate rooms.
- Disconnected or suspicious graph structures.
- Indoor/outdoor classification evidence.

## Storage and Artifacts

FlatBuffers is the canonical interchange format.

Primary artifact:

```text
app/public/data/layouts.bin
```

Useful debug artifacts:

```text
.poe-layouts/reports/campaign-acts-1-5/layouts.json
.poe-layouts/reports/campaign-acts-1-5/summary.md
.poe-layouts/reports/campaign-acts-1-5/index.html
```

Optional SQLite can still be useful later for ad hoc analysis, but it should not
be the canonical app contract. If we add it, treat it as a development index
built from the same model, not a competing source of truth.

## Tauri/React/Pixi App

The Tauri app should load generated FlatBuffers data locally. React should own
the app shell and stateful panels; Pixi.js should own the interactive graph
canvas.

Initial app requirements:

- Search for a zone by id/name/act.
- Show all known layout variants for the selected zone.
- Render a selected layout graph.
- Support the rotated PoE world view by default, plus raw orientation toggles.
- Zoom and pan a single layout.
- Toggle detail panels for the selected zone/topology/layout.
- Hover graph nodes to show:
  - node index
  - position
  - room name
  - orientation
  - tags
  - candidate room paths
  - source file/provenance
  - raw parser details
- Hover graph edges to show:
  - edge index
  - from/to nodes
  - edge tile path
  - source file/provenance
  - raw parser details
- Filter/highlight:
  - fixed nodes
  - random nodes
  - missing candidates
  - multi-candidate room nodes
  - indoor/outdoor/hybrid classification

Pixi.js is the preferred graph renderer because the core visualization is 2D
topology exploration. React should not render thousands of graph primitives.
Three.js is not needed for the first complete rewrite.

## CLI Commands

Desired command set:

```sh
poe-layouts discover --poe-dir /path/to/Path\ of\ Exile

poe-layouts list \
  --poe-dir /path/to/Path\ of\ Exile \
  --prefix Metadata/Terrain/

poe-layouts scrape \
  --poe-dir /path/to/Path\ of\ Exile \
  --scope campaign-acts-1-5 \
  --raw-out data/raw

poe-layouts build-layout-db \
  --poe-dir /path/to/Path\ of\ Exile \
  --scope campaign-acts-1-5 \
  --out app/public/data/layouts.bin

poe-layouts validate \
  --input app/public/data/layouts.bin

poe-layouts export-json \
  --input app/public/data/layouts.bin \
  --out .poe-layouts/reports/campaign-acts-1-5/layouts.json
```

`build-layout-db` can internally run scrape/parse/model/export. Separate commands
are still useful for debugging each stage.

## Implementation Steps

### Step 0: Approve Schema and Dependency Set

- Review `schema/poe_layouts.fbs` draft.
- Review the Rust and TypeScript library set.
- Confirm `crates/poe-schema/build.rs` is the canonical `flatc` invocation path.
- Decide how the `flatc` binary itself is installed or discovered:
  - checked system dependency,
  - package-manager-installed tool,
  - or documented manual prerequisite.
- Decide whether generated TypeScript bindings are checked in.
- Decide whether an extracted fixture directory remains supported while GGPK
  support is incomplete.

Done when:

- The schema draft is accepted for v0.
- The dependency list is accepted.
- The `build.rs` generation path is documented.
- The first implementation PR has a small, auditable dependency diff.

### Step 1: Establish the New Skeleton

- Add `schema/poe_layouts.fbs`.
- Add FlatBuffers compiler setup through `crates/poe-schema/build.rs`.
- Add Rust schema generation in a new `poe-schema` crate.
- Add TypeScript schema generation under `app/src/generated`.
- Add a tiny hand-built `layouts.bin` fixture.
- Make the app load the FlatBuffers fixture.

Done when:

- `cargo check --workspace` passes.
- `npm run build` passes.
- The app renders one fixture zone/layout from FlatBuffers.

### Step 2: Port/Build GGPK File Access

- Implement install discovery.
- Implement archive listing.
- Implement extraction by logical path.
- Add tests against fixtures or manifests.

Done when:

- CLI can list terrain/table files from a local install.
- CLI can extract a known file by logical path.

### Step 3: Implement Acts 1-5 Scrape Scope

- Read `WorldAreas` and `Topologies`.
- Select Acts 1-5 campaign areas.
- Resolve topology graph dependencies.
- Extract `.dgr`, `.tsi`, `.arm`, and known related files.
- Write an extraction manifest.

Done when:

- Running `scrape --scope campaign-acts-1-5` produces a raw cache and manifest.
- Missing files are reported with actionable logical paths.

### Step 4: Rebuild Terrain Parsers

- Port/refine `.dgr`, `.tsi`, and `.arm` parsers.
- Preserve raw fields needed for UI detail panels.
- Add parser-level validation issues.

Done when:

- The parser can read the extracted Acts 1-5 terrain corpus.
- Parser failures are isolated and reported without killing the full build.

### Step 5: Build the FlatBuffers Layout Artifact

- Convert parsed tables/terrain into `LayoutDatabase`.
- Write `layouts.bin`.
- Write debug JSON/Markdown reports.
- Include source file references throughout the artifact.

Done when:

- `build-layout-db --scope campaign-acts-1-5` writes `layouts.bin`.
- `validate --input layouts.bin` reports the same high-level counts as the
  debug report.

### Step 6: Rewrite the App Around FlatBuffers

- Replace JSON loading with FlatBuffers loading.
- Add query helpers for zones, topologies, nodes, edges, candidates, and issues.
- Keep the existing Pixi graph rendering idea.
- Add zoom/pan.
- Add hover details for nodes and edges.
- Add zone/topology detail toggles.

Done when:

- Search, layout list, graph render, zoom/pan, and hover details work on the
  generated Acts 1-5 artifact.
- Playwright validation screenshots confirm the app is nonblank, readable, and
  not overflowing at desktop/compact sizes.

### Step 7: Add Derived Interpretation

- Add indoor/outdoor/hybrid classification.
- Add confidence and evidence strings.
- Add filters/highlights for classification.
- Add reports for unknown or low-confidence cases.

Done when:

- Each topology has an environment classification.
- The UI can inspect why the classifier chose that value.

### Step 8: Patch-Day Automation

- Add a repeatable command that rebuilds the artifact from a local updated PoE
  install.
- Record enough metadata to diff patch outputs. Simple stable hashes are fine
  where useful; avoid a complex cache identity system.
- Emit summaries of what changed since the previous run.

Done when:

- One command rebuilds Acts 1-5 data after a patch.
- The report highlights added/removed/changed zones, topologies, rooms, and
  parser warnings.

## Validation Loop

Every implementation slice should run through this loop:

```text
extract/scrape
  -> parse
  -> build FlatBuffers
  -> validate counts/issues
  -> render in Tauri/React/Pixi
  -> visually inspect screenshot
  -> fix parser/model/UI
  -> repeat
```

Minimum validation before considering a milestone complete:

- `cargo check --workspace`
- relevant Rust tests
- `npm run build`
- app visual screenshot at desktop size
- app visual screenshot at compact size
- no horizontal overflow
- default selected layout is parsed and nonblank

## Open Questions

- Which exact GGPK/bundle implementation should be ported first?
- Do we need `.datc64` parsing directly from GGPK, or do we initially support an
  extracted table fixture path while the GGPK reader matures?
- Which auxiliary terrain files besides `.dgr`, `.tsi`, `.arm`, and `.et` affect
  indoor/outdoor classification?
- Should the app artifact include raw parser strings everywhere, or should large
  raw payloads be kept in sidecar debug files?
- How much of the old JSON report should remain once FlatBuffers is primary?

These should be resolved as part of implementation, not before the rewrite
starts.
