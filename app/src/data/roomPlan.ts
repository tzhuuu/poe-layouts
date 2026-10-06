export type RoomTile = {
  x: number;
  y: number;
  kind: string;
  width: number;
  height: number;
  edges: [number, number, number, number];
  ground: [number, number, number, number];
  elevations: [number, number, number, number];
  feature: number;
  origin: number;
};

export type RoomMarker = {
  x: number;
  y: number;
  rotation: number;
  kind: string;
  tag: string;
};

export type RoomObject = { x: number; y: number; art: string; entity: string };

export type RoomPlan = {
  logical_path: string;
  label: string;
  version: number;
  width: number;
  height: number;
  tile_size: number;
  assets: string[];
  tiles: RoomTile[];
  markers: RoomMarker[];
  objects: RoomObject[];
};

export async function loadRoomPlan(path: string, signal: AbortSignal): Promise<RoomPlan> {
  const response = await fetch(`/api/room-plan?${new URLSearchParams({ path })}`, { signal });
  if (!response.headers.get("content-type")?.includes("application/json")) {
    throw new Error("Room API did not return JSON. Restart the layouts server after updating it.");
  }
  if (!response.ok) {
    const error = await response.json() as { error?: string };
    throw new Error(error.error ?? `Could not load room (${response.status})`);
  }
  const plan = await response.json() as RoomPlan;
  if (!Number.isSafeInteger(plan.width) || !Number.isSafeInteger(plan.height)
    || plan.width <= 0 || plan.height <= 0 || plan.width * plan.height > 65_536
    || plan.tile_size !== 24 || !Array.isArray(plan.tiles) || !Array.isArray(plan.assets)
    || !Array.isArray(plan.markers) || !Array.isArray(plan.objects)) {
    throw new Error("Invalid room plan response");
  }
  return plan;
}
