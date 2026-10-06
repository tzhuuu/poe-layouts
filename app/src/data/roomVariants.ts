export type RoomVariant = {
  logicalPath: string;
  label: string;
  version: number;
  width: number;
  height: number;
  assetCount: number;
  bossTags: string[];
};

export type RoomCatalog = {
  roomSet: string | null;
  variants: RoomVariant[];
  warnings: string[];
};

export type RawRoomCatalog = {
  room_set: string | null;
  variants: {
    logical_path: string;
    label: string;
    version: number;
    width: number;
    height: number;
    asset_count: number;
    boss_tags?: string[];
  }[];
  warnings: string[];
};

export async function loadRoomVariants(layout: string, signal: AbortSignal): Promise<RoomCatalog> {
  const response = await fetch(`/api/room-variants?${new URLSearchParams({ layout })}`, { signal });
  if (!response.ok) throw new Error(`Could not load room variants (${response.status})`);
  if (!response.headers.get("content-type")?.includes("application/json")) {
    throw new Error("Could not load room variants: the API did not return JSON");
  }
  const raw = await response.json() as RawRoomCatalog;
  return parseRoomCatalog(raw);
}

export function parseRoomCatalog(raw: RawRoomCatalog): RoomCatalog {
  return {
    roomSet: raw.room_set,
    warnings: raw.warnings,
    variants: raw.variants.map((variant) => ({
      logicalPath: variant.logical_path,
      label: variant.label,
      version: variant.version,
      width: variant.width,
      height: variant.height,
      assetCount: variant.asset_count,
      bossTags: variant.boss_tags ?? [],
    })),
  };
}
