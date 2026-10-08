import assert from "node:assert/strict";
import { test } from "node:test";
import { adjacentLayout, layoutKeyStep } from "../src/data/layoutNavigation.ts";

test("layout arrows follow the available candidate order", () => {
  const paths = ["first.tgr", "second.tgr", "third.dgr"];
  assert.equal(adjacentLayout(paths, "second.tgr", -1), "first.tgr");
  assert.equal(adjacentLayout(paths, "second.tgr", 1), "third.dgr");
});

test("layout navigation stops at either end", () => {
  const paths = ["first.tgr", "last.tgr"];
  assert.equal(adjacentLayout(paths, "first.tgr", -1), null);
  assert.equal(adjacentLayout(paths, "last.tgr", 1), null);
});

test("empty, single-layout, and unknown selections have no invalid neighbor", () => {
  assert.equal(adjacentLayout([], null, 1), null);
  assert.equal(adjacentLayout(["only.tgr"], "only.tgr", -1), null);
  assert.equal(adjacentLayout(["only.tgr"], "only.tgr", 1), null);
  assert.equal(adjacentLayout(["only.tgr"], "missing.tgr", 1), null);
});

test("the first key press is immediate with a 450ms hold delay", () => {
  const first = layoutKeyStep(null, "ArrowRight", false, 100);
  assert.equal(first.step, true);
  assert.equal(first.state?.nextStepAt, 550);
  assert.equal(layoutKeyStep(first.state, "ArrowRight", true, 549).step, false);
  assert.equal(layoutKeyStep(first.state, "ArrowRight", true, 550).step, true);
});

test("held keys step at most once per 300ms without accumulating skipped repeats", () => {
  let state = layoutKeyStep(null, "ArrowRight", false, 0).state;
  for (const now of [50, 100, 200, 400]) {
    const result = layoutKeyStep(state, "ArrowRight", true, now);
    assert.equal(result.step, false);
    assert.equal(result.state, state);
  }
  state = layoutKeyStep(state, "ArrowRight", true, 450).state;
  assert.equal(layoutKeyStep(state, "ArrowRight", true, 749).step, false);
  const delayed = layoutKeyStep(state, "ArrowRight", true, 2000);
  assert.equal(delayed.step, true);
  assert.equal(delayed.state?.nextStepAt, 2300);
  assert.equal(layoutKeyStep(delayed.state, "ArrowRight", true, 2001).step, false);
});

test("fresh taps and direction changes stay immediate", () => {
  const first = layoutKeyStep(null, "ArrowRight", false, 0);
  assert.equal(layoutKeyStep(first.state, "ArrowRight", false, 20).step, true);
  const reverse = layoutKeyStep(first.state, "ArrowLeft", false, 30);
  assert.equal(reverse.step, true);
  assert.equal(reverse.state?.key, "ArrowLeft");
  assert.equal(layoutKeyStep(reverse.state, "ArrowRight", true, 1000).step, false);
});

test("a hold started outside the canvas cannot begin stepping on focus", () => {
  assert.deepEqual(layoutKeyStep(null, "ArrowRight", true, 1000), { step: false, state: null });
  assert.equal(layoutKeyStep(null, "ArrowRight", false, 1001).step, true);
});
