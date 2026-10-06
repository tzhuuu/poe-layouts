# poe-layouts

Local-first Path of Exile layout visualizer rewrite.

The app target is a local Rust web server plus React + Pixi. There is no hosted
backend in the planned product shape; Rust commands fetch/update data into local
artifacts, and `pather-layouts-server` serves the browser UI plus local API endpoints.

The current target is PoE1 campaign Acts 1-5. The planned architecture is:

```text
PoE install / Content.ggpk
  -> Rust GGPK reader
  -> campaign Acts 1-5 scraper
  -> low-level file parsers
  -> semantic layout model builder
  -> FlatBuffers artifact
  -> pather-layouts-server + React + Pixi visualizer
```

See [docs/rewrite-plan.md](docs/rewrite-plan.md) for the implementation plan,
library choices, schema draft, and milestone checklist. See
[docs/component-flows.md](docs/component-flows.md) for the current pipeline and
component flows.

## Current Parser Milestone

The first implementation slice lives in:

```text
crates/poe-content  low-level CDN/cache/GGPK helpers
crates/poe-dat      generic datc64 and GraphQL DAT table readers
crates/pather-schema  app-facing FlatBuffers schema helpers
crates/pather-core  campaign scrape and layout artifact APIs
crates/pather-cli   command-line entrypoint for pipeline debugging
crates/pather-layouts-server  local layout HTTP API and static webapp server
```

Check the current patch versions and release-line cache namespaces:

```sh
cargo run -p pather-cli -- latest-versions
```

Commands that fetch parser inputs default to the latest PoE1 patch version from
the version endpoint. Pass `--patch-version` when you want a reproducible target.

Fetch and snapshot the current PoE1 patch index:

```sh
cargo run -p pather-cli -- snapshot-index
```

Inspect the compressed bundle header for `_.index.bin`:

```sh
cargo run -p pather-cli -- inspect-index-header
```

Fetch, decompress, and parse the live index using the temporary `ooz-wasm`
bridge:

```sh
cargo run -p pather-cli -- inspect-index --prefix metadata/terrain/ --limit 10
cargo run -p pather-cli -- inspect-index --logical-path data/worldareas.datc64
```

Extract one logical file from the patch CDN cache:

```sh
cargo run -p pather-cli -- extract-file \
  --logical-path data/worldareas.datc64 \
  --out .poe-layouts/raw/data/worldareas.datc64
```

Browse logical GGPK folders without extracting bytes:

```sh
cargo run -p pather-cli -- browse-files \
  --prefix Metadata/Terrain/Act1/Area1

cargo run -p pather-cli -- browse-files \
  --prefix Metadata/Terrain/Act1/Area1 \
  --recursive \
  --extension dgr
```

Extract a whole logical folder while preserving the GGPK path hierarchy:

```sh
cargo run -p pather-cli -- extract-folder \
  --prefix Metadata/Terrain/Act1/Area1

cargo run -p pather-cli -- extract-folder \
  --prefix Metadata/Terrain/Act1/Area1 \
  --extension dgr
```

Folder extracts write files under `.poe-layouts/raw/files` by default and write a
JSON manifest to `.poe-layouts/raw/extract-folder-manifest.json`.

The campaign scrape infers broader terrain area folders from table-declared
files. For example, a candidate under `metadata/terrain/act1/area1/...` causes
the scrape to extract all of `metadata/terrain/act1/area1`. Add manual roots
with repeated `--raw-folder` flags:

```sh
cargo run -p pather-cli -- scrape-campaign-acts-1-5 \
  --raw-folder Metadata/Terrain/Act1/Town
```

Inspect projected columns from an extracted `.datc64` table:

```sh
cargo run -p pather-cli -- inspect-dat-table \
  --input .poe-layouts/raw/data/worldareas.datc64 \
  --table WorldAreas \
  --limit 5
```

Pass `--all-columns` to inspect every effective schema column, including stable
generated names for anonymous `_` fields.

Use an explicit patch version when you want a reproducible target:

```sh
cargo run -p pather-cli -- snapshot-index --patch-version <patch-version>
```

Build an offline cache manifest for parser inputs:

```sh
cargo run -p pather-cli -- prefetch-bundles \
  --bundle _.index.bin \
  --manifest .poe-layouts/cache-manifest.json
```

Verify that manifest later without network access:

```sh
cargo run -p pather-cli -- verify-cache \
  --manifest .poe-layouts/cache-manifest.json
```

Preview or clear the latest local PoE1 release-line cache namespace:

```sh
cargo run -p pather-cli -- clear-cache --dry-run
cargo run -p pather-cli -- clear-cache
```

Clear only one exact patch under that release line:

```sh
cargo run -p pather-cli -- clear-cache --patch-version <patch-version>
```

Refresh the local table schema snapshot from `poe-tool-dev/dat-schema`:

```sh
cargo run -p pather-cli -- update-dat-schema
```

That command validates that the fetched GraphQL schema includes the immediate
scrape targets, currently `WorldAreas` and `Topologies`, then writes
`data/cache/dat-schema/_Core.gql` and `data/cache/dat-schema/schema-manifest.json`. Unlike bundle
commands, this refreshes from the network by default; pass `--offline` to
rebuild the local files from the local cache.

Scrape the current Acts 1-5 campaign scope into the raw cache plus the
app-facing `FlatBuffers` artifact:

```sh
cargo run -p pather-cli -- scrape-campaign-acts-1-5
```

The default outputs are:

```text
.poe-layouts/raw/campaign-acts-1-5/manifest.json
.poe-layouts/raw/campaign-acts-1-5/high-level-graph.json
app/public/data/layouts.bin
```

The scrape extracts `WorldAreas`, `Topologies`, table-declared terrain
candidates, and the parent terrain folder for each candidate. The broader raw
corpus lands under:

```text
.poe-layouts/raw/campaign-acts-1-5/files
```

The high-level graph is the handoff artifact for early layout browsing and room
rendering work. It contains stable nodes for acts, areas, referenced topologies,
terrain folders, and table-declared terrain files, plus typed edges such as
`contains_area`, `uses_topology`, `declares_graph`, `declares_tsi`, and
`groups_terrain_file`.

Inspect the generated app artifact:

```sh
cargo run -p pather-cli -- inspect-layout-db
```

When `pather-layouts-server` is running, browse and fetch extracted raw files:

```text
http://127.0.0.1:5174/api/raw-files?prefix=metadata/terrain/act1/area1
http://127.0.0.1:5174/api/raw-files?prefix=metadata/terrain/act1/area1&recursive=true&extension=dgr
http://127.0.0.1:5174/raw-files/metadata/terrain/act1/area1/example.dgr
```

The `inspect-index` command currently uses a small Node.js `ooz-wasm` bridge for
Oodle chunks. Rust owns the bundle/index orchestration and parsing around that
bridge.

Downloaded bundle files live under `.poe-layouts/cache` by default. The live CDN
snapshot test is ignored in normal test runs; refresh it with:

```sh
INSTA_UPDATE=always cargo test -p poe-content live_poe1_index_snapshot -- --ignored
```
