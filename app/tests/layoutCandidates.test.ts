import assert from "node:assert/strict";
import { test } from "node:test";
import { groupLayoutCandidates, selectLayoutCandidate, type LayoutCandidate } from "../src/data/layoutCandidates.ts";

const candidates: LayoutCandidate[] = [
  { logicalPath: "prison1.dgr", group: ["Prison"], rootPaths: ["wrapper.dgr"] },
  { logicalPath: "prison2.dgr", group: ["Prison"], rootPaths: ["wrapper.dgr"] },
  { logicalPath: "boss.dgr", group: ["Warden"], rootPaths: ["wrapper.dgr"] },
];

test("child URLs are retained while old wrapper URLs select a reachable child", () => {
  assert.equal(selectLayoutCandidate(candidates, "boss.dgr")?.logicalPath, "boss.dgr");
  assert.equal(selectLayoutCandidate(candidates, "wrapper.dgr")?.logicalPath, "prison1.dgr");
  assert.equal(selectLayoutCandidate(candidates, "unrelated.dgr")?.logicalPath, "prison1.dgr");
  assert.equal(selectLayoutCandidate([], "wrapper.dgr"), null);
});

test("picker groups preserve candidate order and section names", () => {
  const groups = groupLayoutCandidates(candidates);
  assert.deepEqual(groups.map((group) => [group.label, group.candidates.length]), [["Prison", 2], ["Warden", 1]]);
  assert.deepEqual(groups[0].candidates.map((candidate) => candidate.logicalPath), ["prison1.dgr", "prison2.dgr"]);
});

test("old wrapper selection uses its own child, not another topology's default", () => {
  const other = { logicalPath: "other.dgr", group: [], rootPaths: ["other-root.dgr"] };
  assert.equal(selectLayoutCandidate([other, ...candidates], "wrapper.dgr")?.logicalPath, "prison1.dgr");
});

test("nested groups and ordinary graphs remain distinct", () => {
  const groups = groupLayoutCandidates([
    { logicalPath: "ordinary.tgr", group: [], rootPaths: ["ordinary.tgr"] },
    { logicalPath: "floor.dgr", group: ["Level1", "Inner"], rootPaths: ["pyramid.dgr"] },
  ]);
  assert.deepEqual(groups.map((group) => group.label), ["", "Level1 / Inner"]);
});

test("flattening grouped candidates gives arrow navigation the same order as the picker", () => {
  const interleaved = [candidates[0], candidates[2], candidates[1]];
  assert.deepEqual(groupLayoutCandidates(interleaved).flatMap((group) => group.candidates).map((candidate) => candidate.logicalPath),
    ["prison1.dgr", "prison2.dgr", "boss.dgr"]);
});
