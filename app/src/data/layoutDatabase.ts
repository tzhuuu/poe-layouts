import * as flatbuffers from "flatbuffers";
import { LayoutDatabase } from "../generated/poe-layouts/layout-database";
import { TerrainFileKind } from "../generated/poe-layouts/terrain-file-kind";
import { TerrainFileStatus } from "../generated/poe-layouts/terrain-file-status";

export type ZoneSummary = {
  rowIndex: number;
  id: string;
  name: string;
  act: number;
  areaLevel: number;
  isTown: boolean;
  topologyIndices: number[];
  tsiFile: string | null;
};

export type TerrainFileSummary = {
  logicalPath: string;
  source: string;
  kind: TerrainFileKind;
  status: TerrainFileStatus;
  reason: string | null;
};

export type LayoutData = {
  schemaVersion: string;
  gameVersion: string;
  releaseLine: string;
  scope: string;
  zones: ZoneSummary[];
  terrainFiles: TerrainFileSummary[];
  sourceFileCount: number;
  warnings: string[];
};

export async function loadLayoutData(): Promise<LayoutData> {
  const response = await fetch("/data/layouts.bin", { cache: "no-store" });
  if (!response.ok) {
    throw new Error(`Could not load layouts.bin (${response.status})`);
  }
  const bytes = new Uint8Array(await response.arrayBuffer());
  const database = LayoutDatabase.getRootAsLayoutDatabase(
    new flatbuffers.ByteBuffer(bytes),
  );

  return {
    schemaVersion: database.schemaVersion() ?? "unknown",
    gameVersion: database.gameVersion() ?? "unknown",
    releaseLine: database.releaseLine() ?? "unknown",
    scope: database.scope() ?? "unknown",
    zones: readZones(database),
    terrainFiles: readTerrainFiles(database),
    sourceFileCount: database.sourceFilesLength(),
    warnings: readWarnings(database),
  };
}

export function terrainFilesForZone(
  data: LayoutData,
  zone: ZoneSummary,
): TerrainFileSummary[] {
  const sourcePrefix = `WorldAreas[${zone.rowIndex}]`;
  return data.terrainFiles.filter((file) => file.source.includes(sourcePrefix));
}

export function terrainKindLabel(kind: TerrainFileKind): string {
  switch (kind) {
    case TerrainFileKind.Graph:
      return "Graph";
    case TerrainFileKind.Tsi:
      return "TSI";
    case TerrainFileKind.DgrVariant:
      return "DGR";
    case TerrainFileKind.ArmVariant:
      return "ARM";
    default:
      return "Unknown";
  }
}

export function terrainStatusLabel(status: TerrainFileStatus): string {
  switch (status) {
    case TerrainFileStatus.Extracted:
      return "Extracted";
    case TerrainFileStatus.Missing:
      return "Missing";
    default:
      return "Candidate";
  }
}

function readZones(database: LayoutDatabase): ZoneSummary[] {
  const zones: ZoneSummary[] = [];
  for (let index = 0; index < database.zonesLength(); index += 1) {
    const zone = database.zones(index);
    if (!zone) {
      continue;
    }
    const topologyIndices: number[] = [];
    for (
      let topologyIndex = 0;
      topologyIndex < zone.topologyIndicesLength();
      topologyIndex += 1
    ) {
      topologyIndices.push(zone.topologyIndices(topologyIndex) ?? 0);
    }
    zones.push({
      rowIndex: zone.rowIndex(),
      id: zone.id() ?? "",
      name: zone.name() ?? "",
      act: zone.act(),
      areaLevel: zone.areaLevel(),
      isTown: zone.isTown(),
      topologyIndices,
      tsiFile: zone.tsiFile(),
    });
  }
  return zones;
}

function readTerrainFiles(database: LayoutDatabase): TerrainFileSummary[] {
  const terrainFiles: TerrainFileSummary[] = [];
  for (let index = 0; index < database.terrainFilesLength(); index += 1) {
    const terrainFile = database.terrainFiles(index);
    if (!terrainFile) {
      continue;
    }
    terrainFiles.push({
      logicalPath: terrainFile.logicalPath() ?? "",
      source: terrainFile.source() ?? "",
      kind: terrainFile.kind(),
      status: terrainFile.status(),
      reason: terrainFile.reason(),
    });
  }
  return terrainFiles;
}

function readWarnings(database: LayoutDatabase): string[] {
  const warnings: string[] = [];
  for (let index = 0; index < database.warningsLength(); index += 1) {
    const warning = database.warnings(index);
    if (typeof warning === "string") {
      warnings.push(warning);
    }
  }
  return warnings;
}
