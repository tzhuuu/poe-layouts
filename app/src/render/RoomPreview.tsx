import { useEffect, useState } from "react";
import { AlertTriangle, Box, LoaderCircle } from "lucide-react";
import type { RoomVariant } from "../data/roomVariants";
import { loadRoomPlan, type RoomPlan } from "../data/roomPlan";
import { RoomPlanView } from "./RoomPlanView";

export type RoomPreviewProps = {
  roomLabel: string | null;
  variant: RoomVariant | null;
};

export function RoomPreview({ roomLabel, variant }: RoomPreviewProps) {
  const filename = variant?.logicalPath.split("/").at(-1);
  const path = variant?.logicalPath ?? null;
  const [state, setState] = useState<
    { status: "empty" | "loading" } | { status: "ready"; plan: RoomPlan } | { status: "error"; message: string }
  >({ status: "empty" });
  useEffect(() => {
    const controller = new AbortController();
    if (!path) {
      setState({ status: "empty" });
      return () => controller.abort();
    }
    setState({ status: "loading" });
    loadRoomPlan(path, controller.signal).then((plan) => {
      if (!controller.signal.aborted) setState({ status: "ready", plan });
    }).catch((error: unknown) => {
      if (!controller.signal.aborted) setState({ status: "error", message: error instanceof Error ? error.message : String(error) });
    });
    return () => controller.abort();
  }, [path]);
  return (
    <section aria-label="Room preview" className="grid min-h-0 min-w-0 grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden rounded-md border border-foreground/20 bg-[#101716] text-white/90 shadow-sm">
      <header className="flex min-w-0 items-center justify-between gap-3 border-b border-white/10 px-3 py-2.5">
        <h3 className="truncate font-mono text-sm font-semibold" title={variant?.logicalPath}>{filename ?? roomLabel ?? "No room selected"}</h3>
        <span className="shrink-0 text-[11px] text-white/50">ARM plan</span>
      </header>
      {state.status === "ready" && state.plan.logical_path === path ? <RoomPlanView key={path} plan={state.plan} /> : <div className="grid min-h-0 place-items-center">
        <div className="flex flex-col items-center gap-3 p-4 text-sm text-white/50">
          {state.status === "error" ? <AlertTriangle aria-hidden="true" className="size-8 text-red-300" /> : path ? <LoaderCircle aria-hidden="true" className="size-8 animate-spin" /> : <Box aria-hidden="true" className="size-8" />}
          <span role={state.status === "error" ? "alert" : "status"} className="max-w-full break-words text-center">{state.status === "error" ? state.message : path ? "Loading room..." : "No room preview available"}</span>
        </div>
      </div>}
      <footer className="flex flex-wrap gap-3 border-t border-white/10 px-3 py-2 text-xs text-white/60">
        {variant && <span>ARM v{variant.version}</span>}
        {state.status === "ready" && state.plan.logical_path === path && <><span>{state.plan.width} x {state.plan.height} tiles</span><span>{state.plan.markers.length} markers</span><span>{state.plan.objects.length} objects</span></>}
      </footer>
    </section>
  );
}
