import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  ArrowLeft,
  ArrowRight,
  Bug,
  ChevronDown,
  ChevronRight,
  CheckCircle2,
  Database,
  File,
  Folder,
  FolderTree,
  GitBranch,
  Grid2X2,
  Home,
  Layers,
  CircleHelp,
  TreePine,
  Info,
  Map as MapIcon,
  Search,
  TableProperties,
  Maximize2,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import { TerrainFileStatus } from "../generated/poe-layouts/terrain-file-status";
import { TerrainFileKind } from "../generated/poe-layouts/terrain-file-kind";
import { nodeBossDetails, nodeBossLabel, nodeHasBoss, nodeRole, nodeRoles } from "../data/nodeHighlights";
import { RoomList, RoomPicker, useRoomWorkspace } from "./RoomList";
import { WarningBadge } from "./WarningBadge";
import { NodeRotationPicker } from "./NodeRotationPicker";
import { GraphWorkspaceProvider, useGraphWorkspace } from "./GraphWorkspace";
import { GraphNodeDetails } from "./GraphNodeDetails";
import { ExplorerNavigationProvider, useExplorerNavigation } from "./ExplorerNavigation";
import { isWorkspaceTab } from "../data/navigation";
import { adjacentLayout, layoutKeyStep, type LayoutKeyRepeat } from "../data/layoutNavigation";
import { nodeCanvas, nodeRotationDegrees, rotateNodePoint, type NodeRotation, type RotationDirection } from "../data/nodeRotation";
import { loadLayoutEnvironments, type LayoutEnvironment } from "../data/layoutEnvironment";
import {
  loadLayoutGraph,
  nodeLabelLines,
  nodeTransitionDetails,
  type LayoutGraph,
  type LayoutGraphNode,
} from "../data/layoutGraph";
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
  loadRawFiles,
  loadRawFileText,
  type RawFilesResponse,
  type RawFileEntry,
  type RawFolderEntry,
} from "../data/rawFiles";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "./ui/popover";
import { ScrollArea } from "./ui/scroll-area";
import { Separator } from "./ui/separator";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "./ui/tabs";

type LoadState =
  | { status: "loading" }
  | { status: "ready"; data: LayoutData }
  | { status: "error"; message: string };

export function App() {
  return <ExplorerNavigationProvider><ExplorerApp /></ExplorerNavigationProvider>;
}

function ExplorerApp() {
  const { navigation, navigate } = useExplorerNavigation();
  const [loadState, setLoadState] = useState<LoadState>({ status: "loading" });
  const [query, setQuery] = useState("");

  const reloadLayoutData = useCallback(async () => {
    const data = await loadLayoutData();
    setLoadState({ status: "ready", data });
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

  useEffect(() => {
    if (loadState.status !== "ready") return;
    const zone = loadState.data.zones.find((candidate) => candidate.id === navigation.zone)
      ?? loadState.data.zones[0];
    if (navigation.zone !== (zone?.id ?? null)) navigate({ zone: zone?.id ?? null }, "replace");
  }, [loadState, navigation.zone, navigate]);

  if (loadState.status === "loading") {
    return (
      <main className="grid h-screen min-h-[680px] place-items-center bg-background p-6 text-sm text-muted-foreground">
        Loading layout data
      </main>
    );
  }

  if (loadState.status === "error") {
    return (
      <main className="grid h-screen min-h-[680px] place-items-center bg-background p-6">
        <section className="max-w-lg rounded-md border bg-card p-5 shadow-sm">
          <div className="flex items-center gap-2 text-destructive">
            <AlertTriangle className="size-4" />
            <h1 className="text-base font-semibold">layouts.bin unavailable</h1>
          </div>
          <p className="mt-2 text-sm leading-6 text-muted-foreground">
            {loadState.message}
          </p>
        </section>
      </main>
    );
  }

  return (
    <LayoutExplorer
      data={loadState.data}
      query={query}
      selectedZoneId={navigation.zone}
      onQueryChange={setQuery}
      onSelectZone={(zone) => navigate({ zone })}
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
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const filteredZones = useMemo(
    () => filterZones(data.zones, query),
    [data.zones, query],
  );
  const selectedZone =
    data.zones.find((zone) => zone.id === selectedZoneId) ??
    data.zones[0] ??
    null;
  const selectedTerrain = useMemo(
    () => selectedZone ? terrainFilesForZone(data, selectedZone) : [],
    [data, selectedZone],
  );
  const layoutCountsByZoneId = useMemo(
    () => layoutCountsByZone(data),
    [data],
  );
  const stats = useMemo(() => terrainStats(data.terrainFiles), [data.terrainFiles]);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key.toLowerCase() !== "k" || (!event.metaKey && !event.ctrlKey)) {
        return;
      }
      event.preventDefault();
      searchInputRef.current?.focus();
      searchInputRef.current?.select();
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  return (
    <main className="grid h-screen min-h-[680px] min-w-[720px] grid-cols-[240px_minmax(0,1fr)] overflow-hidden bg-background text-foreground xl:grid-cols-[340px_minmax(0,1fr)]">
      <aside className="grid min-h-0 grid-rows-[auto_minmax(0,1fr)] gap-4 border-r bg-muted/30 p-4">
        <header className="space-y-3">
          <div className="flex items-start justify-between gap-3">
            <div>
              <p className="text-xs font-semibold uppercase text-muted-foreground">
                PoE1 {data.scope}
              </p>
              <h1 className="mt-1 text-xl font-semibold tracking-tight">
                Layout Explorer
              </h1>
            </div>
            <Badge variant="outline">{data.gameVersion}</Badge>
          </div>

          <DebugStats stats={stats} zoneCount={data.zones.length} />
        </header>

        <ExplorerPane
          filteredZones={filteredZones}
          layoutCountsByZoneId={layoutCountsByZoneId}
          query={query}
          searchInputRef={searchInputRef}
          selectedZone={selectedZone}
          onQueryChange={onQueryChange}
          onSelectZone={onSelectZone}
        />
      </aside>

      <WorkspaceTabs key={selectedZone?.id} gameVersion={data.gameVersion} selectedTerrain={selectedTerrain} selectedZone={selectedZone} />
    </main>
  );
}

function ExplorerPane({
  filteredZones,
  layoutCountsByZoneId,
  query,
  searchInputRef,
  selectedZone,
  onQueryChange,
  onSelectZone,
}: {
  filteredZones: ZoneSummary[];
  layoutCountsByZoneId: Map<string, number>;
  query: string;
  searchInputRef: React.RefObject<HTMLInputElement | null>;
  selectedZone: ZoneSummary | null;
  onQueryChange: (query: string) => void;
  onSelectZone: (zoneId: string) => void;
}) {
  return (
    <section className="grid min-h-0 grid-rows-[auto_minmax(0,1fr)] gap-3">
      <label className="space-y-1.5">
        <span className="flex items-center justify-between gap-3 text-xs font-medium text-muted-foreground">
          Search
          <kbd className="rounded border bg-background px-1.5 py-0.5 font-mono text-[10px] font-medium text-muted-foreground">
            {navigator.platform.toLowerCase().includes("mac") ? "Cmd" : "Ctrl"} K
          </kbd>
        </span>
        <span className="relative block">
          <Search className="absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            autoComplete="off"
            className="pl-8"
            onChange={(event) => onQueryChange(event.target.value)}
            placeholder="Zone, act, id"
            ref={searchInputRef}
            value={query}
          />
        </span>
      </label>

      <ScrollArea className="min-h-0 rounded-md border bg-card">
        <div className="space-y-1 p-1.5">
          {filteredZones.map((zone) => (
            <button
              className={[
                "grid w-full grid-cols-[minmax(0,1fr)_auto] items-center gap-2 rounded-md px-2.5 py-2 text-left text-sm outline-none transition-colors",
                "hover:bg-accent hover:text-accent-foreground focus-visible:ring-2 focus-visible:ring-ring",
                zone.id === selectedZone?.id
                  ? "bg-accent text-accent-foreground shadow-[inset_3px_0_0_hsl(var(--primary))]"
                  : "text-foreground",
              ].join(" ")}
              key={zone.id}
              onClick={() => onSelectZone(zone.id)}
              type="button"
            >
              <span className="min-w-0">
                <strong className="flex min-w-0 items-center gap-1.5 font-medium">
                  <span className="min-w-0 truncate">{zone.name || zone.id}</span>
                  {zone.isTown && <span className="shrink-0 text-muted-foreground" title="Town"><Home aria-hidden="true" className="size-3.5" /></span>}
                </strong>
                <small className="block truncate text-xs text-muted-foreground">
                  {zone.id} ({layoutCountLabel(layoutCountsByZoneId.get(zone.id) ?? 0)})
                </small>
              </span>
              <Badge variant="secondary">Act {zone.act}</Badge>
            </button>
          ))}
        </div>
      </ScrollArea>
    </section>
  );
}

function WorkspaceTabs({
  gameVersion,
  selectedTerrain,
  selectedZone,
}: {
  gameVersion: string;
  selectedTerrain: TerrainFileSummary[];
  selectedZone: ZoneSummary | null;
}) {
  const { navigation, navigate } = useExplorerNavigation();
  const [environments, setEnvironments] = useState<EnvironmentLoadState>({ status: "loading" });
  const activeTab = navigation.tab;
  useEffect(() => {
    let cancelled = false;
    setEnvironments({ status: "loading" });
    loadLayoutEnvironments()
      .then((index) => {
        if (index.patchVersion !== gameVersion) {
          throw new Error(`Classifications are for ${index.patchVersion}; layouts are for ${gameVersion}`);
        }
        if (!cancelled) setEnvironments({
          status: "ready",
          byPath: new Map(index.layouts.map((layout) => [layout.logicalPath, layout])),
          warnings: index.warnings,
        });
      })
      .catch((error: unknown) => {
        if (!cancelled) setEnvironments({ status: "error", message: error instanceof Error ? error.message : String(error) });
      });
    return () => { cancelled = true; };
  }, [gameVersion]);

  const graphCandidates = useMemo(
    () =>
      selectedTerrain.filter(
        (file) =>
          file.kind === TerrainFileKind.Graph &&
          file.status === TerrainFileStatus.Extracted,
      ),
    [selectedTerrain],
  );
  const selectedLayoutPath = graphCandidates.find((file) => file.logicalPath === navigation.layout)?.logicalPath
    ?? graphCandidates[0]?.logicalPath ?? null;
  const setSelectedLayoutPath = (layout: string) => navigate({ layout });
  const roomLayoutPaths = useMemo(
    () => [...new Set(graphCandidates.map((file) => file.logicalPath))],
    [graphCandidates],
  );
  const previousLayoutPath = adjacentLayout(roomLayoutPaths, selectedLayoutPath, -1);
  const nextLayoutPath = adjacentLayout(roomLayoutPaths, selectedLayoutPath, 1);
  const canvasRegionRef = useRef<HTMLElement | null>(null);
  const restoreCanvasFocus = useRef(false);
  const layoutKeyRepeatRef = useRef<LayoutKeyRepeat | null>(null);
  useEffect(() => {
    const reset = () => { layoutKeyRepeatRef.current = null; };
    const release = (event: KeyboardEvent) => {
      if (event.key === layoutKeyRepeatRef.current?.key) reset();
    };
    const hide = () => { if (document.hidden) reset(); };
    window.addEventListener("keyup", release);
    window.addEventListener("blur", reset);
    document.addEventListener("visibilitychange", hide);
    return () => {
      window.removeEventListener("keyup", release);
      window.removeEventListener("blur", reset);
      document.removeEventListener("visibilitychange", hide);
    };
  }, []);
  useEffect(() => {
    if (restoreCanvasFocus.current) {
      restoreCanvasFocus.current = false;
      canvasRegionRef.current?.focus({ preventScroll: true });
    }
  }, [selectedLayoutPath]);
  const roomWorkspace = useRoomWorkspace(roomLayoutPaths, selectedLayoutPath);
  useEffect(() => {
    if (navigation.zone === selectedZone?.id && navigation.layout !== selectedLayoutPath) {
      navigate({ layout: selectedLayoutPath }, "replace");
    }
  }, [navigation.zone, navigation.layout, selectedZone?.id, selectedLayoutPath, navigate]);

  return (
    <Tabs
      className="grid min-h-0 min-w-0 grid-rows-[auto_minmax(0,1fr)] gap-3 p-4"
      value={activeTab}
      onValueChange={(tab) => { if (isWorkspaceTab(tab)) navigate({ tab }); }}
    >
      <WorkspaceHeader
        selectedTerrain={selectedTerrain}
        selectedZone={selectedZone}
        environments={environments}
        fallbackLayoutPath={graphCandidates[0]?.logicalPath ?? null}
        picker={activeTab === "rooms" ? (
          <RoomPicker key={selectedZone?.id} workspace={roomWorkspace} />
        ) : (
          <LayoutPicker
            key={selectedZone?.id}
            onSelectLayout={setSelectedLayoutPath}
            previousLayoutPath={previousLayoutPath}
            nextLayoutPath={nextLayoutPath}
            selectedLayoutPath={selectedLayoutPath}
            terrainFiles={selectedTerrain.filter((file) => file.kind === TerrainFileKind.Graph)}
          />
        )}
      />

      <TabsContent
        className="grid min-h-0 min-w-0 overflow-hidden data-[state=inactive]:hidden"
        value="layouts"
      >
        <GraphWorkspaceProvider key={`${selectedZone?.id}:${selectedLayoutPath}`}>
        <section className="grid min-h-0 grid-cols-[minmax(0,1fr)_240px] gap-3 xl:grid-cols-[minmax(0,1fr)_320px]">
          <section
            aria-label="Layout canvas"
            className="relative min-h-0 overflow-hidden rounded-md border border-foreground/20 bg-[#101716] shadow-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
            ref={canvasRegionRef}
            tabIndex={0}
            onPointerDownCapture={(event) => {
              const target = event.target as Element;
              if (event.button === 0 && target.closest('svg[role="group"]') && !target.closest("[data-node-index]")) {
                event.currentTarget.focus({ preventScroll: true });
              }
            }}
            onKeyDown={(event) => {
              if (event.target !== event.currentTarget || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
              if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
              event.preventDefault();
              const repeat = layoutKeyStep(layoutKeyRepeatRef.current, event.key, event.repeat, performance.now());
              layoutKeyRepeatRef.current = repeat.state;
              if (!repeat.step) return;
              const path = event.key === "ArrowLeft" ? previousLayoutPath : nextLayoutPath;
              if (path) {
                restoreCanvasFocus.current = true;
                setSelectedLayoutPath(path);
              }
            }}
          >
            <LayoutGraphPreview
              logicalPath={selectedLayoutPath}
              outdoor={environments.status === "ready" && environments.byPath.get(selectedLayoutPath ?? "")?.environment === "outdoor"}
              zoneId={selectedZone?.id}
            />
          </section>

          <ZoneInspector zone={selectedZone} />
        </section>
        </GraphWorkspaceProvider>

      </TabsContent>

      <TabsContent
        className="min-h-0 min-w-0 overflow-hidden data-[state=inactive]:hidden"
        forceMount
        value="rooms"
      >
        <RoomList workspace={roomWorkspace} />
      </TabsContent>

      <TabsContent
        className="min-h-0 rounded-md border bg-card shadow-sm"
        value="files"
      >
        <FileBrowser terrainFiles={selectedTerrain} zone={selectedZone} />
      </TabsContent>
    </Tabs>
  );
}

function WorkspaceHeader({
  selectedTerrain,
  selectedZone,
  environments,
  fallbackLayoutPath,
  picker,
}: {
  selectedTerrain: TerrainFileSummary[];
  selectedZone: ZoneSummary | null;
  environments: EnvironmentLoadState;
  fallbackLayoutPath: string | null;
  picker: React.ReactNode;
}) {
  const missing = selectedTerrain.filter(
    (file) => file.status === TerrainFileStatus.Missing,
  ).map((file) => `${file.logicalPath}: ${file.reason ?? "File was not extracted"}`);
  const zoneTsiPath = selectedZone?.tsiFile?.replace(/\\/g, "/").toLowerCase();
  const environment = environments.status === "ready"
    ? environments.byPath.get(zoneTsiPath ?? "") ?? environments.byPath.get(fallbackLayoutPath ?? "")
    : undefined;
  const unavailableReason = environments.status === "error"
    ? environments.message
    : environments.status === "loading"
      ? "Loading environment classification"
      : "No classification for this zone";

  return (
    <header className="grid gap-3">
      <div className="flex min-h-10 items-start justify-between gap-4">
        <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1">
          <h2 className="max-w-full break-words text-2xl font-semibold tracking-tight">
            {selectedZone?.name ?? "No zone selected"}
          </h2>
          {selectedZone && <EnvironmentBadge layout={environment} unavailableReason={unavailableReason} />}
        </div>
        {environments.status === "ready" && <WarningBadge label="Environment warnings" warnings={environments.warnings}>{environments.warnings.length} environment warnings</WarningBadge>}
        <WarningBadge label="Missing terrain files" warnings={missing}>{missing.length} missing</WarningBadge>
      </div>

      <div className="flex min-w-0 flex-wrap items-center gap-3">
        <TabsList className="w-fit shrink-0">
          <TabsTrigger value="layouts">
            <TableProperties />
            Layouts
          </TabsTrigger>
          <TabsTrigger value="rooms">
            <Layers />
            Rooms
          </TabsTrigger>
          <TabsTrigger value="files">
            <FolderTree />
            Files
          </TabsTrigger>
        </TabsList>
        <div className="min-w-0 border-l pl-3">{picker}</div>
      </div>
    </header>
  );
}

function DebugStats({
  stats,
  zoneCount,
}: {
  stats: ReturnType<typeof terrainStats>;
  zoneCount: number;
}) {
  const [open, setOpen] = useState(false);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          aria-label="Dataset debug"
          className="text-muted-foreground"
          onBlur={() => setOpen(false)}
          onFocus={() => setOpen(true)}
          onPointerEnter={() => setOpen(true)}
          onPointerLeave={() => setOpen(false)}
          size="icon"
          type="button"
          variant="outline"
        >
          <Bug />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="w-64"
        onPointerEnter={() => setOpen(true)}
        onPointerLeave={() => setOpen(false)}
      >
        <div className="space-y-3">
          <div>
            <h2 className="text-sm font-semibold">Loaded Dataset</h2>
            <p className="text-xs text-muted-foreground">
              Counts from the current FlatBuffers artifact.
            </p>
          </div>
          <dl className="grid grid-cols-2 gap-2 text-sm">
            <DebugStat icon={<MapIcon />} label="Zones" value={zoneCount} />
            <DebugStat icon={<Database />} label="Candidates" value={stats.total} />
            <DebugStat icon={<CheckCircle2 />} label="Extracted" value={stats.extracted} />
            <DebugStat icon={<AlertTriangle />} label="Missing" value={stats.missing} />
          </dl>
        </div>
      </PopoverContent>
    </Popover>
  );
}

function DebugStat({
  icon,
  label,
  value,
}: {
  icon: React.ReactNode;
  label: string;
  value: number | string;
}) {
  return (
    <div className="rounded-md border bg-muted/30 p-2">
      <dt className="flex items-center gap-1.5 text-xs text-muted-foreground [&_svg]:size-3.5">
        {icon}
        {label}
      </dt>
      <dd className="mt-1 font-semibold">{value}</dd>
    </div>
  );
}

function LayoutGraphPreview({ logicalPath, outdoor, zoneId }: { logicalPath: string | null; outdoor: boolean; zoneId?: string }) {
  const { dispatch } = useGraphWorkspace();
  const [loadState, setLoadState] = useState<
    | { status: "idle" }
    | { status: "loading" }
    | { status: "ready"; graph: LayoutGraph }
    | { status: "error"; message: string }
  >({ status: "idle" });

  useEffect(() => {
    if (!logicalPath) {
      setLoadState({ status: "idle" });
      return;
    }
    let cancelled = false;
    setLoadState({ status: "loading" });
    loadLayoutGraph(logicalPath, zoneId)
      .then((graph) => {
        if (!cancelled) {
          dispatch({ type: "load", graph });
          setLoadState({ status: "ready", graph });
        }
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
  }, [logicalPath, zoneId, dispatch]);

  if (!logicalPath) {
    return (
      <div className="grid h-full place-items-center p-6 text-sm text-white/70">
        No extracted DGR layout graph is available for this zone.
      </div>
    );
  }

  if (loadState.status === "loading") {
    return (
      <div className="grid h-full place-items-center p-6 text-sm text-white/70">
        Loading DGR graph...
      </div>
    );
  }

  if (loadState.status === "error") {
    return (
      <div className="grid h-full place-items-center p-6">
        <div className="max-w-md rounded-md border border-amber-300/40 bg-amber-950/50 p-3 text-sm text-amber-100">
          {loadState.message}
        </div>
      </div>
    );
  }

  if (loadState.status !== "ready") {
    return null;
  }

  return <LayoutGraphSvg graph={loadState.graph} outdoor={outdoor} />;
}

function LayoutGraphSvg({ graph, outdoor }: { graph: LayoutGraph; outdoor: boolean }) {
  const { state, dispatch } = useGraphWorkspace();
  const gridId = useId();
  const [gridVisible, setGridVisible] = useState(true);
  const [pickerNode, setPickerNode] = useState<number | null>(null);
  const nodeRotations = state.rotations;
  const canvas = nodeCanvas(graph.width, graph.height);
  const view = useMemo(() => layoutGraphView(graph, nodeRotations, state.direction), [graph, nodeRotations, state.direction]);
  const fitCamera = useMemo(() => cameraFromView(view), [view]);
  const [camera, setCamera] = useState<GraphCamera>(fitCamera);
  const svgRef = useRef<SVGSVGElement | null>(null);
  const dragRef = useRef<GraphDrag | null>(null);

  useEffect(() => {
    setCamera(fitCamera);
    dragRef.current = null;
  }, [graph, fitCamera.minX, fitCamera.minY, fitCamera.width, fitCamera.height]);

  useEffect(() => {
    setGridVisible(true);
  }, [graph]);

  const zoomGraph = useCallback(
    (factor: number, anchor?: GraphPoint) => {
      setCamera((current) =>
        zoomCamera(
          current,
          anchor ?? cameraCenter(current),
          factor,
          fitCamera.width / 20,
          fitCamera.width * 8,
        ),
      );
    },
    [fitCamera.width],
  );

  const handlePointerDown = useCallback(
    (event: React.PointerEvent<SVGSVGElement>) => {
      if (event.button !== 0) {
        return;
      }
      event.currentTarget.setPointerCapture(event.pointerId);
      dragRef.current = {
        camera,
        pointerId: event.pointerId,
        startX: event.clientX,
        startY: event.clientY,
        nodeIndex: (event.target as Element).closest("[data-node-index]")?.getAttribute("data-node-index") ?? null,
        moved: false,
      };
    },
    [camera],
  );

  const handlePointerMove = useCallback((event: React.PointerEvent<SVGSVGElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId || !svgRef.current) {
      return;
    }
    if (Math.hypot(event.clientX - drag.startX, event.clientY - drag.startY) > 4) drag.moved = true;
    if (!drag.moved) return;
    const delta = graphDeltaFromClientDelta(
      svgRef.current,
      drag.camera,
      event.clientX - drag.startX,
      event.clientY - drag.startY,
    );
    setCamera({
      ...drag.camera,
      minX: drag.camera.minX - delta.x,
      minY: drag.camera.minY - delta.y,
    });
  }, []);

  const handlePointerEnd = useCallback((event: React.PointerEvent<SVGSVGElement>) => {
    if (dragRef.current?.pointerId === event.pointerId) {
      const drag = dragRef.current;
      if (event.type === "pointerup" && !drag.moved && drag.nodeIndex !== null) {
        const index = Number(drag.nodeIndex);
        dispatch({ type: "select", index });
        setPickerNode(index);
      }
      dragRef.current = null;
    }
  }, [dispatch]);

  const handleWheel = useCallback(
    (event: React.WheelEvent<SVGSVGElement>) => {
      if (!svgRef.current) {
        return;
      }
      event.preventDefault();
      const anchor = graphPointFromClient(svgRef.current, camera, event.clientX, event.clientY);
      zoomGraph(event.deltaY > 0 ? 1.18 : 1 / 1.18, anchor);
    },
    [camera, zoomGraph],
  );

  return (
    <div className="grid h-full min-h-0 min-w-0 grid-rows-[auto_minmax(0,1fr)_auto]">
      <header className="flex min-w-0 flex-wrap items-center justify-between gap-2 border-b border-white/10 bg-black/25 px-3 py-2 text-white">
        <div className="min-w-0 flex-1 basis-48">
          <h3 className="truncate font-mono text-sm font-semibold">
            {baseName(graph.logicalPath)}
          </h3>
          <p className="truncate font-mono text-xs text-white/60">
            {graph.masterFile ?? graph.logicalPath}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <div className="flex gap-1">
            {outdoor && (
              <Button
                aria-label="24-unit grid"
                aria-pressed={gridVisible}
                className={`border-white/10 text-white hover:bg-white/20 hover:text-white ${gridVisible ? "bg-white/20" : "bg-white/5"}`}
                onClick={() => setGridVisible((visible) => !visible)}
                size="icon"
                title="24-unit grid"
                type="button"
                variant="outline"
              >
                <Grid2X2 />
              </Button>
            )}
            <Button
              aria-label="Zoom out"
              className="border-white/10 bg-white/10 text-white hover:bg-white/20 hover:text-white"
              onClick={() => zoomGraph(1.25)}
              size="icon"
              title="Zoom out"
              type="button"
              variant="outline"
            >
              <ZoomOut />
            </Button>
            <Button
              aria-label="Zoom in"
              className="border-white/10 bg-white/10 text-white hover:bg-white/20 hover:text-white"
              onClick={() => zoomGraph(0.8)}
              size="icon"
              title="Zoom in"
              type="button"
              variant="outline"
            >
              <ZoomIn />
            </Button>
            <Button
              aria-label="Fit graph"
              className="border-white/10 bg-white/10 text-white hover:bg-white/20 hover:text-white"
              onClick={() => setCamera(fitCamera)}
              size="icon"
              title="Fit graph"
              type="button"
              variant="outline"
            >
              <Maximize2 />
            </Button>
          </div>
          <Badge variant="secondary">{graph.nodes.length} nodes</Badge>
          <Badge variant="secondary">{graph.edges.length} edges</Badge>
          <WarningBadge label="Layout warnings" warnings={graph.warnings} />
        </div>
      </header>

      <div className="min-h-0 min-w-0 overflow-hidden">
        <svg
          aria-label={`Layout graph ${baseName(graph.logicalPath)}`}
          className="block h-full w-full cursor-grab select-none active:cursor-grabbing"
          onPointerCancel={handlePointerEnd}
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerLeave={() => dispatch({ type: "hover", index: null })}
          onPointerUp={handlePointerEnd}
          onWheel={handleWheel}
          preserveAspectRatio="xMidYMid meet"
          ref={svgRef}
          role="group"
          style={{ touchAction: "none" }}
          viewBox={`${camera.minX} ${camera.minY} ${camera.width} ${camera.height}`}
        >
          {outdoor && gridVisible && (
            <defs>
              <pattern
                height={24}
                id={gridId}
                patternTransform={view.coordinateTransform}
                patternUnits="userSpaceOnUse"
                width={24}
              >
                <path d="M 24 0 H 0 V 24" fill="none" stroke="#b2c4bf" strokeOpacity={0.25} strokeWidth={0.5} />
              </pattern>
            </defs>
          )}
          <rect
            fill="#101716"
            height={view.height}
            width={view.width}
            x={view.minX}
            y={view.minY}
          />
          {outdoor && gridVisible && (
            <rect
              aria-hidden="true"
              data-grid-size={24}
              fill={`url(#${gridId})`}
              height={view.height}
              pointerEvents="none"
              width={view.width}
              x={view.minX}
              y={view.minY}
            />
          )}
          {view.edges.map((edge) => {
            const from = view.nodesByIndex.get(edge.from);
            const to = view.nodesByIndex.get(edge.to);
            if (!from || !to) {
              return null;
            }
            return (
              <line
                key={edge.index}
                stroke={edge.edgeTile?.toLowerCase().includes("void") ? "#5a6261" : "#8fb3a8"}
                strokeOpacity={0.82}
                strokeWidth={view.visuals.edgeWidth}
                x1={from.scaledX}
                x2={to.scaledX}
                y1={from.scaledY}
                y2={to.scaledY}
              >
                <title>
                  {`${edge.from} -> ${edge.to}${edge.edgeTile ? ` ${edge.edgeTile}` : ""}`}
                </title>
              </line>
            );
          })}
          {view.nodes.map((node) => {
            const role = nodeRole(node);
            const color = nodeRoles[role].color;
            const labelLines = nodeLabelLines(node);
            const bossPosition = nodeHasBoss(node) ? bossMarkerPosition(node, view.visuals) : null;
            const rotation = nodeRotations[node.index];
            const point = canvas && rotation ? rotateNodePoint(node, canvas, rotation, state.direction) : node;
            const direction = state.direction === "clockwise" ? "CW" : "CCW";
            return (
            <NodeRotationPicker
              key={node.index}
              node={node}
              canvas={canvas}
              rotation={rotation}
              direction={state.direction}
              open={pickerNode === node.index}
              onOpenChange={(open) => {
                if (open) dispatch({ type: "select", index: node.index });
                setPickerNode((current) => open ? node.index : current === node.index ? null : current);
              }}
              onRotationChange={(rotation) => dispatch({ type: "rotate", index: node.index, rotation })}
              onDirectionChange={(direction) => dispatch({ type: "direction", direction })}
            >
            <g
              aria-label={`Node ${node.index}${node.label ? ` ${node.label}` : ""}`}
              className="cursor-pointer outline-none focus-visible:[&>circle]:stroke-white"
              data-node-index={node.index}
              data-test-rotation={rotation}
              data-room-label={node.label ?? ""}
              data-node-role={role}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  dispatch({ type: "select", index: node.index });
                  setPickerNode(node.index);
                }
              }}
              onPointerEnter={() => {
                if (!dragRef.current) dispatch({ type: "hover", index: node.index });
              }}
              onPointerLeave={() => dispatch({ type: "leave", index: node.index })}
              onFocus={() => dispatch({ type: "hover", index: node.index })}
              onBlur={() => dispatch({ type: "leave", index: node.index })}
              role="button"
              tabIndex={0}
            >
              {bossPosition && (
                <g data-boss-for={node.index}>
                  <line
                    x1={node.scaledX}
                    y1={node.scaledY}
                    x2={bossPosition.x}
                    y2={bossPosition.y}
                    stroke={nodeRoles.boss.color}
                    strokeOpacity={0.65}
                    strokeWidth={view.visuals.ringStrokeWidth}
                  />
                  <circle
                    cx={bossPosition.x}
                    cy={bossPosition.y}
                    r={view.visuals.nodeRadius * 1.25}
                    fill={nodeRoles.boss.color}
                    stroke="#101716"
                    strokeWidth={view.visuals.nodeStrokeWidth}
                  />
                  <text
                    x={bossPosition.x}
                    y={bossPosition.y - view.visuals.nodeRingRadius}
                    textAnchor="middle"
                    fontSize={view.visuals.labelFontSize * 1.25}
                    fontWeight="600"
                    fill={nodeRoles.boss.color}
                  >
                    {nodeBossLabel(node)}
                  </text>
                  <title>{nodeBossDetails(node)}</title>
                </g>
              )}
              <circle
                cx={node.scaledX}
                cy={node.scaledY}
                fill={color}
                r={node.label ? view.visuals.labeledNodeRadius : view.visuals.nodeRadius}
                stroke="#101716"
                strokeWidth={view.visuals.nodeStrokeWidth}
              />
              <circle
                cx={node.scaledX}
                cy={node.scaledY}
                fill="none"
                r={view.visuals.nodeRingRadius}
                stroke={color}
                strokeOpacity="0.78"
                strokeDasharray={role === "void" ? "4 4" : undefined}
                strokeWidth={view.visuals.ringStrokeWidth}
              />
              {rotation && <line
                aria-hidden="true"
                data-orientation-marker="true"
                pointerEvents="none"
                x1={node.scaledX}
                y1={node.scaledY}
                x2={node.scaledX}
                y2={node.scaledY - view.visuals.nodeRingRadius}
                stroke="#ffffff"
                strokeWidth={view.visuals.ringStrokeWidth}
                transform={`rotate(${nodeRotationDegrees(rotation, state.direction) - 45} ${node.scaledX} ${node.scaledY})`}
              />}
              <text
                fill={color}
                fontSize={view.visuals.indexFontSize}
                fontWeight="700"
                textAnchor="middle"
                x={node.scaledX}
                y={node.scaledY - view.visuals.indexOffset}
              >
                {node.index}
              </text>
              {labelLines.length > 0 && (
                <text
                  fill="#f7f8f4"
                  fontSize={view.visuals.labelFontSize}
                  fontWeight="600"
                  textAnchor="middle"
                  x={node.scaledX}
                  y={node.scaledY + view.visuals.labelOffset}
                >
                  {labelLines.map((label, index) => <tspan key={label} x={node.scaledX} dy={index === 0 ? 0 : view.visuals.labelFontSize * 1.25}>{truncateLabel(label, view.visuals.labelMaxLength)}</tspan>)}
                </text>
              )}
              <title>
                node {node.index}
                {node.label ? ` ${node.label}` : ""}
                {`\n${nodeRoles[role].label}\nSource rotation: ${node.rotation ?? "any"}\nRaw position: ${node.x}, ${node.y}\nCurrent position: ${point.x}, ${point.y}\nMetadata: ${node.metadata.join(" ")}`}
                {rotation ? `\nTest rotation: ${rotation} ${direction}` : ""}
                {nodeTransitionDetails(node)}
              </title>
            </g>
            </NodeRotationPicker>
            );
          })}
        </svg>
      </div>
      <footer className="flex flex-wrap gap-x-3 gap-y-1 border-t border-white/10 bg-black/25 px-3 py-1.5 text-[10px] text-white/70" aria-label="Node color legend" title="Roles are inferred from graph labels, metadata tags, and possible ARM boss spawn hooks.">
        {Object.entries(nodeRoles).map(([role, style]) => (
          <span className="flex items-center gap-1" key={role}><span className="size-2 rounded-full" style={{ backgroundColor: style.color }} />{style.label}</span>
        ))}
      </footer>
    </div>
  );
}

function ZoneInspector({
  zone,
}: {
  zone: ZoneSummary | null;
}) {
  const { state } = useGraphWorkspace();
  const graph = state.graph;
  const canvas = graph ? nodeCanvas(graph.width, graph.height) : null;
  const namedRooms = new Set(graph?.nodes.flatMap((node) => node.label ? [node.label] : [])).size;
  if (!zone) {
    return (
      <aside className="rounded-md border bg-card p-4 text-sm text-muted-foreground">
        No zone selected
      </aside>
    );
  }

  return (
    <aside className="grid min-h-0 grid-rows-[auto_minmax(0,1fr)] rounded-md border bg-card shadow-sm">
      <header className="flex items-center justify-between gap-3 border-b px-3 py-2.5">
        <div>
          <h3 className="text-sm font-semibold">Selection Metadata</h3>
          <p className="text-xs text-muted-foreground">Zone and node metadata</p>
        </div>
        <Badge variant={zone.isTown ? "secondary" : "outline"}>
          {zone.isTown ? <Home className="mr-1 size-3" /> : <MapIcon className="mr-1 size-3" />}
          {zone.isTown ? "Town" : "Area"}
        </Badge>
      </header>

      <ScrollArea className="min-h-0">
        <div className="space-y-4 p-3">
          <dl className="space-y-2 text-sm">
            <DetailRow label="ID" value={zone.id} />
            <DetailRow label="Area Level" value={zone.areaLevel} />
            <DetailRow label="Topology Rows" value={zone.topologyIndices.length} />
          </dl>

          <Separator />

          <section aria-label="Layout metadata" className="space-y-3">
            <h4 className="text-sm font-semibold">Layout</h4>
            {graph ? (
              <>
                <p className="break-all font-mono text-xs" title={graph.logicalPath}>{baseName(graph.logicalPath)}</p>
                <dl className="space-y-2 text-sm">
                  <DetailRow label="Format" value={`${graph.logicalPath.split(".").at(-1)?.toUpperCase() ?? "Graph"}${graph.version === null ? "" : ` v${graph.version}`}`} />
                  <div title="Declared Size header from the selected layout file; not confirmed final generated zone bounds.">
                    <DetailRow label="Layout size" value={graph.width !== null && graph.height !== null ? `${graph.width} x ${graph.height}` : "Not declared"} />
                  </div>
                  <div title="Declared Size multiplied by 24, used for the viewer's rotation tests.">
                    <DetailRow label="Test canvas" value={canvas ? `${canvas.width} x ${canvas.height} units` : "Unavailable"} />
                  </div>
                  <DetailRow label="Nodes" value={graph.nodes.length} />
                  <DetailRow label="Edges" value={graph.edges.length} />
                  <DetailRow label="Room labels" value={namedRooms} />
                </dl>
                {graph.masterFile && <div className="space-y-1">
                  <p className="text-xs font-medium text-muted-foreground">Master file</p>
                  <p className="break-all font-mono text-[10px] text-muted-foreground">{graph.masterFile}</p>
                </div>}
              </>
            ) : <p className="text-xs text-muted-foreground">Layout metadata unavailable</p>}
          </section>

          <Separator />

          <GraphNodeDetails />
        </div>
      </ScrollArea>
    </aside>
  );
}

function DetailRow({ label, value }: { label: string; value: number | string }) {
  return (
    <div className="grid grid-cols-[112px_minmax(0,1fr)] gap-2 border-b pb-2 last:border-b-0 last:pb-0">
      <dt className="text-xs font-medium text-muted-foreground">{label}</dt>
      <dd className="min-w-0 break-words font-medium">{value}</dd>
    </div>
  );
}

type EnvironmentLoadState =
  | { status: "loading" }
  | { status: "ready"; byPath: Map<string, LayoutEnvironment>; warnings: string[] }
  | { status: "error"; message: string };

function EnvironmentBadge({ layout, unavailableReason }: {
  layout: LayoutEnvironment | undefined;
  unavailableReason: string;
}) {
  const environment = layout?.environment ?? "unknown";
  const styles = {
    indoor: { label: "Indoor", icon: Home, className: "border-sky-200 bg-sky-50 text-sky-800" },
    outdoor: { label: "Outdoor", icon: TreePine, className: "border-emerald-200 bg-emerald-50 text-emerald-800" },
    mixed: { label: "Mixed", icon: Layers, className: "border-amber-200 bg-amber-50 text-amber-800" },
    unknown: { label: "Unknown", icon: CircleHelp, className: "text-muted-foreground" },
  }[environment];
  const Icon = styles.icon;
  return (
    <Badge
      className={`gap-1 whitespace-nowrap ${styles.className}`}
      title={layout ? `Room-set classification\n${layout.evidence.join("\n")}` : unavailableReason}
      variant="outline"
    >
      <Icon className="size-3" />{styles.label}
    </Badge>
  );
}

function LayoutPicker({
  onSelectLayout,
  selectedLayoutPath,
  terrainFiles,
  previousLayoutPath,
  nextLayoutPath,
}: {
  onSelectLayout: (logicalPath: string) => void;
  selectedLayoutPath: string | null;
  terrainFiles: TerrainFileSummary[];
  previousLayoutPath: string | null;
  nextLayoutPath: string | null;
}) {
  const [open, setOpen] = useState(false);
  const closeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  function cancelClose() {
    if (closeTimer.current !== null) clearTimeout(closeTimer.current);
    closeTimer.current = null;
  }

  function openOnHover(event: React.PointerEvent) {
    cancelClose();
    if (event.pointerType === "mouse") setOpen(true);
  }

  function closeOnLeave(event: React.PointerEvent) {
    if (event.pointerType !== "mouse") return;
    cancelClose();
    closeTimer.current = setTimeout(() => setOpen(false), 180);
  }

  useEffect(() => () => {
    if (closeTimer.current !== null) clearTimeout(closeTimer.current);
  }, []);

  return (
    <div className="flex min-w-0 items-center gap-1">
      <Button
        aria-label="Previous layout"
        disabled={previousLayoutPath === null}
        onClick={() => { if (previousLayoutPath) onSelectLayout(previousLayoutPath); }}
        size="icon"
        title="Previous layout"
        variant="outline"
      ><ArrowLeft aria-hidden="true" /></Button>
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          aria-label="Choose layout"
          className="max-w-full gap-2 px-3"
          disabled={terrainFiles.length === 0}
          onClick={(event) => {
            if (open && event.detail > 0) event.preventDefault();
          }}
          onPointerEnter={openOnHover}
          onPointerLeave={closeOnLeave}
          title={selectedLayoutPath ?? "No layouts available"}
          variant="outline"
        >
          <GitBranch aria-hidden="true" />
          <span>{layoutCountLabel(terrainFiles.length)}</span>
          {selectedLayoutPath && <span className="hidden max-w-[200px] truncate font-mono text-xs text-muted-foreground xl:block">{baseName(selectedLayoutPath)}</span>}
          <ChevronDown aria-hidden="true" className={open ? "rotate-180" : ""} />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        aria-label="Choose layout"
        className="w-[min(480px,calc(100vw-32px))] overflow-hidden p-0"
        onOpenAutoFocus={(event) => event.preventDefault()}
        onPointerEnter={cancelClose}
        onPointerLeave={closeOnLeave}
      >
        <header className="border-b px-3 py-2 text-sm font-semibold">Possible layouts</header>
        <div className="max-h-[min(400px,60vh)] overflow-y-auto p-1">
          {terrainFiles.map((file) => (
            <button
              aria-current={file.logicalPath === selectedLayoutPath ? "true" : undefined}
              className={[
                "grid w-full grid-cols-[20px_minmax(0,1fr)_auto] items-center gap-2 rounded-sm px-2 py-2.5 text-left text-sm outline-none transition-colors",
                "hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
                file.logicalPath === selectedLayoutPath
                  ? "bg-accent/80 shadow-[inset_3px_0_0_hsl(var(--primary))]"
                  : "",
              ].join(" ")}
              disabled={file.status !== TerrainFileStatus.Extracted}
              key={`${file.kind}:${file.logicalPath}:${file.source}`}
              onClick={() => {
                onSelectLayout(file.logicalPath);
                cancelClose();
                setOpen(false);
              }}
              title={file.logicalPath}
              type="button"
            >
              {file.logicalPath === selectedLayoutPath ? <CheckCircle2 aria-hidden="true" className="size-4 text-primary" /> : <span />}
              <span className="min-w-0">
                <span className="block truncate font-mono text-xs font-medium">{baseName(file.logicalPath)}</span>
                <span className="block truncate text-[10px] text-muted-foreground">{file.logicalPath}</span>
                {file.reason && (
                  <small className="block truncate text-xs text-muted-foreground">
                    {file.reason}
                  </small>
                )}
              </span>
              <span className="justify-self-end">
                <StatusBadge status={file.status} />
              </span>
            </button>
          ))}
        </div>
      </PopoverContent>
    </Popover>
      <Button
        aria-label="Next layout"
        disabled={nextLayoutPath === null}
        onClick={() => { if (nextLayoutPath) onSelectLayout(nextLayoutPath); }}
        size="icon"
        title="Next layout"
        variant="outline"
      ><ArrowRight aria-hidden="true" /></Button>
    </div>
  );
}

function FileBrowser({
  terrainFiles,
  zone,
}: {
  terrainFiles: TerrainFileSummary[];
  zone: ZoneSummary | null;
}) {
  const initialPrefix = useMemo(
    () => initialRawFilePrefix(zone, terrainFiles),
    [terrainFiles, zone],
  );
  const [expandedPrefixes, setExpandedPrefixes] = useState<Set<string>>(
    () => new Set([initialPrefix]),
  );
  const [folderCache, setFolderCache] = useState<Map<string, RawFilesResponse>>(
    () => new Map(),
  );
  const [folderErrors, setFolderErrors] = useState<Map<string, string>>(
    () => new Map(),
  );
  const [loadingFolders, setLoadingFolders] = useState<Set<string>>(
    () => new Set(),
  );
  const [selectedFile, setSelectedFile] = useState<RawFileEntry | null>(null);

  useEffect(() => {
    setExpandedPrefixes(new Set([initialPrefix]));
    setFolderCache(new Map());
    setFolderErrors(new Map());
    setLoadingFolders(new Set());
    setSelectedFile(null);
  }, [initialPrefix]);

  const loadFolder = useCallback((prefix: string) => {
    setLoadingFolders((current) => new Set(current).add(prefix));
    loadRawFiles(prefix)
      .then((data) => {
        setFolderCache((current) => new Map(current).set(prefix, data));
        setFolderErrors((current) => {
          const next = new Map(current);
          next.delete(prefix);
          return next;
        });
      })
      .catch((error: unknown) => {
        setFolderErrors((current) =>
          new Map(current).set(
            prefix,
            error instanceof Error ? error.message : String(error),
          ),
        );
      })
      .finally(() => {
        setLoadingFolders((current) => {
          const next = new Set(current);
          next.delete(prefix);
          return next;
        });
      });
  }, []);

  useEffect(() => {
    loadFolder(initialPrefix);
  }, [initialPrefix, loadFolder]);

  const toggleFolder = useCallback(
    (prefix: string) => {
      setExpandedPrefixes((current) => {
        const next = new Set(current);
        if (next.has(prefix)) {
          next.delete(prefix);
        } else {
          next.add(prefix);
          if (!folderCache.has(prefix)) {
            loadFolder(prefix);
          }
        }
        return next;
      });
    },
    [folderCache, loadFolder],
  );

  return (
    <section className="grid h-full min-h-0 grid-cols-[340px_minmax(0,1fr)]">
      <aside className="grid min-h-0 grid-rows-[auto_minmax(0,1fr)] border-r">
        <header className="border-b px-3 py-2">
          <h3 className="text-sm font-semibold">Raw Files</h3>
          <p className="truncate font-mono text-xs text-muted-foreground">
            /{initialPrefix || "files"}
          </p>
        </header>
        <ScrollArea className="min-h-0">
          <div className="p-2">
            <FolderTreeNode
              candidatePaths={new Set(terrainFiles.map((file) => file.logicalPath))}
              depth={0}
              expandedPrefixes={expandedPrefixes}
              folderCache={folderCache}
              folderErrors={folderErrors}
              loadingFolders={loadingFolders}
              onSelectFile={setSelectedFile}
              onToggleFolder={toggleFolder}
              prefix={initialPrefix}
              selectedFile={selectedFile}
            />
          </div>
        </ScrollArea>
      </aside>

      <FilePreview file={selectedFile} />
    </section>
  );
}

function FolderTreeNode({
  candidatePaths,
  depth,
  expandedPrefixes,
  folderCache,
  folderErrors,
  loadingFolders,
  onSelectFile,
  onToggleFolder,
  prefix,
  selectedFile,
}: {
  candidatePaths: Set<string>;
  depth: number;
  expandedPrefixes: Set<string>;
  folderCache: Map<string, RawFilesResponse>;
  folderErrors: Map<string, string>;
  loadingFolders: Set<string>;
  onSelectFile: (file: RawFileEntry) => void;
  onToggleFolder: (prefix: string) => void;
  prefix: string;
  selectedFile: RawFileEntry | null;
}) {
  const expanded = expandedPrefixes.has(prefix);
  const data = folderCache.get(prefix);
  const error = folderErrors.get(prefix);
  const loading = loadingFolders.has(prefix);
  const indent = { paddingLeft: `${depth * 14 + 8}px` };

  return (
    <div>
      <button
        className="flex w-full items-center gap-1 rounded-sm py-1 pr-2 text-left text-sm outline-none transition-colors hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring"
        onClick={() => onToggleFolder(prefix)}
        style={indent}
        type="button"
      >
        {expanded ? (
          <ChevronDown className="size-3.5 text-muted-foreground" />
        ) : (
          <ChevronRight className="size-3.5 text-muted-foreground" />
        )}
        <Folder className="size-4 text-muted-foreground" />
        <span className="truncate font-medium">{baseName(prefix) || "files"}</span>
        {loading && <span className="ml-auto text-xs text-muted-foreground">...</span>}
      </button>

      {expanded && error && (
        <div
          className="py-1 pr-2 text-xs text-amber-700"
          style={{ paddingLeft: `${(depth + 1) * 14 + 28}px` }}
        >
          {error}
        </div>
      )}

      {expanded &&
        data?.folders.map((folder) => (
          <FolderTreeNode
            candidatePaths={candidatePaths}
            depth={depth + 1}
            expandedPrefixes={expandedPrefixes}
            folderCache={folderCache}
            folderErrors={folderErrors}
            key={folder.logicalPath}
            loadingFolders={loadingFolders}
            onSelectFile={onSelectFile}
            onToggleFolder={onToggleFolder}
            prefix={folder.logicalPath}
            selectedFile={selectedFile}
          />
        ))}

      {expanded &&
        data?.files.map((file) => (
          <FileTreeRow
            depth={depth + 1}
            file={file}
            highlighted={candidatePaths.has(file.logicalPath)}
            key={file.logicalPath}
            onSelectFile={onSelectFile}
            selected={selectedFile?.logicalPath === file.logicalPath}
          />
        ))}
    </div>
  );
}

function FileTreeRow({
  depth,
  file,
  highlighted,
  onSelectFile,
  selected,
}: {
  depth: number;
  file: RawFileEntry;
  highlighted: boolean;
  onSelectFile: (file: RawFileEntry) => void;
  selected: boolean;
}) {
  return (
    <button
      className={[
        "flex w-full items-center gap-1 rounded-sm py-1 pr-2 text-left text-sm outline-none transition-colors hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring",
        selected ? "bg-accent text-accent-foreground" : "",
        highlighted && !selected ? "text-primary" : "",
      ].join(" ")}
      onClick={() => onSelectFile(file)}
      style={{ paddingLeft: `${depth * 14 + 28}px` }}
      type="button"
    >
      <File className="size-4 shrink-0 text-muted-foreground" />
      <span className="min-w-0 flex-1 truncate font-mono text-xs">
        {baseName(file.logicalPath)}
      </span>
      <span className="shrink-0 text-[10px] text-muted-foreground">
        {formatBytes(file.byteLen)}
      </span>
    </button>
  );
}

function FilePreview({ file }: { file: RawFileEntry | null }) {
  const [loadState, setLoadState] = useState<
    | { status: "idle" }
    | { status: "loading" }
    | { status: "ready"; content: string }
    | { status: "error"; message: string }
  >({ status: "idle" });

  useEffect(() => {
    if (!file) {
      setLoadState({ status: "idle" });
      return;
    }
    let cancelled = false;
    setLoadState({ status: "loading" });
    loadRawFileText(file)
      .then((content) => {
        if (!cancelled) {
          setLoadState({ status: "ready", content });
        }
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
  }, [file]);

  if (!file) {
    return (
      <section className="grid min-h-0 place-items-center p-6 text-sm text-muted-foreground">
        Select a file to preview its contents.
      </section>
    );
  }

  return (
    <section className="grid min-h-0 grid-rows-[auto_minmax(0,1fr)]">
      <header className="flex items-center justify-between gap-3 border-b px-3 py-2">
        <div className="min-w-0">
          <h3 className="truncate font-mono text-sm font-semibold">
            {baseName(file.logicalPath)}
          </h3>
          <p className="truncate font-mono text-xs text-muted-foreground">
            {file.logicalPath}
          </p>
        </div>
        <Badge variant="outline">{formatBytes(file.byteLen)}</Badge>
      </header>

      {loadState.status === "loading" && (
        <div className="p-3 text-sm text-muted-foreground">Loading file...</div>
      )}
      {loadState.status === "error" && (
        <div className="p-3">
          <div className="rounded-md border border-amber-200 bg-amber-50 p-3 text-sm text-amber-900">
            {loadState.message}
          </div>
        </div>
      )}
      {loadState.status === "ready" && (
        <ScrollArea className="min-h-0">
          <pre className="whitespace-pre-wrap break-words p-3 font-mono text-xs leading-5">
            {loadState.content}
          </pre>
        </ScrollArea>
      )}
    </section>
  );
}

function StatusBadge({ status }: { status: TerrainFileStatus }) {
  if (status === TerrainFileStatus.Extracted) {
    return (
      <Badge variant="success">
        <CheckCircle2 className="mr-1 size-3" />
        {terrainStatusLabel(status)}
      </Badge>
    );
  }
  if (status === TerrainFileStatus.Missing) {
    return (
      <Badge variant="warning">
        <AlertTriangle className="mr-1 size-3" />
        {terrainStatusLabel(status)}
      </Badge>
    );
  }
  return (
    <Badge variant="secondary">
      <Info className="mr-1 size-3" />
      {terrainStatusLabel(status)}
    </Badge>
  );
}

type ScaledLayoutNode = LayoutGraphNode & {
  scaledX: number;
  scaledY: number;
};

type LayoutGraphView = {
  coordinateTransform: string;
  edges: LayoutGraph["edges"];
  height: number;
  minX: number;
  minY: number;
  nodes: ScaledLayoutNode[];
  nodesByIndex: Map<number, ScaledLayoutNode>;
  visuals: LayoutGraphVisuals;
  width: number;
};

type LayoutGraphVisuals = {
  edgeWidth: number;
  indexFontSize: number;
  indexOffset: number;
  labelFontSize: number;
  labelMaxLength: number;
  labelOffset: number;
  labeledNodeRadius: number;
  nodeRadius: number;
  nodeRingRadius: number;
  nodeStrokeWidth: number;
  ringStrokeWidth: number;
};

type GraphCamera = {
  height: number;
  minX: number;
  minY: number;
  width: number;
};

type GraphDrag = {
  camera: GraphCamera;
  pointerId: number;
  startX: number;
  startY: number;
  nodeIndex: string | null;
  moved: boolean;
};

type GraphPoint = {
  x: number;
  y: number;
};

function layoutGraphView(graph: LayoutGraph, rotations: Partial<Record<number, NodeRotation>>, direction: RotationDirection): LayoutGraphView {
  const canvas = nodeCanvas(graph.width, graph.height);
  const points = graph.nodes.map((node) => {
    const rotation = rotations[node.index];
    const point = canvas && rotation ? rotateNodePoint(node, canvas, rotation, direction) : node;
    return { node, x: point.x, y: point.y };
  });
  const xValues = points.map((point) => point.x);
  const yValues = points.map((point) => point.y);
  const minRawX = Math.min(...xValues, 0);
  const maxRawX = Math.max(...xValues, 1);
  const minRawY = Math.min(...yValues, 0);
  const maxRawY = Math.max(...yValues, 1);
  const scale = 3;
  const padding = 72;
  const unrotatedWidth = (maxRawX - minRawX) * scale;
  const unrotatedHeight = (maxRawY - minRawY) * scale;
  const centerX = unrotatedWidth / 2;
  const centerY = unrotatedHeight / 2;
  const rotation = -Math.PI / 4;
  const cos = Math.cos(rotation);
  const sin = Math.sin(rotation);
  const rotatedNodes = points.map((point) => {
    const x = (point.x - minRawX) * scale;
    const y = (point.y - minRawY) * scale;
    const dx = x - centerX;
    const dy = y - centerY;
    return {
      node: point.node,
      x: dx * cos - dy * sin + centerX,
      y: dx * sin + dy * cos + centerY,
    };
  });
  const minRotatedX = Math.min(...rotatedNodes.map((node) => node.x), 0);
  const maxRotatedX = Math.max(...rotatedNodes.map((node) => node.x), 1);
  const minRotatedY = Math.min(...rotatedNodes.map((node) => node.y), 0);
  const maxRotatedY = Math.max(...rotatedNodes.map((node) => node.y), 1);
  const rotatedWidth = maxRotatedX - minRotatedX;
  const rotatedHeight = maxRotatedY - minRotatedY;
  const width = Math.max(rotatedWidth + padding * 2, 760);
  const height = Math.max(rotatedHeight + padding * 2, 520);
  const xOffset = (width - rotatedWidth) / 2 - minRotatedX;
  const yOffset = (height - rotatedHeight) / 2 - minRotatedY;
  const originX = -minRawX * scale - centerX;
  const originY = -minRawY * scale - centerY;
  const translatedOriginX = originX * cos - originY * sin + centerX + xOffset;
  const translatedOriginY = originX * sin + originY * cos + centerY + yOffset;
  const coordinateTransform = `matrix(${scale * cos} ${scale * sin} ${-scale * sin} ${scale * cos} ${translatedOriginX} ${translatedOriginY})`;
  const nodes = rotatedNodes.map(({ node, x, y }) => ({
    ...node,
    scaledX: x + xOffset,
    scaledY: y + yOffset,
  }));
  const visuals = layoutGraphVisuals(width, height);
  const bossPositions = nodes.filter(nodeHasBoss).map((node) => bossMarkerPosition(node, visuals));
  const minX = Math.min(0, ...bossPositions.map(({ x }) => x - visuals.labelFontSize * 4 - 12));
  const minY = Math.min(0, ...bossPositions.map(({ y }) => y - visuals.nodeRingRadius - visuals.labelFontSize * 1.25 - 12));
  const maxX = Math.max(width, ...bossPositions.map(({ x }) => x + visuals.labelFontSize * 4 + 12));
  const maxY = Math.max(height, ...bossPositions.map(({ y }) => y + visuals.nodeRadius * 1.25 + 12));

  return {
    coordinateTransform,
    edges: graph.edges,
    height: maxY - minY,
    minX,
    minY,
    nodes,
    nodesByIndex: new Map(nodes.map((node) => [node.index, node])),
    visuals,
    width: maxX - minX,
  };
}

function bossMarkerPosition(node: ScaledLayoutNode, visuals: LayoutGraphVisuals): GraphPoint {
  return {
    x: node.scaledX + visuals.nodeRingRadius * 3,
    y: node.scaledY - visuals.nodeRingRadius * 2,
  };
}

function layoutGraphVisuals(width: number, height: number): LayoutGraphVisuals {
  const scale = clamp(Math.max(width / 860, height / 560), 1, 4.5);
  return {
    edgeWidth: clamp(3 * scale, 3, 9),
    indexFontSize: 10 * scale,
    indexOffset: 20 * scale,
    labelFontSize: 11 * scale,
    labelMaxLength: Math.round(clamp(18 * scale, 18, 34)),
    labelOffset: 30 * scale,
    labeledNodeRadius: 11 * scale,
    nodeRadius: 8 * scale,
    nodeRingRadius: 15 * scale,
    nodeStrokeWidth: clamp(3 * scale, 3, 9),
    ringStrokeWidth: clamp(2 * scale, 2, 7),
  };
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

function cameraFromView(view: LayoutGraphView): GraphCamera {
  return {
    height: view.height,
    minX: view.minX,
    minY: view.minY,
    width: view.width,
  };
}

function cameraCenter(camera: GraphCamera): GraphPoint {
  return {
    x: camera.minX + camera.width / 2,
    y: camera.minY + camera.height / 2,
  };
}

function zoomCamera(
  camera: GraphCamera,
  anchor: GraphPoint,
  factor: number,
  minWidth: number,
  maxWidth: number,
): GraphCamera {
  const clampedWidth = Math.min(Math.max(camera.width * factor, minWidth), maxWidth);
  const scale = clampedWidth / camera.width;
  const width = camera.width * scale;
  const height = camera.height * scale;
  const anchorRatioX = (anchor.x - camera.minX) / camera.width;
  const anchorRatioY = (anchor.y - camera.minY) / camera.height;
  return {
    height,
    minX: anchor.x - anchorRatioX * width,
    minY: anchor.y - anchorRatioY * height,
    width,
  };
}

function graphPointFromClient(
  svg: SVGSVGElement,
  camera: GraphCamera,
  clientX: number,
  clientY: number,
): GraphPoint {
  const viewport = graphViewport(svg, camera);
  return {
    x:
      camera.minX +
      ((clientX - viewport.left) / viewport.width) * camera.width,
    y:
      camera.minY +
      ((clientY - viewport.top) / viewport.height) * camera.height,
  };
}

function graphDeltaFromClientDelta(
  svg: SVGSVGElement,
  camera: GraphCamera,
  deltaX: number,
  deltaY: number,
): GraphPoint {
  const viewport = graphViewport(svg, camera);
  return {
    x: (deltaX / viewport.width) * camera.width,
    y: (deltaY / viewport.height) * camera.height,
  };
}

function graphViewport(svg: SVGSVGElement, camera: GraphCamera) {
  const bounds = svg.getBoundingClientRect();
  const viewportAspect = bounds.width / bounds.height;
  const cameraAspect = camera.width / camera.height;
  if (viewportAspect > cameraAspect) {
    const width = bounds.height * cameraAspect;
    return {
      height: bounds.height,
      left: bounds.left + (bounds.width - width) / 2,
      top: bounds.top,
      width,
    };
  }
  const height = bounds.width / cameraAspect;
  return {
    height,
    left: bounds.left,
    top: bounds.top + (bounds.height - height) / 2,
    width: bounds.width,
  };
}

function truncateLabel(label: string, maxLength: number): string {
  if (label.length <= maxLength) {
    return label;
  }
  return `${label.slice(0, Math.max(0, maxLength - 3))}...`;
}

function filterZones(zones: ZoneSummary[], query: string): ZoneSummary[] {
  const normalized = query.trim().toLowerCase();
  if (!normalized) {
    return zones;
  }
  return zones
    .map((zone, index) => ({
      index,
      score: scoreZoneMatch(zone, normalized),
      zone,
    }))
    .filter((result) => result.score > 0)
    .sort((left, right) => right.score - left.score || left.index - right.index)
    .map((result) => result.zone);
}

function layoutCountsByZone(data: LayoutData): Map<string, number> {
  return new Map(
    data.zones.map((zone) => [zone.id, terrainFilesForZone(data, zone).length]),
  );
}

function layoutCountLabel(count: number): string {
  return `${count} ${count === 1 ? "layout" : "layouts"}`;
}

function initialRawFilePrefix(
  zone: ZoneSummary | null,
  terrainFiles: TerrainFileSummary[],
): string {
  const candidateParents = terrainFiles
    .map((file) => parentPrefix(file.logicalPath))
    .filter(Boolean);
  if (candidateParents.length > 0) {
    const prefix = commonPrefix(candidateParents);
    return baseName(prefix) === "graphs" ? parentPrefix(prefix) : prefix;
  }
  return "";
}

function parentPrefix(logicalPath: string): string {
  return logicalPath.split("/").slice(0, -1).join("/");
}

function commonPrefix(paths: string[]): string {
  const [firstPath] = paths;
  if (!firstPath) {
    return "";
  }
  const firstParts = firstPath.split("/");
  let length = firstParts.length;
  for (const path of paths.slice(1)) {
    const parts = path.split("/");
    length = Math.min(length, parts.length);
    for (let index = 0; index < length; index += 1) {
      if (firstParts[index] !== parts[index]) {
        length = index;
        break;
      }
    }
  }
  return firstParts.slice(0, length).join("/");
}

function baseName(logicalPath: string): string {
  return logicalPath.split("/").filter(Boolean).at(-1) ?? logicalPath;
}

function formatBytes(byteLen: number): string {
  if (byteLen < 1024) {
    return `${byteLen} B`;
  }
  if (byteLen < 1024 * 1024) {
    return `${(byteLen / 1024).toFixed(1)} KiB`;
  }
  return `${(byteLen / (1024 * 1024)).toFixed(1)} MiB`;
}

function scoreZoneMatch(zone: ZoneSummary, query: string): number {
  const fields = [
    zone.name,
    zone.id,
    `act ${zone.act}`,
    `a${zone.act}`,
    zone.isTown ? "town" : "area",
  ];
  return Math.max(...fields.map((field) => fuzzyScore(field, query)));
}

function fuzzyScore(value: string, query: string): number {
  const normalized = value.toLowerCase();
  if (!normalized) {
    return 0;
  }
  if (normalized === query) {
    return 1000;
  }
  if (normalized.startsWith(query)) {
    return 850 - normalized.length;
  }
  const containsIndex = normalized.indexOf(query);
  if (containsIndex >= 0) {
    return 700 - containsIndex * 4 - normalized.length;
  }

  let queryIndex = 0;
  let score = 0;
  let previousMatchIndex = -1;
  for (let valueIndex = 0; valueIndex < normalized.length; valueIndex += 1) {
    if (normalized[valueIndex] !== query[queryIndex]) {
      continue;
    }
    score += previousMatchIndex + 1 === valueIndex ? 24 : 10;
    if (valueIndex === 0 || isWordBoundary(normalized[valueIndex - 1] ?? "")) {
      score += 16;
    }
    previousMatchIndex = valueIndex;
    queryIndex += 1;
    if (queryIndex === query.length) {
      return score - normalized.length;
    }
  }

  return 0;
}

function isWordBoundary(character: string): boolean {
  return character === " " || character === "_" || character === "-" || character === "'";
}

function terrainStats(terrainFiles: TerrainFileSummary[]) {
  return terrainFiles.reduce(
    (stats, file) => {
      stats.total += 1;
      if (file.status === TerrainFileStatus.Extracted) {
        stats.extracted += 1;
      } else if (file.status === TerrainFileStatus.Missing) {
        stats.missing += 1;
      } else {
        stats.candidates += 1;
      }
      return stats;
    },
    { candidates: 0, extracted: 0, missing: 0, total: 0 },
  );
}
