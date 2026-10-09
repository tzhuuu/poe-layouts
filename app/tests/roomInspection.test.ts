import assert from "node:assert/strict";
import { test } from "node:test";
import { inspectRoomItem, roomItemType } from "../src/data/roomInspection.ts";
import type { RoomPlan } from "../src/data/roomPlan.ts";

const plan: RoomPlan = {
  logical_path: "metadata/test.arm", label: "test", version: 1,
  width: 3, height: 2, tile_size: 24, assets: ["ground.tdt", "wall.tdt"],
  markers: [{ kind: "spawn", tag: "mapboss", x: 36, y: 12, rotation: 90 }],
  objects: [{ art: "tree.ao", entity: "tree.ot", x: 48, y: 24 }],
  tiles: [{ x: 2, y: 1, kind: "k", width: 3, height: 2, origin: 4,
    ground: [1, 0, 1, 1], edges: [2, 0, 0, 9], elevations: [1, 2, 3, 4], feature: 0 }],
};

test("room marker inspection preserves tags, source positions and rotation", () => {
  const inspection = inspectRoomItem(plan, "marker", 0)!;
  assert.equal(inspection.path, plan.logical_path);
  assert.equal(inspection.label, "mapboss");
  assert.deepEqual(inspection.position, { x: 36, y: 12 });
  assert.deepEqual(inspection.fields, [
    { label: "Kind", value: "spawn" }, { label: "Tag", value: "mapboss" },
    { label: "Position", value: "36, 12" }, { label: "Rotation", value: "90" },
  ]);
  assert.equal(inspectRoomItem({ ...plan, markers: [{ ...plan.markers[0], tag: "" }] }, "marker", 0)?.label, "spawn");
});

test("room object inspection includes both art and entity paths", () => {
  assert.deepEqual(inspectRoomItem(plan, "object", 0)?.position, { x: 48, y: 24 });
  assert.deepEqual(inspectRoomItem(plan, "object", 0)?.fields, [
    { label: "Position", value: "48, 24" },
    { label: "Entity", value: "tree.ot" }, { label: "Art", value: "tree.ao" },
  ]);
});

test("room tile inspection resolves one-based asset references without losing raw indices", () => {
  const inspection = inspectRoomItem(plan, "tile", 0)!;
  const fields = Object.fromEntries(inspection.fields.map(({ label, value }) => [label, value]));
  assert.equal(inspection.label, "Tile 2, 1");
  assert.deepEqual(inspection.position, { x: 48, y: 24 });
  assert.equal(fields.Position, "48, 24");
  assert.equal(fields["Key size"], "3 x 2");
  assert.equal(fields.Origin, "4");
  assert.equal(fields.Elevations, "1, 2, 3, 4");
  assert.equal(fields["Ground indices"], "1, 0, 1, 1");
  assert.equal(fields["Ground assets"], "ground.tdt\nDefault / none\nground.tdt\nground.tdt");
  assert.equal(fields["Edge assets"], "wall.tdt\nDefault / none\nDefault / none\nUnknown asset 9");
});

test("invalid room item indices do not produce inspection metadata", () => {
  for (const kind of ["marker", "object", "tile"] as const) {
    for (const index of [-1, 1, 0.5, NaN]) assert.equal(inspectRoomItem(plan, kind, index), null);
  }
});

test("object peers match art and entity, ignoring position and path casing", () => {
  const objects = [
    { art: "Metadata/Tree.ao", entity: "Doodad", x: 0, y: 0 },
    { art: "metadata\\tree.ao", entity: "doodad", x: 20, y: 30 },
    { art: "rock.ao", entity: "Doodad", x: 0, y: 0 },
    { art: "Metadata/Tree.ao", entity: "Other", x: 0, y: 0 },
    { art: "", entity: "", x: 0, y: 0 },
  ];
  const source = { ...plan, objects };
  assert.equal(roomItemType(source, "object", 0), roomItemType(source, "object", 1));
  assert.notEqual(roomItemType(source, "object", 0), roomItemType(source, "object", 2));
  assert.notEqual(roomItemType(source, "object", 0), roomItemType(source, "object", 3));
  assert.equal(roomItemType(source, "object", 4), null);
});

test("marker peers match kind and tag, not source rotation or position", () => {
  const source = { ...plan, markers: [
    plan.markers[0], { ...plan.markers[0], tag: "MAPBOSS", rotation: 180, x: 12 },
    { ...plan.markers[0], tag: "pack" }, { ...plan.markers[0], kind: "entrance" },
  ] };
  assert.equal(roomItemType(source, "marker", 0), roomItemType(source, "marker", 1));
  assert.notEqual(roomItemType(source, "marker", 0), roomItemType(source, "marker", 2));
  assert.notEqual(roomItemType(source, "marker", 0), roomItemType(source, "marker", 3));
});

test("tile peers ignore coordinates but retain key metadata distinctions", () => {
  const source = { ...plan, tiles: [
    plan.tiles[0], { ...plan.tiles[0], x: 5, y: 6 }, { ...plan.tiles[0], feature: 2 },
  ] };
  assert.equal(roomItemType(source, "tile", 0), roomItemType(source, "tile", 1));
  assert.notEqual(roomItemType(source, "tile", 0), roomItemType(source, "tile", 2));
  assert.equal(roomItemType(source, "tile", -1), null);
});
