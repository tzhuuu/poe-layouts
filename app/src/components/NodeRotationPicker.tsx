import { RotateCcw, RotateCw } from "lucide-react";
import type { ReactElement } from "react";
import type { LayoutGraphNode } from "../data/layoutGraph";
import { NODE_ROTATIONS, rotateNodePoint, type NodeCanvas, type NodeRotation, type RotationDirection } from "../data/nodeRotation";
import { Button } from "./ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "./ui/popover";

export function NodeRotationPicker({ node, canvas, rotation, direction, open, onOpenChange, onRotationChange, onDirectionChange, children }: {
  node: LayoutGraphNode;
  canvas: NodeCanvas | null;
  rotation: NodeRotation | undefined;
  direction: RotationDirection;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onRotationChange: (rotation: NodeRotation | undefined) => void;
  onDirectionChange: (direction: RotationDirection) => void;
  children: ReactElement;
}) {
  const point = canvas && rotation ? rotateNodePoint(node, canvas, rotation, direction) : node;
  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>{children}</PopoverTrigger>
      <PopoverContent
        aria-label={`Node ${node.index} rotation`}
        className="w-[min(300px,calc(100vw-32px))] space-y-3 p-3"
        collisionPadding={16}
        onPointerDown={(event) => event.stopPropagation()}
        onWheel={(event) => event.stopPropagation()}
        side="bottom"
        sticky="always"
      >
        <header className="flex items-center justify-between gap-2">
          <div className="min-w-0">
            <h4 className="text-sm font-semibold">Node {node.index}</h4>
            {node.label && <p className="truncate font-mono text-xs text-muted-foreground" title={node.label}>{node.label}</p>}
          </div>
          <Button aria-label={`Reset node ${node.index} rotation`} disabled={rotation === undefined} onClick={() => onRotationChange(undefined)} size="icon" title="Restore source rotation" variant="ghost"><RotateCcw /></Button>
        </header>
        <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
          <dt className="text-muted-foreground">Source rotation</dt><dd className="text-right font-mono">{node.rotation ?? "any"}</dd>
          <dt className="text-muted-foreground">Test rotation</dt><dd className="text-right font-mono">{rotation ? `${rotation} ${direction === "clockwise" ? "CW" : "CCW"}` : "None"}</dd>
          <dt className="text-muted-foreground">Raw position</dt><dd className="text-right font-mono">{node.x}, {node.y}</dd>
          {rotation && <><dt className="text-muted-foreground">Test position</dt><dd className="text-right font-mono">{point.x}, {point.y}</dd></>}
        </dl>
        <div aria-label="Quarter-turn direction" className="grid grid-cols-2 gap-1" role="group">
          <Button aria-label="Counterclockwise quarter turns" aria-pressed={direction === "counterclockwise"} onClick={() => onDirectionChange("counterclockwise")} size="sm" title="R90 counterclockwise (test convention)" variant={direction === "counterclockwise" ? "secondary" : "outline"}><RotateCcw />CCW</Button>
          <Button aria-label="Clockwise quarter turns" aria-pressed={direction === "clockwise"} onClick={() => onDirectionChange("clockwise")} size="sm" title="R90 clockwise (test convention)" variant={direction === "clockwise" ? "secondary" : "outline"}><RotateCw />CW</Button>
        </div>
        <div aria-label={`Node ${node.index} rotation choices`} className="grid grid-cols-4 gap-1" role="group">
          {NODE_ROTATIONS.map((value) => <Button aria-label={`Node ${node.index} rotation ${value}`} aria-pressed={rotation === value} disabled={!canvas} key={value} onClick={() => onRotationChange(value)} size="sm" variant={rotation === value ? "secondary" : "outline"}>{value}</Button>)}
        </div>
        <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground"><span>Canvas</span><span className="font-mono">{canvas ? `${canvas.width} x ${canvas.height}` : "Size unavailable"}</span></div>
      </PopoverContent>
    </Popover>
  );
}
