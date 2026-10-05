# Review Later

Notes for the next audit pass when you are back at the repo.

## Fetching Abstraction

We now have three related fetch paths:

- Latest game versions from `https://poe-versions.obsoleet.org`
- Patch CDN bundle files under `https://patch.poecdn.com/<patch>/Bundles2/...`
- Latest table schema from `poe-tool-dev/dat-schema`'s `_Core.gql`

These should probably converge behind a small client layer instead of letting
each CLI command perform ad hoc network work.

## Files To Review

- `crates/poe-ggpk/src/cache.rs`
  - Owns `DiskCache`, cache modes, metadata sidecars, manifests, verification,
    and cache clearing.
  - This is the reusable foundation.
- `crates/poe-ggpk/src/patchcdn.rs`
  - Owns latest-version lookup, `PatchCdnSource`, CDN URL construction, and
    bundle fetching.
  - This is close to a game-content client, but it is currently specific to the
    patch CDN.
- `crates/poe-ggpk/src/dat_schema.rs`
  - Owns `DatSchemaClient`, schema snapshot validation, and schema manifest
    writing.
  - This is the first milestone-2 fetch client extraction.
- `crates/poe-cli/src/main.rs`
  - `update-dat-schema` now delegates GraphQL schema fetching and validation to
    `poe-ggpk`.
  - Continue moving remote-input logic out of the CLI as the scraper grows.
- `schema/dat/_Core.gql`
  - Checked-in schema snapshot from `poe-tool-dev/dat-schema`.
  - Confirm the `WorldAreas` and `Topologies` fields look like the tables we
    want to parse first.
- `schema/dat/schema-manifest.json`
  - Current schema snapshot metadata: source URL, byte length, BLAKE3, and
    required table names.
- `docs/component-flows.md`
  - Current flow docs. Keep this updated when the client layer appears.

## Proposed Shape

Add a generic source/client layer, probably in `poe-ggpk` at first unless it
grows enough to deserve a separate crate.

Possible API shape:

```rust
pub struct PoeDataClient {
    cache: DiskCache,
}

impl PoeDataClient {
    pub fn latest_versions(&self, mode: CacheMode) -> Result<LatestPatchVersions, Error>;
    pub fn patch_cdn(&self, game: PoeGame, patch_version: String) -> PatchCdnClient;
    pub fn dat_schema(&self) -> DatSchemaClient;
}
```

Then keep source-specific clients thin:

```rust
pub struct PatchCdnClient {
    source: PatchCdnSource,
    cache: DiskCache,
}

pub struct DatSchemaClient {
    source_url: String,
    cache: DiskCache,
}
```

The key idea: all remote inputs should share the same boring contract:

- Build a stable cache key.
- Fetch online or fail offline.
- Write byte metadata next to cached files.
- Emit a manifest with source URL, byte length, and BLAKE3.
- Validate source-specific invariants after fetch.

## Open Design Questions

- Should latest-version JSON be cached too, or should only large/static inputs
  use the disk cache?
- Should the GraphQL schema snapshot live in `.poe-layouts/cache` first and then
  be copied to `schema/dat/_Core.gql` only when intentionally refreshing the
  checked-in snapshot?
- Should schema validation be generic, for example requiring a list of table
  types, so future monster-data work can ask for `MonsterVarieties`, `Mods`,
  `Tags`, etc. without adding new CLI logic?
- Should manifests be one type with a `kind` field, or separate manifest structs
  for CDN bundles, schema snapshots, and future raw scrape outputs?

## Likely Next Commits

- Fold more patch-CDN orchestration behind client-shaped APIs so the CLI mostly
  wires options to library calls.
- Start the `.datc64` table reader against the checked-in schema snapshot,
  beginning with `WorldAreas` and `Topologies`.
