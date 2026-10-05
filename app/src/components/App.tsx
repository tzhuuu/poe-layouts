import { useEffect, useMemo, useState } from "react";
import { TerrainFileStatus } from "../generated/poe-layouts/terrain-file-status";
import {
  loadLayoutData,
  terrainFilesForZone,
  terrainKindLabel,
  terrainStatusLabel,
  type LayoutData,
  type TerrainFileSummary,
  type ZoneSummary,
} from "../data/layoutDatabase";
import { PixiLayoutPreview } from "../render/PixiLayoutPreview";

type LoadState =
  | { status: "loading" }
  | { status: "ready"; data: LayoutData }
  | { status: "error"; message: string };

export function App() {
  const [loadState, setLoadState] = useState<LoadState>({ status: "loading" });
  const [query, setQuery] = useState("");
  const [selectedZoneId, setSelectedZoneId] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    loadLayoutData()
      .then((data) => {
        if (cancelled) {
          return;
        }
        setLoadState({ status: "ready", data });
        setSelectedZoneId(data.zones[0]?.id ?? null);
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setLoadState({
            status: "error",
            message: error instanceof Error ? error.message : String(error),
          });
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (loadState.status === "loading") {
    return <main className="appShell loadingShell">Loading layout data</main>;
  }

  if (loadState.status === "error") {
    return (
      <main className="appShell loadingShell">
        <section className="emptyState">
          <h1>layouts.bin unavailable</h1>
          <p>{loadState.message}</p>
        </section>
      </main>
    );
  }

  return (
    <LayoutExplorer
      data={loadState.data}
      query={query}
      selectedZoneId={selectedZoneId}
      onQueryChange={setQuery}
      onSelectZone={setSelectedZoneId}
    />
  );
}

type LayoutExplorerProps = {
  data: LayoutData;
  query: string;
  selectedZoneId: string | null;
  onQueryChange: (query: string) => void;
  onSelectZone: (zoneId: string) => void;
};

function LayoutExplorer({
  data,
  query,
  selectedZoneId,
  onQueryChange,
  onSelectZone,
}: LayoutExplorerProps) {
  const filteredZones = useMemo(
    () => filterZones(data.zones, query),
    [data.zones, query],
  );
  const selectedZone =
    filteredZones.find((zone) => zone.id === selectedZoneId) ??
    filteredZones[0] ??
    data.zones[0] ??
    null;
  const selectedTerrain = selectedZone
    ? terrainFilesForZone(data, selectedZone)
    : [];
  const stats = useMemo(() => terrainStats(data.terrainFiles), [data.terrainFiles]);

  return (
    <main className="appShell">
      <aside className="zonePane">
        <header className="paneHeader">
          <div>
            <p className="eyebrow">PoE1 {data.scope}</p>
            <h1>Layout Explorer</h1>
          </div>
          <span className="versionBadge">{data.gameVersion}</span>
        </header>

        <label className="searchBox">
          <span>Search</span>
          <input
            autoComplete="off"
            onChange={(event) => onQueryChange(event.target.value)}
            placeholder="Zone, act, id"
            value={query}
          />
        </label>

        <div className="actFilters" aria-label="Acts">
          {[1, 2, 3, 4, 5].map((act) => (
            <span key={act}>Act {act}</span>
          ))}
        </div>

        <section className="zoneList" aria-label="Zones">
          {filteredZones.map((zone) => (
            <button
              className={zone.id === selectedZone?.id ? "zoneRow active" : "zoneRow"}
              key={zone.id}
              onClick={() => onSelectZone(zone.id)}
              type="button"
            >
              <span>
                <strong>{zone.name || zone.id}</strong>
                <small>{zone.id}</small>
              </span>
              <em>Act {zone.act}</em>
            </button>
          ))}
        </section>
      </aside>

      <section className="workspace">
        <header className="workspaceHeader">
          <div>
            <p className="eyebrow">{data.releaseLine}</p>
            <h2>{selectedZone?.name ?? "No zone selected"}</h2>
          </div>
          <div className="metricStrip">
            <Metric label="Zones" value={data.zones.length} />
            <Metric label="Candidates" value={data.terrainFiles.length} />
            <Metric label="Extracted" value={stats.extracted} />
            <Metric label="Missing" value={stats.missing} />
          </div>
        </header>

        <section className="visualPane">
          <PixiLayoutPreview zone={selectedZone} terrainFiles={selectedTerrain} />
          <ZoneDetails zone={selectedZone} terrainFiles={selectedTerrain} />
        </section>

        <TerrainTable terrainFiles={selectedTerrain} />
      </section>
    </main>
  );
}

function Metric({ label, value }: { label: string; value: number | string }) {
  return (
    <div className="metric">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function ZoneDetails({
  zone,
  terrainFiles,
}: {
  zone: ZoneSummary | null;
  terrainFiles: TerrainFileSummary[];
}) {
  if (!zone) {
    return <aside className="detailPane">No zone selected</aside>;
  }
  const extracted = terrainFiles.filter(
    (file) => file.status === TerrainFileStatus.Extracted,
  ).length;
  const missing = terrainFiles.filter(
    (file) => file.status === TerrainFileStatus.Missing,
  ).length;

  return (
    <aside className="detailPane">
      <dl>
        <div>
          <dt>ID</dt>
          <dd>{zone.id}</dd>
        </div>
        <div>
          <dt>Area Level</dt>
          <dd>{zone.areaLevel}</dd>
        </div>
        <div>
          <dt>Topology Rows</dt>
          <dd>{zone.topologyIndices.length}</dd>
        </div>
        <div>
          <dt>Type</dt>
          <dd>{zone.isTown ? "Town" : "Area"}</dd>
        </div>
        <div>
          <dt>Extracted</dt>
          <dd>{extracted}</dd>
        </div>
        <div>
          <dt>Missing</dt>
          <dd>{missing}</dd>
        </div>
      </dl>
    </aside>
  );
}

function TerrainTable({ terrainFiles }: { terrainFiles: TerrainFileSummary[] }) {
  const rows = terrainFiles.slice(0, 80);
  return (
    <section className="terrainPane">
      <header>
        <h3>Layout Candidates</h3>
        <span>{terrainFiles.length}</span>
      </header>
      <div className="terrainTable">
        {rows.map((file) => (
          <article key={`${file.kind}:${file.logicalPath}:${file.source}`}>
            <strong>{terrainKindLabel(file.kind)}</strong>
            <span>{file.logicalPath}</span>
            <em className={file.status === TerrainFileStatus.Missing ? "warn" : ""}>
              {terrainStatusLabel(file.status)}
            </em>
          </article>
        ))}
      </div>
    </section>
  );
}

function filterZones(zones: ZoneSummary[], query: string): ZoneSummary[] {
  const normalized = query.trim().toLowerCase();
  if (!normalized) {
    return zones;
  }
  return zones.filter((zone) => {
    const haystack = `${zone.name} ${zone.id} act ${zone.act}`.toLowerCase();
    return haystack.includes(normalized);
  });
}

function terrainStats(terrainFiles: TerrainFileSummary[]) {
  return terrainFiles.reduce(
    (stats, file) => {
      if (file.status === TerrainFileStatus.Extracted) {
        stats.extracted += 1;
      } else if (file.status === TerrainFileStatus.Missing) {
        stats.missing += 1;
      } else {
        stats.candidates += 1;
      }
      return stats;
    },
    { candidates: 0, extracted: 0, missing: 0 },
  );
}
