import { useEffect, useMemo, useState } from "react";
import { Box, CheckCircle2, ChevronDown, Layers, Search } from "lucide-react";
import { loadLayoutRooms, type LayoutRooms } from "../data/layoutRooms";
import type { RoomVariant } from "../data/roomVariants";
import { RoomPreview } from "../render/RoomPreview";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";
import { HoverPopover } from "./ui/hover-popover";
import { Input } from "./ui/input";
import { WarningBadge } from "./WarningBadge";
import { useExplorerNavigation } from "./ExplorerNavigation";

type RoomListState =
  | { status: "loading" }
  | { status: "ready"; data: LayoutRooms }
  | { status: "error"; message: string };

export function useRoomWorkspace(paths: string[], selectedLayoutPath: string | null) {
  const { navigation, navigate } = useExplorerNavigation();
  const [state, setState] = useState<RoomListState>({ status: "loading" });
  const [query, setQuery] = useState("");
  const selectedLabel = navigation.room;
  const selectedVariantPath = navigation.variant;

  useEffect(() => {
    const controller = new AbortController();
    setState({ status: "loading" });
    setQuery("");
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
  }, [paths]);

  const data = state.status === "ready" ? state.data : null;
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
    if (!data || navigation.tab !== "rooms") return;
    const roomLabel = room?.label ?? null;
    const variantPath = variant?.logicalPath ?? null;
    if (navigation.room !== roomLabel || navigation.variant !== variantPath) {
      navigate({ room: roomLabel, variant: variantPath }, "replace");
    }
  }, [data, navigation.tab, navigation.room, navigation.variant, room?.label, variant?.logicalPath, navigate]);
  const warnings = [...new Set([...(data?.warnings ?? []), ...(data?.roomCatalogs.flatMap((catalog) => catalog.warnings) ?? [])])];
  const filteredRooms = data?.rooms.filter((candidate) => candidate.label.toLowerCase().includes(query.trim().toLowerCase())) ?? [];
  const occurrenceCount = room?.layouts.find((layout) => layout.logicalPath === selectedLayoutPath)?.nodeIndices.length ?? 0;
  const roomSets = [...new Set(data?.roomCatalogs.filter((catalog) => catalog.variants.some((candidate) => candidate.logicalPath === variant?.logicalPath)).flatMap((catalog) => catalog.roomSet ? [catalog.roomSet] : []) ?? [])];

  function selectRoom(label: string) {
    navigate({ room: label });
  }

  return {
    state, data, room, variants, variant, warnings, filteredRooms,
    occurrenceCount, roomSets, query, setQuery, selectRoom,
    selectVariant: (variant: string) => navigate({ variant }), selectedLayoutPath,
  };
}

type RoomWorkspace = ReturnType<typeof useRoomWorkspace>;

export function RoomPicker({ workspace }: { workspace: RoomWorkspace }) {
  const { data, room, warnings, filteredRooms, query, setQuery, selectRoom } = workspace;

  return (
    <div className="flex min-w-0 flex-wrap items-center gap-2">
      <HoverPopover
        label="Choose room"
        trigger={
          <Button aria-label="Choose room" className="max-w-full" disabled={!data?.rooms.length} variant="outline">
            <Layers aria-hidden="true" />
            <span className="max-w-[220px] truncate font-mono text-xs">{room?.label ?? "No room selected"}</span>
            <Badge variant="secondary">{data?.rooms.length ?? 0} rooms</Badge>
            <ChevronDown aria-hidden="true" />
          </Button>
        }
      >
        {(close) => (
          <>
            <div className="border-b p-2">
              <label className="relative block">
                <Search aria-hidden="true" className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
                <Input aria-label="Search room labels" className="pl-8" placeholder="Search rooms" value={query} onChange={(event) => setQuery(event.target.value)} />
              </label>
            </div>
            <div className="max-h-[min(400px,60vh)] overflow-y-auto p-1">
              {filteredRooms.map((candidate) => (
                <button
                  aria-current={candidate.label === room?.label ? "true" : undefined}
                  className={`grid w-full grid-cols-[20px_minmax(0,1fr)_auto] items-center gap-2 rounded-sm px-2 py-2.5 text-left hover:bg-accent focus-visible:outline-ring ${candidate.label === room?.label ? "bg-accent/80 shadow-[inset_3px_0_0_hsl(var(--primary))]" : ""}`}
                  key={candidate.label}
                  onClick={() => {
                    selectRoom(candidate.label);
                    close();
                  }}
                  type="button"
                >
                  {candidate.label === room?.label ? <CheckCircle2 aria-hidden="true" className="size-4 text-primary" /> : <span />}
                  <span className="min-w-0 break-all font-mono text-xs">{candidate.label}</span>
                  <span className="shrink-0 text-xs tabular-nums text-muted-foreground">{candidate.layouts.length} / {data?.parsedLayoutCount} layouts</span>
                </button>
              ))}
              {filteredRooms.length === 0 && <p className="p-3 text-sm text-muted-foreground">No matching rooms.</p>}
            </div>
          </>
        )}
      </HoverPopover>
      <WarningBadge label="Room warnings" warnings={warnings} />
    </div>
  );
}

export function RoomList({ workspace }: { workspace: RoomWorkspace }) {
  const { state, data, room, variants, variant, occurrenceCount, roomSets, selectVariant, selectedLayoutPath } = workspace;

  return (
    <section className="grid h-full min-h-0 min-w-0 grid-rows-[minmax(0,1fr)]">
      {state.status === "loading" && <div className="grid place-items-center text-sm text-muted-foreground">Loading rooms...</div>}
      {state.status === "error" && <p role="alert" className="break-words p-3 text-sm text-destructive">{state.message}</p>}
      {data && (
        <div className="grid min-h-0 min-w-0 grid-cols-[minmax(0,1fr)_240px] gap-3 xl:grid-cols-[minmax(0,1fr)_320px]">
          <RoomPreview roomLabel={room?.label ?? null} variant={variant} />
          <aside aria-label="Room details" className="min-h-0 overflow-y-auto rounded-md border bg-card shadow-sm">
            <header className="border-b px-3 py-2.5">
              <h3 className="break-all font-mono text-sm font-semibold">{room?.label ?? "No named rooms"}</h3>
              <span className="text-xs text-muted-foreground">{variants.length} ARM {variants.length === 1 ? "variant" : "variants"}</span>
            </header>
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
