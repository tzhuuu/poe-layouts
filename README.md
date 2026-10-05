# poe-layouts

Local-first Path of Exile layout visualizer rewrite.

The current target is PoE1 campaign Acts 1-5. The planned architecture is:

```text
PoE install / Content.ggpk
  -> Rust GGPK reader
  -> campaign Acts 1-5 scraper
  -> low-level file parsers
  -> semantic layout model builder
  -> FlatBuffers artifact
  -> Tauri + React + Pixi visualizer
```

See [docs/rewrite-plan.md](docs/rewrite-plan.md) for the implementation plan,
library choices, schema draft, and milestone checklist. See
[docs/component-flows.md](docs/component-flows.md) for the current pipeline and
component flows.

## Current Parser Milestone

The first implementation slice lives in:

```text
crates/poe-ggpk  low-level CDN/cache/GGPK helpers
crates/poe-cli   command-line entrypoint for pipeline debugging
```

Check the current patch versions and release-line cache namespaces:

```sh
cargo run -p poe-cli -- latest-versions
```

Commands that fetch parser inputs default to the latest PoE1 patch version from
the version endpoint. Pass `--patch-version` when you want a reproducible target.

Fetch and snapshot the current PoE1 patch index:

```sh
cargo run -p poe-cli -- snapshot-index
```

Inspect the compressed bundle header for `_.index.bin`:

```sh
cargo run -p poe-cli -- inspect-index-header
```

Fetch, decompress, and parse the live index using the temporary `ooz-wasm`
bridge:

```sh
cargo run -p poe-cli -- inspect-index --prefix metadata/terrain/ --limit 10
cargo run -p poe-cli -- inspect-index --logical-path data/worldareas.datc64
```

Extract one logical file from the patch CDN cache:

```sh
cargo run -p poe-cli -- extract-file \
  --logical-path data/worldareas.datc64 \
  --out .poe-layouts/raw/data/worldareas.datc64
```

Inspect projected columns from an extracted `.datc64` table:

```sh
cargo run -p poe-cli -- inspect-dat-table \
  --input .poe-layouts/raw/data/worldareas.datc64 \
  --table WorldAreas \
  --limit 5
```

Pass `--all-columns` to inspect every effective schema column, including stable
generated names for anonymous `_` fields.

Use an explicit patch version when you want a reproducible target:

```sh
cargo run -p poe-cli -- snapshot-index --patch-version <patch-version>
```

Build an offline cache manifest for parser inputs:

```sh
cargo run -p poe-cli -- prefetch-bundles \
  --bundle _.index.bin \
  --manifest .poe-layouts/cache-manifest.json
```

Verify that manifest later without network access:

```sh
cargo run -p poe-cli -- verify-cache \
  --manifest .poe-layouts/cache-manifest.json
```

Preview or clear the latest local PoE1 release-line cache namespace:

```sh
cargo run -p poe-cli -- clear-cache --dry-run
cargo run -p poe-cli -- clear-cache
```

Clear only one exact patch under that release line:

```sh
cargo run -p poe-cli -- clear-cache --patch-version <patch-version>
```

Refresh the checked-in table schema snapshot from `poe-tool-dev/dat-schema`:

```sh
cargo run -p poe-cli -- update-dat-schema
```

That command validates that the fetched GraphQL schema includes the immediate
scrape targets, currently `WorldAreas` and `Topologies`, then writes
`schema/dat/_Core.gql` and `schema/dat/schema-manifest.json`. Unlike bundle
commands, this refreshes from the network by default; pass `--offline` to
rebuild the checked-in files from the local cache.

The extractor will eventually generate the full bundle list for Acts 1-5 after
reading `_.index.bin`. The cache layer is already shaped so that parse/build
steps can require all inputs to exist locally before they start.

The `inspect-index` command currently uses a small Node.js `ooz-wasm` bridge for
Oodle chunks. Rust owns the bundle/index orchestration and parsing around that
bridge.

Downloaded bundle files live under `.poe-layouts/cache` by default. The live CDN
snapshot test is ignored in normal test runs; refresh it with:

```sh
INSTA_UPDATE=always cargo test -p poe-ggpk live_poe1_index_snapshot -- --ignored
```
