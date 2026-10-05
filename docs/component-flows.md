# Component Flows

This document tracks how the moving pieces fit together. Keep implementation
details in code; keep cross-component flow shape here.

## Current Components

```text
poe-cli
  -> poe-ggpk
     -> patch CDN source
     -> disk cache
     -> bundle/index parsers
     -> datc64 binary reader
     -> GraphQL dat schema interpreter
  -> scripts/ooz-decompress-bundle.mjs
     -> ooz-wasm
```

- `poe-cli` is the debugging and pipeline entrypoint.
- `poe-ggpk` owns stable Rust APIs for patch versions, cache layout, bundle
  header parsing, index parsing, path hashing, path reps unpacking, and cache
  verification. It also owns generic PoE file readers, plus source-specific
  fetch clients such as `DatSchemaClient`.
- `scripts/ooz-decompress-bundle.mjs` is the temporary Oodle bridge. It should
  stay thin: read bundle bytes, decode chunks with `ooz-wasm`, write bytes.

## `poe-ggpk` Source Layout

```text
bundle.rs             bundle header/decompression primitives
cache.rs              shared disk cache, manifests, verification, clearing
dat_schema_client.rs  fetch/update checked-in GraphQL dat schema snapshots
datc64.rs             generic .datc64 binary reader
dat_graphql.rs        GraphQL schema interpreter for named dat tables/columns
ggpk.rs               low-level GGPK record scanning primitives
index_bundle.rs       Bundles2 index parsing, path reps, path hashing
patchcdn.rs           version resolution and patch CDN fetch orchestration
patch_client.rs       high-level patch index loading and logical file extraction
```

The dependency direction should stay one-way: `dat_graphql` may depend on
`datc64`, but `datc64` must not know about GraphQL or specific tables. The
future layout scraper should depend on `GraphqlDatSchema` for table reads and
add PoE-layout semantics outside these generic readers.

`PatchClient` is the higher-level file access boundary for milestone 2. It owns
fetching cached CDN bundles, loading the decompressed index, resolving logical
paths, and extracting logical files. The caller supplies a `BundleDecompressor`
implementation; today the CLI implementation shells out to the temporary
Node.js `ooz-wasm` bridge.

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
  checked-in schema snapshot.
- Two-layer table reading:
  - `datc64` is the generic PoE file reader for `.datc64` envelopes, primitive
    field layouts, UTF-16 strings, row keys, arrays, and projected rows.
  - `dat_graphql` is the schema-specific layer for GraphQL table definitions,
    effective column names, column layouts, and named projections.
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
  -> schema/dat/_Core.gql
  -> schema/dat/schema-manifest.json
```

The GraphQL schema snapshot is the contract for interpreting extracted table
files. The current scraper milestone requires at least `WorldAreas` and
`Topologies`; future table parsing should use the checked-in snapshot by
default and refresh it explicitly when we need upstream schema changes. Schema
refreshes force a network fetch by default; `--offline` rebuilds the checked-in
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
  -> poe-ggpk parses index sections
  -> ooz bridge decompresses nested path reps bundle
  -> poe-ggpk unpacks logical paths
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

## Dat Table Reader Flow

```text
inspect-dat-table
  -> read schema/dat/_Core.gql
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
checked-in GraphQL schema, then reuse it across raw table files after extracting
them from the patch CDN.

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
  -> record simple .dgr/.arm path variants
  -> extract candidates that are present and available in cache/network mode
  -> write raw cache and manifest with missing paths/warnings
```

The goal of this milestone is not layout modeling yet. It is to produce a
repeatable raw corpus and manifest that the parser/model milestones can consume.

Current first pass: `scrape-campaign-acts-1-5` writes
`.poe-layouts/raw/campaign-acts-1-5/manifest.json`. In offline mode with only the
current table bundles cached, it selects 81 main campaign areas and records
terrain candidates as missing cache entries rather than failing the scrape.

## Refactor Notes

The next useful cleanup is to move the milestone-2 domain logic out of
`poe-cli` and behind a typed layout-data API. The shape should be:

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
