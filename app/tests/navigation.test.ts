import assert from "node:assert/strict";
import { test } from "node:test";
import { navigationUrl, readNavigation, updateNavigation } from "../src/data/navigation.ts";

test("URL navigation defaults missing and invalid tabs safely", () => {
  assert.deepEqual(readNavigation(new URL("https://example.test/?tab=unknown&zone=")), {
    zone: null, tab: "view", view: "layout", layout: null, room: null, variant: null,
  });
});

test("all workspace selections survive a URL round trip", () => {
  const navigation = {
    zone: "1_1_3", tab: "view" as const, view: "room" as const,
    layout: "metadata/terrain/graphs/island1_2.tgr",
    room: "door & entrance", variant: "metadata/terrain/rooms/room 2.arm",
  };
  const source = new URL("https://example.test/explorer?debug=1#preview");
  const target = new URL(navigationUrl(source, navigation), source);
  assert.deepEqual(readNavigation(target), navigation);
  assert.equal(target.searchParams.get("debug"), "1");
  assert.equal(target.pathname, "/explorer");
  assert.equal(target.hash, "#preview");
});

test("legacy layout and room tabs select the corresponding unified view", () => {
  for (const [tab, view] of [["layouts", "layout"], ["rooms", "room"]]) {
    const navigation = readNavigation(new URL(`https://example.test/?tab=${tab}`));
    assert.equal(navigation.tab, "view");
    assert.equal(navigation.view, view);
  }
  assert.equal(readNavigation(new URL("https://example.test/?tab=view&view=unknown")).view, "layout");
});

test("sidebar selections open View and preserve the other renderer's selection", () => {
  const current = readNavigation(new URL("https://example.test/?tab=files&view=room&layout=old.tgr&room=door&variant=door.arm"));
  const layout = updateNavigation(current, { tab: "view", view: "layout", layout: "next.tgr" });
  assert.equal(layout.tab, "view");
  assert.equal(layout.view, "layout");
  assert.equal(layout.room, "door");
  assert.equal(layout.variant, "door.arm");
  const room = updateNavigation(layout, { tab: "view", view: "room", room: "boss" });
  assert.equal(room.layout, "next.tgr");
  assert.equal(room.variant, null);
  const files = updateNavigation(room, { tab: "files" });
  assert.equal(files.view, "room");
  assert.deepEqual(updateNavigation(files, { tab: "view" }), room);
});

test("changing zones defaults the viewer back to a layout", () => {
  const current = readNavigation(new URL("https://example.test/?zone=old&tab=view&view=room&room=door"));
  assert.equal(updateNavigation(current, { zone: "new" }).view, "layout");
  assert.equal(updateNavigation(current, { zone: "new", view: "room", room: "boss" }).view, "room");
});

test("zone changes clear stale selections but preserve the active tab", () => {
  const current = readNavigation(new URL("https://example.test/?zone=old&tab=files&layout=old.tgr&room=door&variant=old.arm"));
  assert.deepEqual(updateNavigation(current, { zone: "new" }), {
    zone: "new", tab: "files", view: "layout", layout: null, room: null, variant: null,
  });
  assert.equal(updateNavigation(current, { zone: "new", layout: "new.tgr" }).layout, "new.tgr");
});

test("room changes clear only its variant; tabs and layouts preserve room selection", () => {
  const current = readNavigation(new URL("https://example.test/?zone=old&tab=rooms&layout=old.tgr&room=door&variant=old.arm"));
  assert.equal(updateNavigation(current, { room: "boss" }).variant, null);
  assert.equal(updateNavigation(current, { room: "boss", variant: "boss.arm" }).variant, "boss.arm");
  assert.equal(updateNavigation(current, { tab: "view", view: "layout", layout: "new.tgr" }).variant, "old.arm");
});

test("cleared fields are removed instead of serialized as null", () => {
  const source = new URL("https://example.test/?zone=old&room=door&variant=old.arm");
  const next = updateNavigation(readNavigation(source), { room: null });
  const target = new URL(navigationUrl(source, next), source);
  assert.equal(target.searchParams.has("room"), false);
  assert.equal(target.searchParams.has("variant"), false);
});
