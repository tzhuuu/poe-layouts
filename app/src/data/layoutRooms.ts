import { parseRoomCatalog, type RawRoomCatalog, type RoomCatalog } from "./roomVariants";

export type LayoutRoom = {
  label: string;
  nodeCount: number;
  layouts: { logicalPath: string; nodeIndices: number[] }[];
};

export type LayoutRooms = {
  requestedLayoutCount: number;
  parsedLayoutCount: number;
  rooms: LayoutRoom[];
  warnings: string[];
  roomCatalogs: RoomCatalog[];
};

type RawLayoutRooms = {
  requested_layout_count: number;
  parsed_layout_count: number;
  rooms: {
    label: string;
    node_count: number;
    layouts: { logical_path: string; node_indices: number[] }[];
  }[];
  warnings: string[];
  room_catalogs?: RawRoomCatalog[];
};

export async function loadLayoutRooms(paths: string[], signal: AbortSignal): Promise<LayoutRooms> {
  const response = await fetch("/api/layout-rooms", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ paths }),
    signal,
  });
  if (!response.ok) {
    throw new Error(`Could not load room list (${response.status})`);
  }
  if (!response.headers.get("content-type")?.includes("application/json")) {
    throw new Error("Could not load room list: the API did not return JSON");
  }
  const raw = await response.json() as RawLayoutRooms;
  return {
    requestedLayoutCount: raw.requested_layout_count,
    parsedLayoutCount: raw.parsed_layout_count,
    warnings: raw.warnings,
    roomCatalogs: (raw.room_catalogs ?? []).map(parseRoomCatalog),
    rooms: raw.rooms.map((room) => ({
      label: room.label,
      nodeCount: room.node_count,
      layouts: room.layouts.map((layout) => ({
        logicalPath: layout.logical_path,
        nodeIndices: layout.node_indices,
      })),
    })),
  };
}
