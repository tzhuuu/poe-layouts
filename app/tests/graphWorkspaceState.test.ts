import assert from "node:assert/strict";
import { test } from "node:test";
import type { LayoutGraph, LayoutGraphNode } from "../src/data/layoutGraph.ts";
import { graphWorkspaceReducer, initialGraphWorkspaceState, inspectedGraphNode } from "../src/data/graphWorkspaceState.ts";

const nodes: LayoutGraphNode[] = [0, 7].map((index) => ({
  index, x: index * 24, y: index * 48, links: [], label: `room${index}`,
  rotation: "R90", metadata: [], transitions: [], roomBosses: null,
}));
const graph: LayoutGraph = { logicalPath: "test.tgr", version: 19, width: 36, height: 48, masterFile: null, nodes, edges: [], warnings: [] };
const loaded = () => graphWorkspaceReducer(initialGraphWorkspaceState(), { type: "load", graph });

test("hover previews a node and leaving restores the pinned selection", () => {
  let state = graphWorkspaceReducer(loaded(), { type: "select", index: 0 });
  state = graphWorkspaceReducer(state, { type: "hover", index: 7 });
  assert.equal(inspectedGraphNode(state)?.index, 7);
  state = graphWorkspaceReducer(state, { type: "leave", index: 7 });
  assert.equal(inspectedGraphNode(state)?.index, 0);
});

test("unselected hover clears on leave and stale leave events do not clear a new hover", () => {
  let state = graphWorkspaceReducer(loaded(), { type: "hover", index: 0 });
  state = graphWorkspaceReducer(state, { type: "hover", index: 7 });
  state = graphWorkspaceReducer(state, { type: "leave", index: 0 });
  assert.equal(inspectedGraphNode(state)?.index, 7);
  state = graphWorkspaceReducer(state, { type: "hover", index: null });
  assert.equal(inspectedGraphNode(state), null);
});

test("selection is independent of rotation overrides and directions", () => {
  let state = graphWorkspaceReducer(loaded(), { type: "select", index: 0 });
  state = graphWorkspaceReducer(state, { type: "rotate", index: 0, rotation: "R90" });
  state = graphWorkspaceReducer(state, { type: "rotate", index: 7, rotation: "R270" });
  state = graphWorkspaceReducer(state, { type: "direction", direction: "clockwise" });
  assert.equal(inspectedGraphNode(state), graph.nodes[0]);
  assert.deepEqual(state.rotations, { 0: "R90", 7: "R270" });
  state = graphWorkspaceReducer(state, { type: "rotate", index: 7, rotation: undefined });
  assert.deepEqual(state.rotations, { 0: "R90" });
  state = graphWorkspaceReducer(state, { type: "select", index: null });
  assert.equal(inspectedGraphNode(state), null);
  assert.equal(state.rotations[0], "R90");
  assert.equal(graph.nodes[0].rotation, "R90");
});

test("loading a different graph clears stale selections and overrides", () => {
  let state = graphWorkspaceReducer(loaded(), { type: "select", index: 7 });
  state = graphWorkspaceReducer(state, { type: "rotate", index: 7, rotation: "R180" });
  state = graphWorkspaceReducer(state, { type: "hover", index: 0 });
  state = graphWorkspaceReducer(state, { type: "load", graph: { ...graph, logicalPath: "next.tgr" } });
  assert.equal(inspectedGraphNode(state), null);
  assert.deepEqual(state.rotations, {});
  assert.equal(state.direction, "counterclockwise");
});

test("unknown node IDs cannot create selections or rotation overrides", () => {
  const state = loaded();
  assert.equal(graphWorkspaceReducer(state, { type: "select", index: 99 }), state);
  assert.equal(graphWorkspaceReducer(state, { type: "hover", index: 99 }), state);
  assert.equal(graphWorkspaceReducer(state, { type: "rotate", index: 99, rotation: "R90" }), state);
});
