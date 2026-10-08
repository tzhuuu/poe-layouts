import assert from "node:assert/strict";
import { test } from "node:test";
import { NODE_ROTATIONS, nodeCanvas, nodeRotationDegrees, rotateNodePoint } from "../src/data/nodeRotation.ts";

test("node rotations use the full rectangular canvas", () => {
  const point = { x: 24, y: 48 };
  const canvas = { width: 120, height: 72 };
  assert.deepEqual(rotateNodePoint(point, canvas, "I"), { x: 24, y: 48 });
  assert.deepEqual(rotateNodePoint(point, canvas, "R90"), { x: 48, y: 96 });
  assert.deepEqual(rotateNodePoint(point, canvas, "R180"), { x: 96, y: 24 });
  assert.deepEqual(rotateNodePoint(point, canvas, "R270"), { x: 24, y: 24 });
  assert.deepEqual(point, { x: 24, y: 48 });
  assert.deepEqual(canvas, { width: 120, height: 72 });
});

test("Tidal Island entrance test rotations preserve source coordinates", () => {
  const canvas = nodeCanvas(36, 48)!;
  const entrance = { x: 499, y: 1094 };
  assert.deepEqual(canvas, { width: 864, height: 1152 });
  assert.deepEqual(rotateNodePoint(entrance, canvas, "R90"), { x: 1094, y: 365 });
  assert.deepEqual(rotateNodePoint(entrance, canvas, "R180"), { x: 365, y: 58 });
  assert.deepEqual(rotateNodePoint(entrance, canvas, "R270"), { x: 58, y: 499 });
  assert.deepEqual(rotateNodePoint(entrance, canvas, "R90", "clockwise"), { x: 58, y: 499 });
  assert.deepEqual(rotateNodePoint(entrance, canvas, "R270", "clockwise"), { x: 1094, y: 365 });
  assert.deepEqual(entrance, { x: 499, y: 1094 });
});

test("missing or invalid Size cannot transform a node position", () => {
  for (const [width, height] of [[null, 1], [1, null], [0, 1], [-1, 1], [NaN, 1], [1, Infinity], [Number.MAX_VALUE, 1]]) {
    assert.equal(nodeCanvas(width, height), null);
  }
});

test("orientation markers distinguish identity and quarter turns", () => {
  assert.deepEqual(NODE_ROTATIONS.map((rotation) => nodeRotationDegrees(rotation)), [0, -90, -180, -270]);
  assert.deepEqual(NODE_ROTATIONS.map((rotation) => nodeRotationDegrees(rotation, "clockwise")), [0, 90, 180, 270]);
});

test("opposite conventions exchange R90 and R270, and quarter turns invert", () => {
  const point = { x: 24, y: 48 };
  const canvas = { width: 120, height: 72 };
  const swapped = { width: canvas.height, height: canvas.width };
  assert.deepEqual(rotateNodePoint(point, canvas, "R90", "clockwise"), rotateNodePoint(point, canvas, "R270", "counterclockwise"));
  for (const direction of ["clockwise", "counterclockwise"] as const) {
    const rotated = rotateNodePoint(point, canvas, "R90", direction);
    assert.deepEqual(rotateNodePoint(rotated, swapped, "R270", direction), point);
  }
});
