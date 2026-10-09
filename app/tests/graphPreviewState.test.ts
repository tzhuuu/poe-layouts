import assert from "node:assert/strict";
import { test } from "node:test";
import { graphPreviewReducer, initialGraphPreviewState } from "../src/data/graphPreviewState.ts";
import type { LayoutGraph } from "../src/data/layoutGraph.ts";

const graph: LayoutGraph = {
  logicalPath: "first.tgr", version: 19, width: 10, height: 20,
  masterFile: null, nodes: [], edges: [], warnings: [],
};
const loaded = () => graphPreviewReducer(
  graphPreviewReducer(initialGraphPreviewState(), { type: "request", request: "first" }),
  { type: "ready", request: "first", graph, outdoor: true },
);

test("requesting another layout keeps the displayed graph and projection", () => {
  const state = loaded();
  const pending = graphPreviewReducer(state, { type: "request", request: "next" });
  assert.equal(pending.view, state.view);
  assert.equal(pending.view?.graph, graph);
  assert.equal(pending.view?.outdoor, true);
  assert.equal(pending.request, "next");
});

test("the completed layout replaces graph and projection together", () => {
  const pending = graphPreviewReducer(loaded(), { type: "request", request: "next" });
  const replacement = { ...graph, logicalPath: "next.dgr" };
  const state = graphPreviewReducer(pending, { type: "ready", request: "next", graph: replacement, outdoor: false });
  assert.equal(state.view?.graph, replacement);
  assert.equal(state.view?.outdoor, false);
  assert.equal(state.view?.request, state.request);
});

test("superseded graph responses and failures cannot replace the active request", () => {
  const pending = graphPreviewReducer(loaded(), { type: "request", request: "next" });
  assert.equal(graphPreviewReducer(pending, { type: "ready", request: "first", graph, outdoor: false }), pending);
  assert.equal(graphPreviewReducer(pending, { type: "error", request: "first", message: "old failure" }), pending);
});

test("failed replacements preserve the displayed graph and retry clears the error", () => {
  const state = loaded();
  const pending = graphPreviewReducer(state, { type: "request", request: "next" });
  const failed = graphPreviewReducer(pending, { type: "error", request: "next", message: "unavailable" });
  assert.equal(failed.view, state.view);
  assert.equal(failed.error, "unavailable");
  const retry = graphPreviewReducer(failed, { type: "request", request: "retry" });
  assert.equal(retry.error, null);
  assert.equal(retry.view, state.view);
});

test("empty layouts clear the preview and ignore late responses", () => {
  const cleared = graphPreviewReducer(loaded(), { type: "clear" });
  assert.deepEqual(cleared, initialGraphPreviewState());
  assert.equal(graphPreviewReducer(cleared, { type: "ready", request: "first", graph, outdoor: true }), cleared);
});
