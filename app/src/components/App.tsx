import { useCallback, useEffect, useMemo, useState } from "react";
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
import {
  clearCache,
  latestPatchVersions,
  prefetchBundles,
  scrapeCampaignActsOneToFive,
  type CacheClearReport,
  type CacheManifest,
  type ScrapeCampaignSummary,
} from "../data/pipelineApi";
import { PixiLayoutPreview } from "../render/PixiLayoutPreview";

type LoadState =
  | { status: "loading" }
  | { status: "ready"; data: LayoutData }
  | { status: "error"; message: string };

type AppTab = "explorer" | "scrape";

export function App() {
  const [loadState, setLoadState] = useState<LoadState>({ status: "loading" });
  const [activeTab, setActiveTab] = useState<AppTab>("explorer");
  const [query, setQuery] = useState("");
  const [selectedZoneId, setSelectedZoneId] = useState<string | null>(null);

  const reloadLayoutData = useCallback(async () => {
    const data = await loadLayoutData();
    setLoadState({ status: "ready", data });
    setSelectedZoneId((currentZoneId) =>
      currentZoneId && data.zones.some((zone) => zone.id === currentZoneId)
        ? currentZoneId
        : data.zones[0]?.id ?? null,
    );
    return data;
  }, []);

  useEffect(() => {
    reloadLayoutData().catch((error: unknown) => {
      setLoadState({
        status: "error",
        message: error instanceof Error ? error.message : String(error),
      });
    });
  }, [reloadLayoutData]);

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
      activeTab={activeTab}
      query={query}
      selectedZoneId={selectedZoneId}
      onTabChange={setActiveTab}
      onQueryChange={setQuery}
      onReloadLayoutData={reloadLayoutData}
      onSelectZone={setSelectedZoneId}
    />
  );
}

type LayoutExplorerProps = {
  data: LayoutData;
  activeTab: AppTab;
  query: string;
  selectedZoneId: string | null;
  onTabChange: (tab: AppTab) => void;
  onQueryChange: (query: string) => void;
  onReloadLayoutData: () => Promise<LayoutData>;
  onSelectZone: (zoneId: string) => void;
};

function LayoutExplorer({
  data,
  activeTab,
  query,
  selectedZoneId,
  onTabChange,
  onQueryChange,
  onReloadLayoutData,
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

        <nav className="paneTabs" aria-label="Sidebar views">
          <button
            className={activeTab === "explorer" ? "active" : ""}
            onClick={() => onTabChange("explorer")}
            type="button"
          >
            Explorer
          </button>
          <button
            className={activeTab === "scrape" ? "active" : ""}
            onClick={() => onTabChange("scrape")}
            type="button"
          >
            Scrape
          </button>
        </nav>

        {activeTab === "explorer" ? (
          <ExplorerPane
            filteredZones={filteredZones}
            query={query}
            selectedZone={selectedZone}
            onQueryChange={onQueryChange}
            onSelectZone={onSelectZone}
          />
        ) : (
          <ScrapePane
            data={data}
            stats={stats}
            onReloadLayoutData={onReloadLayoutData}
          />
        )}
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

function ExplorerPane({
  filteredZones,
  query,
  selectedZone,
  onQueryChange,
  onSelectZone,
}: {
  filteredZones: ZoneSummary[];
  query: string;
  selectedZone: ZoneSummary | null;
  onQueryChange: (query: string) => void;
  onSelectZone: (zoneId: string) => void;
}) {
  return (
    <section className="tabPane explorerTab">
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
    </section>
  );
}

function ScrapePane({
  data,
  onReloadLayoutData,
  stats,
}: {
  data: LayoutData;
  onReloadLayoutData: () => Promise<LayoutData>;
  stats: ReturnType<typeof terrainStats>;
}) {
  return (
    <section className="tabPane scrapeTab">
      <section className="scrapeSummary" aria-label="Current scrape artifact">
        <div>
          <span>Scope</span>
          <strong>{data.scope}</strong>
        </div>
        <div>
          <span>Patch</span>
          <strong>{data.gameVersion}</strong>
        </div>
        <div>
          <span>Zones</span>
          <strong>{data.zones.length}</strong>
        </div>
        <div>
          <span>Missing</span>
          <strong>{stats.missing}</strong>
        </div>
      </section>

      <PipelineControls
        defaultPatchVersion={data.gameVersion}
        onReloadLayoutData={onReloadLayoutData}
      />
    </section>
  );
}

type PipelineActionState =
  | { status: "idle" }
  | { status: "running"; label: string }
  | { status: "ok"; label: string; detail: string }
  | { status: "error"; label: string; detail: string };

function PipelineControls({
  defaultPatchVersion,
  onReloadLayoutData,
}: {
  defaultPatchVersion: string;
  onReloadLayoutData: () => Promise<LayoutData>;
}) {
  const [patchVersion, setPatchVersion] = useState(defaultPatchVersion);
  const [releaseLine, setReleaseLine] = useState("");
  const [bundlesText, setBundlesText] = useState(
    "_.index.bin\nTiny_11.bundle.bin\nTiny_51.bundle.bin",
  );
  const [refresh, setRefresh] = useState(false);
  const [offline, setOffline] = useState(false);
  const [actionState, setActionState] = useState<PipelineActionState>({
    status: "idle",
  });

  const runAction = async <T,>(
    label: string,
    action: () => Promise<T>,
    summarize: (result: T) => string,
  ) => {
    setActionState({ status: "running", label });
    try {
      const result = await action();
      setActionState({ status: "ok", label, detail: summarize(result) });
    } catch (error: unknown) {
      setActionState({
        status: "error",
        label,
        detail: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const bundles = () =>
    bundlesText
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean);

  return (
    <section className="pipelinePane" aria-label="Pipeline controls">
      <header>
        <span>Pipeline</span>
        <div className="pipelineToggles">
          <label>
            <input
              checked={refresh}
              onChange={(event) => setRefresh(event.target.checked)}
              type="checkbox"
            />
            Refresh
          </label>
          <label>
            <input
              checked={offline}
              onChange={(event) => setOffline(event.target.checked)}
              type="checkbox"
            />
            Cache only
          </label>
        </div>
      </header>

      <div className="pipelineGrid">
        <label>
          <span>Patch</span>
          <input
            autoComplete="off"
            onChange={(event) => setPatchVersion(event.target.value)}
            placeholder="latest"
            value={patchVersion}
          />
        </label>
        <label>
          <span>Release</span>
          <input
            autoComplete="off"
            onChange={(event) => setReleaseLine(event.target.value)}
            placeholder="latest line"
            value={releaseLine}
          />
        </label>
      </div>

      <label className="bundleList">
        <span>Bundles</span>
        <textarea
          onChange={(event) => setBundlesText(event.target.value)}
          spellCheck={false}
          value={bundlesText}
        />
      </label>

      <div className="pipelineButtons">
        <button
          onClick={() =>
            runAction(
              "Scrape",
              async () => {
                const result = await scrapeCampaignActsOneToFive({
                  patchVersion,
                  refresh,
                  offline,
                });
                await onReloadLayoutData();
                return result;
              },
              summarizeScrape,
            )
          }
          type="button"
        >
          Scrape
        </button>
        <button
          onClick={() =>
            runAction("Latest", latestPatchVersions, (result) => {
              setPatchVersion(result.poe);
              return `PoE1 ${result.poe}`;
            })
          }
          type="button"
        >
          Latest
        </button>
        <button
          onClick={() =>
            runAction(
              "Fetch",
              () =>
                prefetchBundles({
                  patchVersion,
                  bundles: bundles(),
                  refresh,
                }),
              summarizeManifest,
            )
          }
          type="button"
        >
          Fetch
        </button>
        <button
          onClick={() =>
            runAction(
              "Preview",
              () =>
                clearCache({
                  releaseLine,
                  patchVersion,
                  dryRun: true,
                }),
              summarizeClearReport,
            )
          }
          type="button"
        >
          Preview
        </button>
        <button
          className="dangerButton"
          onClick={() =>
            runAction(
              "Clear",
              () =>
                clearCache({
                  releaseLine,
                  patchVersion,
                  dryRun: false,
                }),
              summarizeClearReport,
            )
          }
          type="button"
        >
          Clear
        </button>
      </div>

      {actionState.status !== "idle" && (
        <p className={`pipelineStatus ${actionState.status}`}>
          <strong>{actionState.label}</strong>
          <span>
            {actionState.status === "running" ? "Running" : actionState.detail}
          </span>
        </p>
      )}
    </section>
  );
}

function summarizeScrape(summary: ScrapeCampaignSummary) {
  return `${summary.patchVersion}: ${summary.selectedAreas} zones, ${summary.missingFiles} missing`;
}

function summarizeManifest(manifest: CacheManifest) {
  const byteCount = manifest.entries.reduce(
    (total, entry) => total + entry.byte_len,
    0,
  );
  return `${manifest.namespace}: ${manifest.entries.length} bundles, ${formatBytes(byteCount)}`;
}

function summarizeClearReport(report: CacheClearReport) {
  const size = formatBytes(report.byte_len);
  const action = report.removed ? "removed" : report.existed ? "found" : "missing";
  return `${action} ${report.file_count} files, ${size}`;
}

function formatBytes(bytes: number) {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KiB`;
  }
  return `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
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
            <span className="terrainPath">
              <span>{file.logicalPath}</span>
              {file.reason && <small>{file.reason}</small>}
            </span>
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
