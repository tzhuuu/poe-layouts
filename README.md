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
