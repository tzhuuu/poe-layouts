import assert from "node:assert/strict";
import { test } from "node:test";
import { graphOrientationDegrees, graphProjection, projectGraphPoint } from "../src/data/graphProjection.ts";

test("outdoor projection places inland terraces northwest of the shoreline", () => {
  const shoreline = { x: 300, y: 100 };
  const inland = { x: 300, y: 700 };
  const projection = graphProjection(true);
  const coast = projectGraphPoint(shoreline, projection);
  const terrace = projectGraphPoint(inland, projection);
  assert.ok(terrace.x < coast.x);
  assert.ok(terrace.y < coast.y);
  assert.deepEqual(inland, { x: 300, y: 700 });
});

test("Coast entrance deltas agree with northeast and south reference variants", () => {
  const projection = graphProjection(true);
  const northeast = projectGraphPoint({ x: 1461 - 150, y: 103 - 81 }, projection);
  assert.ok(northeast.x > 0 && northeast.y < 0);
  const south = projectGraphPoint({ x: 100 - 1140, y: 109 - 1342 }, projection);
  assert.ok(south.y > 0 && south.y > Math.abs(south.x) * 5);
});

test("grid matrix and node projection use the same scaled basis", () => {
  for (const outdoor of [false, true]) {
    const unit = projectGraphPoint({ x: 24, y: 48 }, graphProjection(outdoor));
    const scaled = projectGraphPoint({ x: 24, y: 48 }, graphProjection(outdoor, 3));
    assert.ok(Math.abs(scaled.x - unit.x * 3) < 1e-10);
    assert.ok(Math.abs(scaled.y - unit.y * 3) < 1e-10);
  }
});

test("indoor projection retains its existing handedness pending reference validation", () => {
  const point = projectGraphPoint({ x: 0, y: 24 }, graphProjection(false));
  assert.ok(point.x > 0 && point.y > 0);
});

test("rotation markers follow the reflected outdoor coordinate basis", () => {
  for (const outdoor of [false, true]) {
    for (const sourceDegrees of [0, -90, -180, -270, 90, 180, 270]) {
      const radians = sourceDegrees * Math.PI / 180;
      const vector = projectGraphPoint({ x: Math.sin(radians), y: -Math.cos(radians) }, graphProjection(outdoor));
      const markerRadians = graphOrientationDegrees(sourceDegrees, outdoor) * Math.PI / 180;
      assert.ok(Math.abs(vector.x - Math.sin(markerRadians)) < 1e-10);
      assert.ok(Math.abs(vector.y + Math.cos(markerRadians)) < 1e-10);
    }
  }
});
