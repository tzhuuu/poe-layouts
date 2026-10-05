# poe-ggpk

Low-level Path of Exile archive and patch CDN access.

This crate is intentionally narrow for the first parser milestone:

- build stable PoE patch CDN URLs for PoE1 and PoE2
- inspect compressed bundle headers
- parse decompressed `Bundles2/_.index.bin` structures
- resolve logical file paths to bundle slices from a decompressed index
- fetch patch CDN bundle files through a disk cache
- record cache metadata next to downloaded files
- write and verify offline cache manifests for parser inputs
- scan classic GGPK record headers from any `Read + Seek` source
- provide snapshot tests for stable behavior and opt-in live CDN checks

The cache layout mirrors the CDN path under the selected cache root:

```text
.poe-layouts/cache/
  poe1/<release-line>/patches/<patch-version>/Bundles2/_.index.bin
  poe1/<release-line>/patches/<patch-version>/Bundles2/_.index.bin.json
```

The parser pipeline should prefetch every bundle it needs, write a manifest,
then run later parse/build steps in offline mode against that manifest:

```sh
cargo run -p poe-cli -- prefetch-bundles \
  --bundle _.index.bin \
  --manifest .poe-layouts/cache-manifest.json

cargo run -p poe-cli -- verify-cache \
  --manifest .poe-layouts/cache-manifest.json
```

By default, parser commands use the latest PoE1 patch version from
`https://poe-versions.obsoleet.org`. The latest release-line cache namespace can
be previewed or cleared with:

```sh
cargo run -p poe-cli -- clear-cache --dry-run
cargo run -p poe-cli -- clear-cache
```

Or at one exact patch version:

```sh
cargo run -p poe-cli -- clear-cache --patch-version <patch-version>
```

The future extractor should produce the bundle list after resolving logical
files from `_.index.bin`; this crate already provides the offline cache contract
that step will use.

The index parser intentionally stops at decompressed index bytes. The current JS
toolchain uses Oodle via `ooz-wasm`; porting or wrapping decompression is the
next step before live CDN logical-path listing can be fully Rust-native.

The live test is ignored by default because it depends on the current patch CDN:

```sh
POE_LAYOUTS_PATCH_VERSION=<patch-version> \
  INSTA_UPDATE=always \
  cargo test -p poe-ggpk live_poe1_index_snapshot -- --ignored
```

Without `POE_LAYOUTS_PATCH_VERSION`, the test queries
`https://poe-versions.obsoleet.org` and uses the returned PoE1 version.
