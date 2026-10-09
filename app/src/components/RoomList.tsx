import { Fragment, useEffect, useMemo, useState } from "react";
import { Box, Layers, Search } from "lucide-react";
import { loadLayoutRooms, type LayoutRooms } from "../data/layoutRooms";
import type { RoomVariant } from "../data/roomVariants";
import type { RoomInspection } from "../data/roomInspection";
import { RoomPreview } from "../render/RoomPreview";
import { Input } from "./ui/input";
import { ScrollArea } from "./ui/scroll-area";
import { WarningBadge } from "./WarningBadge";
import { useExplorerNavigation } from "./ExplorerNavigation";

type RoomListState =
  | { status: "loading" }
  | { status: "ready"; data: LayoutRooms }
  | { status: "error"; message: string };

export function useRoomWorkspace(paths: string[], selectedLayoutPath: string | null, pathsReady: boolean) {
  const { navigation, navigate } = useExplorerNavigation();
  const [state, setState] = useState<RoomListState>({ status: "loading" });
  const [query, setQuery] = useState("");
  const selectedLabel = navigation.room;
  const selectedVariantPath = navigation.variant;

  useEffect(() => {
    const controller = new AbortController();
    setState({ status: "loading" });
    setQuery("");
    if (!pathsReady) return () => controller.abort();
    loadLayoutRooms(paths, controller.signal)
      .then((data) => {
        if (!controller.signal.aborted) setState({ status: "ready", data });
      })
      .catch((error: unknown) => {
        if (!controller.signal.aborted) {
          setState({ status: "error", message: error instanceof Error ? error.message : String(error) });
        }
      });
    return () => controller.abort();
  }, [paths, pathsReady]);

  const data = pathsReady && state.status === "ready" ? state.data : null;
  const room = data?.rooms.find((candidate) => candidate.label === selectedLabel) ?? data?.rooms[0] ?? null;
  const variants = useMemo(() => {
    const byPath = new Map<string, RoomVariant>();
    for (const catalog of data?.roomCatalogs ?? []) {
      for (const variant of catalog.variants) {
        if (variant.label === room?.label) byPath.set(variant.logicalPath, variant);
      }
    }
    return [...byPath.values()].sort((left, right) => left.logicalPath.localeCompare(right.logicalPath));
  }, [data, room?.label]);
  const variant = variants.find((candidate) => candidate.logicalPath === selectedVariantPath) ?? variants[0] ?? null;
  useEffect(() => {
    if (!data || navigation.tab !== "view" || navigation.view !== "room") return;
    const roomLabel = room?.label ?? null;
    const variantPath = variant?.logicalPath ?? null;
    if (navigation.room !== roomLabel || navigation.variant !== variantPath) {
      navigate({ room: roomLabel, variant: variantPath }, "replace");
    }
  }, [data, navigation.tab, navigation.view, navigation.room, navigation.variant, room?.label, variant?.logicalPath, navigate]);
  const warnings = [...new Set([...(data?.warnings ?? []), ...(data?.roomCatalogs.flatMap((catalog) => catalog.warnings) ?? [])])];
  const filteredRooms = data?.rooms.filter((candidate) => candidate.label.toLowerCase().includes(query.trim().toLowerCase())) ?? [];
  const occurrenceCount = room?.layouts.find((layout) => layout.logicalPath === selectedLayoutPath)?.nodeIndices.length ?? 0;
  const roomSets = [...new Set(data?.roomCatalogs.filter((catalog) => catalog.variants.some((candidate) => candidate.logicalPath === variant?.logicalPath)).flatMap((catalog) => catalog.roomSet ? [catalog.roomSet] : []) ?? [])];

  return {
    state, data, room, variants, variant, warnings, filteredRooms,
    occurrenceCount, roomSets, query, setQuery,
    selectVariant: (variant: string) => navigate({ variant }), selectedLayoutPath,
  };
}

type RoomWorkspace = ReturnType<typeof useRoomWorkspace>;

export function RoomSidebar({ workspace }: { workspace: RoomWorkspace }) {
  const { navigation, navigate } = useExplorerNavigation();
  const { state, data, room, filteredRooms, query, setQuery, warnings } = workspace;

  return (
    <section aria-label="Rooms sidebar" className="flex min-h-0 min-w-0 shrink-0 flex-col gap-2 border-t pt-3">
      <header className="flex shrink-0 items-center justify-between gap-2">
      <h2 className="flex items-center gap-2 text-sm font-semibold">
        <Layers aria-hidden="true" className="size-4 shrink-0" />
        Rooms
        {data && <span className="text-xs font-normal text-muted-foreground">{data.rooms.length}</span>}
      </h2>
      <WarningBadge label="Room warnings" warnings={warnings}>{warnings.length}</WarningBadge>
      </header>
      <label className="relative block shrink-0">
        <Search aria-hidden="true" className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
        <Input aria-label="Search sidebar rooms" className="h-8 pl-8" placeholder="Search rooms" value={query} onChange={(event) => setQuery(event.target.value)} />
      </label>
      <ScrollArea aria-label="Room choices" className="h-40 min-h-0 flex-none md:h-auto md:flex-1">
        <div className="space-y-1 pr-2">
          {state.status === "loading" && <p className="py-3 text-sm text-muted-foreground" role="status">Loading rooms</p>}
          {state.status === "error" && <p role="alert" className="break-words py-3 text-xs text-destructive">{state.message}</p>}
          {filteredRooms.map((candidate) => {
            const selected = navigation.tab === "view" && navigation.view === "room" && candidate.label === room?.label;
            return (
              <button
                aria-current={selected ? "true" : undefined}
                aria-label={`View room ${candidate.label}`}
                className={`grid w-full grid-cols-[minmax(0,1fr)_auto] items-center gap-2 rounded-sm px-2 py-2 text-left outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring ${selected ? "bg-accent/80 shadow-[inset_3px_0_0_hsl(var(--primary))]" : ""}`}
                key={candidate.label}
                onClick={() => navigate({ tab: "view", view: "room", room: candidate.label })}
                title={`${candidate.label}: ${candidate.layouts.length} of ${data?.parsedLayoutCount} layouts`}
                type="button"
              >
                <span className="min-w-0 truncate font-mono text-xs">{candidate.label}</span>
                <span className="shrink-0 text-[11px] tabular-nums text-muted-foreground">{candidate.layouts.length} / {data?.parsedLayoutCount}</span>
              </button>
            );
          })}
          {data && filteredRooms.length === 0 && <p className="py-3 text-sm text-muted-foreground">{data.rooms.length === 0 ? "No named rooms" : "No matching rooms"}</p>}
        </div>
      </ScrollArea>
    </section>
  );
}

export function RoomList({ workspace }: { workspace: RoomWorkspace }) {
  const { state, data, room, variants, variant, occurrenceCount, roomSets, selectVariant, selectedLayoutPath } = workspace;
  const [inspection, setInspection] = useState<RoomInspection | null>(null);
  useEffect(() => setInspection(null), [room?.label, variant?.logicalPath]);
  const inspected = inspection?.path === variant?.logicalPath ? inspection : null;

  return (
    <section className="grid h-full min-h-0 min-w-0 grid-rows-[minmax(0,1fr)]">
      {state.status === "loading" && <div className="grid place-items-center text-sm text-muted-foreground">Loading rooms...</div>}
      {state.status === "error" && <p role="alert" className="break-words p-3 text-sm text-destructive">{state.message}</p>}
      {data && (
        <div className="grid min-h-0 min-w-0 grid-rows-[minmax(0,1fr)_140px] gap-3 lg:grid-cols-[minmax(0,1fr)_240px] lg:grid-rows-1 xl:grid-cols-[minmax(0,1fr)_320px]">
          <RoomPreview roomLabel={room?.label ?? null} variant={variant} onInspect={setInspection} />
          <aside aria-label="Room details" className="min-h-0 overflow-y-auto rounded-md border bg-card shadow-sm">
            <header className="border-b px-3 py-2.5">
              <h3 className="break-all font-mono text-sm font-semibold">{room?.label ?? "No named rooms"}</h3>
              <span className="text-xs text-muted-foreground">{variants.length} ARM {variants.length === 1 ? "variant" : "variants"}</span>
            </header>
            <section aria-label="Room node metadata" className="space-y-2 border-b p-3" data-inspected-room-kind={inspected?.kind} data-inspected-room-index={inspected?.index}>
              <h4 className="text-xs font-semibold">Node Metadata</h4>
              {inspected ? <>
                <p className="break-all font-mono text-xs">{inspected.label}</p>
                <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-2 text-xs [&>dt]:text-muted-foreground [&>dd]:min-w-0 [&>dd]:whitespace-pre-wrap [&>dd]:break-all [&>dd]:text-right">
                  <dt>Item</dt><dd>{inspected.kind} {inspected.index}</dd>
                  {inspected.fields.map((field) => <Fragment key={field.label}><dt>{field.label}</dt><dd>{field.value}</dd></Fragment>)}
                </dl>
              </> : <p className="text-xs text-muted-foreground">No node inspected</p>}
            </section>
            {room && (
              <>
                <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-2 border-b p-3 text-xs">
                  <dt className="text-muted-foreground">Layouts</dt><dd className="text-right tabular-nums">{room.layouts.length} / {data.parsedLayoutCount}</dd>
                  <dt className="text-muted-foreground">Total nodes</dt><dd className="text-right tabular-nums">{room.nodeCount}</dd>
                  <dt className="text-muted-foreground">Current layout</dt><dd className="text-right tabular-nums">{occurrenceCount} {occurrenceCount === 1 ? "node" : "nodes"}</dd>
                </dl>
                <section aria-label="Room variants" className="border-b">
                  <h4 className="px-3 py-2 text-xs font-semibold">Possibilities</h4>
                  {variants.map((candidate) => (
                    <button
                      aria-pressed={candidate.logicalPath === variant?.logicalPath}
                      className={`flex w-full items-start gap-2 border-t px-3 py-2.5 text-left hover:bg-accent focus-visible:outline-ring ${candidate.logicalPath === variant?.logicalPath ? "bg-accent/80 shadow-[inset_3px_0_0_hsl(var(--primary))]" : ""}`}
                      key={candidate.logicalPath}
                      onClick={() => selectVariant(candidate.logicalPath)}
                      title={candidate.logicalPath}
                      type="button"
                    >
                      <Box aria-hidden="true" className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
                      <span className="min-w-0">
                        <span className="block break-all font-mono text-xs font-medium">{baseName(candidate.logicalPath)}</span>
                        <span className="text-[11px] text-muted-foreground">{candidate.width} x {candidate.height}</span>
                      </span>
                    </button>
                  ))}
                  {variants.length === 0 && <p className="px-3 pb-3 text-xs text-muted-foreground">No matching ARM variants extracted.</p>}
                </section>
                {variant && (
                  <section className="border-b p-3">
                    <h4 className="mb-2 text-xs font-semibold">Selected variant</h4>
                    <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-2 text-xs">
                      <dt className="text-muted-foreground">Header size</dt><dd className="text-right">{variant.width} x {variant.height}</dd>
                      <dt className="text-muted-foreground">ARM version</dt><dd className="text-right">{variant.version}</dd>
                      <dt className="text-muted-foreground">Asset entries</dt><dd className="text-right">{variant.assetCount}</dd>
                      {variant.bossTags.length > 0 && <><dt className="text-muted-foreground">Boss spawn tags</dt><dd className="break-all text-right">{variant.bossTags.join(", ")}</dd></>}
                    </dl>
                    <p className="mt-3 break-all font-mono text-[10px] text-muted-foreground">{variant.logicalPath}</p>
                    {roomSets.map((path) => <p className="mt-2 break-all font-mono text-[10px] text-muted-foreground" key={path} title="Active room set">{path}</p>)}
                  </section>
                )}
                <section className="p-3">
                  <h4 className="mb-2 text-xs font-semibold">Appears in layouts</h4>
                  {room.layouts.map((layout) => (
                    <div className={`border-b py-2 last:border-0 ${layout.logicalPath === selectedLayoutPath ? "text-primary" : "text-muted-foreground"}`} key={layout.logicalPath} title={layout.logicalPath}>
                      <span className="block break-all font-mono text-[11px]">{baseName(layout.logicalPath)}</span>
                      <span className="text-[10px]">Nodes: {layout.nodeIndices.join(", ")}</span>
                    </div>
                  ))}
                </section>
              </>
            )}
          </aside>
        </div>
      )}
    </section>
  );
}

function baseName(path: string): string {
  return path.split("/").at(-1) ?? path;
}
