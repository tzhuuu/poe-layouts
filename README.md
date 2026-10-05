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
library choices, schema draft, and milestone checklist.

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

Fetch and snapshot the current PoE1 patch index:

```sh
cargo run -p poe-cli -- snapshot-index
```

Use an explicit patch version when you want a reproducible target:

```sh
cargo run -p poe-cli -- snapshot-index --patch-version 3.29.3.3
```

Build an offline cache manifest for parser inputs:

```sh
cargo run -p poe-cli -- prefetch-bundles \
  --patch-version 3.29.3.3 \
  --bundle _.index.bin \
  --manifest .poe-layouts/cache-manifest.json
```

Verify that manifest later without network access:

```sh
cargo run -p poe-cli -- verify-cache \
  --manifest .poe-layouts/cache-manifest.json
```

The extractor will eventually generate the full bundle list for Acts 1-5 after
reading `_.index.bin`. The cache layer is already shaped so that parse/build
steps can require all inputs to exist locally before they start.

Downloaded bundle files live under `.poe-layouts/cache` by default. The live CDN
snapshot test is ignored in normal test runs; refresh it with:

```sh
INSTA_UPDATE=always cargo test -p poe-ggpk live_poe1_index_snapshot -- --ignored
```
