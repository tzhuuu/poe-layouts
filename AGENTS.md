# Agent Notes

## Code Style

- Prefer clear names, small functions, and typed boundaries over explanatory comments.
- Add comments only when they explain non-obvious format behavior, invariants, or external constraints.
- Keep component and pipeline flow explanations in Markdown docs, not scattered through code comments.
- When a new subsystem crosses crate, CLI, script, or app boundaries, update `docs/component-flows.md`.

## Current Direction

- Parser commands should default to the latest PoE1 patch version and cache by release line.
- Rust owns cache, bundle/index orchestration, parsing, and manifests.
- The current Oodle decoder is a temporary Node.js `ooz-wasm` bridge used by the CLI.
- The next milestone is the Acts 1-5 scrape scope: extract `WorldAreas`, `Topologies`, and first terrain dependencies into a raw cache plus manifest.
