import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  Bug,
  ChevronDown,
  ChevronRight,
  CheckCircle2,
  Compass,
  Database,
  File,
  Folder,
  FolderTree,
  GitBranch,
  Home,
  Info,
  Layers,
  Map as MapIcon,
  Search,
  TableProperties,
} from "lucide-react";
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
  loadRawFiles,
  loadRawFileText,
  type RawFilesResponse,
  type RawFileEntry,
  type RawFolderEntry,
} from "../data/rawFiles";
import { PixiLayoutPreview } from "../render/PixiLayoutPreview";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "./ui/popover";
import { ScrollArea } from "./ui/scroll-area";
import { Separator } from "./ui/separator";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "./ui/tabs";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "./ui/tooltip";

type LoadState =
  | { status: "loading" }
  | { status: "ready"; data: LayoutData }
  | { status: "error"; message: string };

export function App() {
  const [loadState, setLoadState] = useState<LoadState>({ status: "loading" });
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
    <TooltipProvider>
      <LayoutExplorer
        data={loadState.data}
        query={query}
        selectedZoneId={selectedZoneId}
        onQueryChange={setQuery}
        onSelectZone={setSelectedZoneId}
      />
    </TooltipProvider>
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
    filteredZones.find((zone) => zone.id === selectedZoneId) ??
    filteredZones[0] ??
    data.zones[0] ??
    null;
  const selectedTerrain = selectedZone
    ? terrainFilesForZone(data, selectedZone)
    : [];
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
    <main className="grid h-screen min-h-[680px] min-w-[1060px] grid-cols-[340px_minmax(0,1fr)] overflow-hidden bg-background text-foreground">
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

      <WorkspaceTabs selectedTerrain={selectedTerrain} selectedZone={selectedZone} />
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
                <strong className="block truncate font-medium">
                  {zone.name || zone.id}
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
  selectedTerrain,
  selectedZone,
}: {
  selectedTerrain: TerrainFileSummary[];
  selectedZone: ZoneSummary | null;
}) {
  return (
    <Tabs
      className="grid min-h-0 min-w-0 grid-rows-[auto_minmax(0,1fr)] gap-3 p-4"
      defaultValue="layouts"
    >
      <WorkspaceHeader
        selectedTerrain={selectedTerrain}
        selectedZone={selectedZone}
      />

      <TabsContent
        className="grid min-h-0 grid-rows-[minmax(0,1fr)_270px] gap-3"
        value="layouts"
      >
        <section className="grid min-h-0 grid-cols-[minmax(0,1fr)_320px] gap-3">
          <section className="relative min-h-0 overflow-hidden rounded-md border border-foreground/20 bg-[#101716] shadow-sm">
            <div className="absolute left-3 top-3 z-10 flex gap-1">
              <ToolbarButton label="World orientation">
                <Compass />
              </ToolbarButton>
              <ToolbarButton label="Topology layers">
                <Layers />
              </ToolbarButton>
            </div>
            <PixiLayoutPreview zone={selectedZone} terrainFiles={selectedTerrain} />
          </section>

          <ZoneInspector zone={selectedZone} />
        </section>

        <LayoutCandidatesPanel terrainFiles={selectedTerrain} />
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
}: {
  selectedTerrain: TerrainFileSummary[];
  selectedZone: ZoneSummary | null;
}) {
  const missing = selectedTerrain.filter(
    (file) => file.status === TerrainFileStatus.Missing,
  ).length;

  return (
    <header className="grid gap-3">
      <div className="flex min-h-10 items-start justify-between gap-4">
        <div className="min-w-0">
          <h2 className="truncate text-2xl font-semibold tracking-tight">
            {selectedZone?.name ?? "No zone selected"}
          </h2>
        </div>
        {missing > 0 && (
          <Badge variant="warning">
            <AlertTriangle className="mr-1 size-3" />
            {missing} missing
          </Badge>
        )}
      </div>

      <TabsList className="w-fit">
        <TabsTrigger value="layouts">
          <TableProperties />
          Layouts
        </TabsTrigger>
        <TabsTrigger value="files">
          <FolderTree />
          Files
        </TabsTrigger>
      </TabsList>
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

function ZoneInspector({
  zone,
}: {
  zone: ZoneSummary | null;
}) {
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
          <p className="text-xs text-muted-foreground">Zone and hover target</p>
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

          <section className="space-y-2">
            <h4 className="flex items-center gap-2 text-sm font-semibold">
              <GitBranch className="size-4 text-muted-foreground" />
              Hover Target
            </h4>
            <div className="rounded-md border border-dashed p-3 text-xs leading-5 text-muted-foreground">
              Pixi hover metadata will land here once nodes and edges expose hit-test
              details.
            </div>
          </section>
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

function LayoutCandidatesPanel({
  terrainFiles,
}: {
  terrainFiles: TerrainFileSummary[];
}) {
  return (
    <section className="grid min-h-0 grid-rows-[auto_minmax(0,1fr)] rounded-md border bg-card shadow-sm">
      <div className="flex items-center justify-between gap-3 border-b px-3 py-2">
        <h3 className="text-sm font-semibold">
          {terrainFiles.length} possible{" "}
          {terrainFiles.length === 1 ? "layout" : "layouts"}
        </h3>
      </div>

      <TerrainTable terrainFiles={terrainFiles} />
    </section>
  );
}

function TerrainTable({ terrainFiles }: { terrainFiles: TerrainFileSummary[] }) {
  return (
    <ScrollArea className="h-full">
      <div className="grid min-w-[760px] grid-cols-[116px_minmax(0,1fr)_112px] border-b bg-muted/40 px-3 py-2 text-xs font-semibold uppercase text-muted-foreground">
        <span>Kind</span>
        <span>Possible Layout</span>
        <span className="text-right">Status</span>
      </div>
      {terrainFiles.map((file) => (
        <article
          className="grid min-w-[760px] grid-cols-[116px_minmax(0,1fr)_112px] items-center gap-3 border-b px-3 py-2.5 text-sm last:border-b-0"
          key={`${file.kind}:${file.logicalPath}:${file.source}`}
        >
          <strong className="font-medium text-muted-foreground">
            {terrainKindLabel(file.kind)}
          </strong>
          <span className="min-w-0">
            <span className="block truncate font-mono text-xs">{file.logicalPath}</span>
            {file.reason && (
              <small className="block truncate text-xs text-muted-foreground">
                {file.reason}
              </small>
            )}
          </span>
          <span className="justify-self-end">
            <StatusBadge status={file.status} />
          </span>
        </article>
      ))}
    </ScrollArea>
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

function ToolbarButton({
  children,
  label,
}: {
  children: React.ReactNode;
  label: string;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          className="border-white/15 bg-black/35 text-white backdrop-blur hover:bg-black/50"
          size="icon"
          type="button"
          variant="outline"
        >
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
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
