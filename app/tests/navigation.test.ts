import assert from "node:assert/strict";
import { test } from "node:test";
import { navigationUrl, readNavigation, updateNavigation } from "../src/data/navigation.ts";

test("URL navigation defaults missing and invalid tabs safely", () => {
  assert.deepEqual(readNavigation(new URL("https://example.test/?tab=unknown&zone=")), {
    zone: null, tab: "layouts", layout: null, room: null, variant: null,
  });
});

test("all workspace selections survive a URL round trip", () => {
  const navigation = {
    zone: "1_1_3", tab: "rooms" as const,
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

test("zone changes clear stale selections but preserve the active tab", () => {
  const current = readNavigation(new URL("https://example.test/?zone=old&tab=files&layout=old.tgr&room=door&variant=old.arm"));
  assert.deepEqual(updateNavigation(current, { zone: "new" }), {
    zone: "new", tab: "files", layout: null, room: null, variant: null,
  });
  assert.equal(updateNavigation(current, { zone: "new", layout: "new.tgr" }).layout, "new.tgr");
});

test("room changes clear only its variant; tabs and layouts preserve room selection", () => {
  const current = readNavigation(new URL("https://example.test/?zone=old&tab=rooms&layout=old.tgr&room=door&variant=old.arm"));
  assert.equal(updateNavigation(current, { room: "boss" }).variant, null);
  assert.equal(updateNavigation(current, { room: "boss", variant: "boss.arm" }).variant, "boss.arm");
  assert.equal(updateNavigation(current, { tab: "layouts", layout: "new.tgr" }).variant, "old.arm");
});

test("cleared fields are removed instead of serialized as null", () => {
  const source = new URL("https://example.test/?zone=old&room=door&variant=old.arm");
  const next = updateNavigation(readNavigation(source), { room: null });
  const target = new URL(navigationUrl(source, next), source);
  assert.equal(target.searchParams.has("room"), false);
  assert.equal(target.searchParams.has("variant"), false);
});
