import type { RoomPlan } from "./roomPlan";

export type RoomItemKind = "marker" | "object" | "tile";
export type RoomInspection = {
  path: string;
  kind: RoomItemKind;
  index: number;
  label: string;
  position: { x: number; y: number };
  fields: { label: string; value: string }[];
};

export function roomItemType(plan: RoomPlan, kind: RoomItemKind, index: number): string | null {
  if (!Number.isSafeInteger(index) || index < 0) return null;
  const normalize = (value: string) => value.replace(/\\/g, "/").toLowerCase();
  if (kind === "object") {
    const object = plan.objects[index];
    return object && (object.art || object.entity) ? JSON.stringify([kind, normalize(object.art), normalize(object.entity)]) : null;
  }
  if (kind === "marker") {
    const marker = plan.markers[index];
    return marker ? JSON.stringify([kind, normalize(marker.kind), normalize(marker.tag)]) : null;
  }
  const tile = plan.tiles[index];
  return tile ? JSON.stringify([kind, tile.kind, tile.width, tile.height, tile.origin, tile.ground, tile.edges, tile.elevations, tile.feature]) : null;
}

export function inspectRoomItem(plan: RoomPlan, kind: RoomItemKind, index: number): RoomInspection | null {
  if (!Number.isSafeInteger(index) || index < 0) return null;
  const source = { path: plan.logical_path, kind, index };
  const asset = (id: number) => id > 0 ? plan.assets[id - 1] ?? `Unknown asset ${id}` : "Default / none";
  if (kind === "marker") {
    const marker = plan.markers[index];
    return marker ? {
      ...source, label: marker.tag || marker.kind, position: { x: marker.x, y: marker.y },
      fields: [
        { label: "Kind", value: marker.kind },
        { label: "Tag", value: marker.tag || "None" },
        { label: "Position", value: `${marker.x}, ${marker.y}` },
        { label: "Rotation", value: String(marker.rotation) },
      ],
    } : null;
  }
  if (kind === "object") {
    const object = plan.objects[index];
    return object ? {
      ...source, label: object.entity || object.art || `Object ${index}`, position: { x: object.x, y: object.y },
      fields: [
        { label: "Position", value: `${object.x}, ${object.y}` },
        { label: "Entity", value: object.entity || "None" },
        { label: "Art", value: object.art || "None" },
      ],
    } : null;
  }
  const tile = plan.tiles[index];
  return tile ? {
    ...source, label: `Tile ${tile.x}, ${tile.y}`, position: { x: tile.x * plan.tile_size, y: tile.y * plan.tile_size },
    fields: [
      { label: "Kind", value: tile.kind },
      { label: "Grid position", value: `${tile.x}, ${tile.y}` },
      { label: "Position", value: `${tile.x * plan.tile_size}, ${tile.y * plan.tile_size}` },
      { label: "Key size", value: `${tile.width} x ${tile.height}` },
      { label: "Origin", value: String(tile.origin) },
      { label: "Elevations", value: tile.elevations.join(", ") },
      { label: "Ground indices", value: tile.ground.join(", ") },
      { label: "Ground assets", value: tile.ground.map(asset).join("\n") },
      { label: "Edge indices", value: tile.edges.join(", ") },
      { label: "Edge assets", value: tile.edges.map(asset).join("\n") },
      { label: "Feature", value: `${tile.feature}: ${asset(tile.feature)}` },
    ],
  } : null;
}
