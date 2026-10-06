import { Box } from "lucide-react";
import type { RoomVariant } from "../data/roomVariants";

export type RoomPreviewProps = {
  roomLabel: string | null;
  variant: RoomVariant | null;
};

export function RoomPreview({ roomLabel, variant }: RoomPreviewProps) {
  const filename = variant?.logicalPath.split("/").at(-1);
  return (
    <section aria-label="Room preview" className="grid min-h-0 min-w-0 grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden rounded-md border border-foreground/20 bg-[#101716] text-white/90 shadow-sm">
      <header className="min-w-0 border-b border-white/10 px-3 py-2.5">
        <h3 className="truncate font-mono text-sm font-semibold" title={variant?.logicalPath}>{filename ?? roomLabel ?? "No room selected"}</h3>
      </header>
      <div className="grid min-h-0 place-items-center">
        <div className="flex flex-col items-center gap-3 p-4 text-sm text-white/50">
          <Box aria-hidden="true" className="size-8" />
          <span>{variant ? "Room render pending" : "No room preview available"}</span>
        </div>
      </div>
      <footer className="flex flex-wrap gap-3 border-t border-white/10 px-3 py-2 text-xs text-white/60">
        {variant && <><span>{variant.width} x {variant.height}</span><span>ARM v{variant.version}</span></>}
      </footer>
    </section>
  );
}
