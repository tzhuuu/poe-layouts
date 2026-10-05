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
  -> scripts/ooz-decompress-bundle.mjs
     -> ooz-wasm
```

- `poe-cli` is the debugging and pipeline entrypoint.
- `poe-ggpk` owns stable Rust APIs for patch versions, cache layout, bundle
  header parsing, index parsing, path hashing, path reps unpacking, and cache
  verification. It also owns source-specific fetch clients such as
  `DatSchemaClient`.
- `scripts/ooz-decompress-bundle.mjs` is the temporary Oodle bridge. It should
  stay thin: read bundle bytes, decode chunks with `ooz-wasm`, write bytes.

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
  -> load/decompress/parse _.index.bin
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
  -> DatTableReader parses schema tables once
  -> parse local .datc64 fixed/variable sections
  -> generate stable names for anonymous schema fields
  -> project requested columns or all effective columns
  -> print rows as JSON
```

The reader currently targets `.datc64`, which is enough for `WorldAreas` and
`Topologies`. It supports scalar primitives, row keys, foreign row keys, strings,
and arrays. The library boundary is `DatTableReader`: scrape/build code should
construct it once from the checked-in GraphQL schema, then reuse it across raw
table files after extracting them from the patch CDN.

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
  -> read schema/dat/_Core.gql
  -> extract data/worldareas.datc64
  -> read WorldAreas columns
  -> extract and read topology table/files
  -> select Acts 1-5 campaign zones
  -> resolve terrain graph dependencies
  -> extract .dgr, .tsi, .arm candidates
  -> write raw cache
  -> write extraction manifest with missing paths and warnings
```

The goal of this milestone is not layout modeling yet. It is to produce a
repeatable raw corpus and manifest that the parser/model milestones can consume.
