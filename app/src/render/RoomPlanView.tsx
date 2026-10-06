import { useEffect, useMemo, useRef, useState } from "react";
import { Crosshair, Diamond, Grid2X2, Minus, Plus, Scan, Shapes } from "lucide-react";
import type { RoomPlan, RoomTile } from "../data/roomPlan";
import { Button } from "../components/ui/button";

type Camera = { zoom: number; x: number; y: number };
const INITIAL_CAMERA: Camera = { zoom: 1, x: 0, y: 0 };

export function RoomPlanView({ plan }: { plan: RoomPlan }) {
  const hostRef = useRef<HTMLDivElement>(null);
  const svgRef = useRef<SVGSVGElement>(null);
  const dragRef = useRef<{ id: number; x: number; y: number } | null>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [camera, setCamera] = useState<Camera>(INITIAL_CAMERA);
  const [dragging, setDragging] = useState(false);
  const [grid, setGrid] = useState(true);
  const [objects, setObjects] = useState(true);
  const [markers, setMarkers] = useState(true);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry) setSize({ width: entry.contentRect.width, height: entry.contentRect.height });
    });
    observer.observe(host);
    return () => observer.disconnect();
  }, []);

  const bounds = useMemo(() => {
    let minX = 0;
    let minY = 0;
    let maxX = plan.width * plan.tile_size;
    let maxY = plan.height * plan.tile_size;
    for (const point of [...plan.markers, ...plan.objects]) {
      minX = Math.min(minX, point.x);
      minY = Math.min(minY, point.y);
      maxX = Math.max(maxX, point.x);
      maxY = Math.max(maxY, point.y);
    }
    return { x: (minX + maxX) / 2, y: (minY + maxY) / 2, extent: (maxX - minX + maxY - minY) / Math.sqrt(2) };
  }, [plan]);
  const fitScale = Math.max(0.001, Math.min(Math.max(1, size.width - 96), Math.max(1, size.height - 96)) / bounds.extent);
  const scale = fitScale * camera.zoom;

  function zoomAt(factor: number, x = size.width / 2, y = size.height / 2) {
    setCamera((current) => {
      const zoom = Math.max(0.2, Math.min(20, current.zoom * factor));
      const ratio = zoom / current.zoom;
      return {
        zoom,
        x: current.x * ratio + (x - size.width / 2) * (1 - ratio),
        y: current.y * ratio + (y - size.height / 2) * (1 - ratio),
      };
    });
  }

  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;
    function wheel(event: WheelEvent) {
      event.preventDefault();
      const rect = svg!.getBoundingClientRect();
      const factor = Math.exp(-Math.max(-100, Math.min(100, event.deltaY)) * 0.008);
      zoomAt(factor, event.clientX - rect.left, event.clientY - rect.top);
    }
    svg.addEventListener("wheel", wheel, { passive: false });
    return () => svg.removeEventListener("wheel", wheel);
  }, [size.width, size.height]);

  function stopDrag(id: number) {
    if (dragRef.current?.id !== id) return;
    dragRef.current = null;
    setDragging(false);
  }

  const roomWidth = plan.width * plan.tile_size;
  const roomHeight = plan.height * plan.tile_size;
  const markerSize = 5 / scale;

  return (
    <div className="relative min-h-0 min-w-0 overflow-hidden" ref={hostRef}>
      <div aria-label="Room view controls" className="absolute right-2 top-2 z-10 flex gap-1 rounded-md border border-white/15 bg-[#101716]/95 p-1">
        <Button aria-label="Room grid" aria-pressed={grid} className="text-white/60 aria-pressed:bg-white/15 aria-pressed:text-white" title="24-unit grid" variant="ghost" size="icon" onClick={() => setGrid((value) => !value)}><Grid2X2 /></Button>
        <Button aria-label="Room objects" aria-pressed={objects} className="text-white/60 aria-pressed:bg-white/15 aria-pressed:text-white" title="Placed objects" variant="ghost" size="icon" onClick={() => setObjects((value) => !value)}><Shapes /></Button>
        <Button aria-label="Room markers" aria-pressed={markers} className="text-white/60 aria-pressed:bg-white/15 aria-pressed:text-white" title="Spawn and entrance markers" variant="ghost" size="icon" onClick={() => setMarkers((value) => !value)}><Crosshair /></Button>
        <Button aria-label="Zoom room out" title="Zoom out" variant="ghost" size="icon" onClick={() => zoomAt(1 / 1.25)}><Minus /></Button>
        <Button aria-label="Zoom room in" title="Zoom in" variant="ghost" size="icon" onClick={() => zoomAt(1.25)}><Plus /></Button>
        <Button aria-label="Fit room" title="Fit room" variant="ghost" size="icon" onClick={() => setCamera(INITIAL_CAMERA)}><Scan /></Button>
      </div>
      <svg
        aria-label={`Room plan ${plan.label || plan.logical_path.split("/").at(-1)}`}
        className={`h-full w-full touch-none select-none ${dragging ? "cursor-grabbing" : "cursor-grab"}`}
        ref={svgRef}
        role="img"
        viewBox={`0 0 ${Math.max(1, size.width)} ${Math.max(1, size.height)}`}
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.currentTarget.setPointerCapture(event.pointerId);
          dragRef.current = { id: event.pointerId, x: event.clientX, y: event.clientY };
          setDragging(true);
        }}
        onPointerMove={(event) => {
          const drag = dragRef.current;
          if (!drag || drag.id !== event.pointerId) return;
          const dx = event.clientX - drag.x;
          const dy = event.clientY - drag.y;
          drag.x = event.clientX;
          drag.y = event.clientY;
          setCamera((current) => ({ ...current, x: current.x + dx, y: current.y + dy }));
        }}
        onPointerUp={(event) => {
          if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
          stopDrag(event.pointerId);
        }}
        onPointerCancel={(event) => stopDrag(event.pointerId)}
        onLostPointerCapture={(event) => stopDrag(event.pointerId)}
      >
        <g data-room-scene transform={`translate(${size.width / 2 + camera.x} ${size.height / 2 + camera.y}) scale(${scale}) rotate(-45) translate(${-bounds.x} ${-bounds.y})`}>
          <rect width={roomWidth} height={roomHeight} fill="#202c2b" stroke="#9eafaa" strokeWidth={1.5} vectorEffect="non-scaling-stroke" />
          {plan.tiles.map((tile) => <TileKey key={`${tile.x},${tile.y}`} tile={tile} plan={plan} />)}
          {grid && <g data-room-grid stroke="#bad2ce" strokeOpacity={0.15} strokeWidth={0.7} vectorEffect="non-scaling-stroke" pointerEvents="none">
            {Array.from({ length: plan.width - 1 }, (_, index) => <line key={`x${index}`} x1={(index + 1) * plan.tile_size} x2={(index + 1) * plan.tile_size} y1={0} y2={roomHeight} vectorEffect="non-scaling-stroke" />)}
            {Array.from({ length: plan.height - 1 }, (_, index) => <line key={`y${index}`} y1={(index + 1) * plan.tile_size} y2={(index + 1) * plan.tile_size} x1={0} x2={roomWidth} vectorEffect="non-scaling-stroke" />)}
          </g>}
          {objects && plan.objects.map((object, index) => <g data-room-object key={index} transform={`translate(${object.x} ${object.y}) rotate(45)`}>
            <title>{`${object.art}\n${object.entity}\n${object.x}, ${object.y}`}</title>
            <rect x={-2.5 / scale} y={-2.5 / scale} width={5 / scale} height={5 / scale} fill="#c4d1cc" fillOpacity={0.7} stroke="#101716" strokeWidth={1} vectorEffect="non-scaling-stroke" />
          </g>)}
          {markers && plan.markers.map((marker, index) => {
            const boss = marker.tag.toLowerCase() === "mapboss";
            const color = boss ? "#f17e85" : marker.kind === "entrance" ? "#91d3f5" : marker.kind === "chest" ? "#e3cd6e" : "#80ceb0";
            return <g data-room-marker={marker.tag || marker.kind} key={index} transform={`translate(${marker.x} ${marker.y}) rotate(45)`}>
              <title>{`${marker.tag || marker.kind}\n${marker.x}, ${marker.y}\nRotation ${marker.rotation}`}</title>
              {boss && <circle r={10 / scale} fill="#101716" stroke={color} strokeWidth={1.5} vectorEffect="non-scaling-stroke" />}
              <path d={`M 0 ${-markerSize} L ${markerSize} 0 L 0 ${markerSize} L ${-markerSize} 0 Z`} fill={color} stroke="#101716" strokeWidth={1.5} vectorEffect="non-scaling-stroke" />
              {(boss || marker.kind === "entrance" || marker.tag.toLowerCase().includes("waypoint")) && <text x={13 / scale} y={4 / scale} fontSize={11 / scale} fill={color} stroke="#101716" strokeWidth={3 / scale} paintOrder="stroke" strokeLinejoin="round">{marker.tag || marker.kind}</text>}
            </g>;
          })}
        </g>
      </svg>
      <div aria-label="Room plan legend" className="pointer-events-none absolute bottom-2 left-3 flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-white/65">
        <span className="flex items-center gap-1"><Diamond className="size-3 text-[#80ceb0]" />Spawn</span>
        <span className="flex items-center gap-1"><Diamond className="size-3 text-[#e3cd6e]" />Chest</span>
        <span className="flex items-center gap-1"><Diamond className="size-3 text-[#91d3f5]" />Entrance</span>
        <span className="flex items-center gap-1"><Diamond className="size-3 text-[#f17e85]" />Map boss</span>
      </div>
    </div>
  );
}

function TileKey({ tile, plan }: { tile: RoomTile; plan: RoomPlan }) {
  const unit = plan.tile_size;
  const x = tile.x * unit;
  const y = tile.y * unit;
  const corners = [[0, unit], [unit, unit], [unit, 0], [0, 0]];
  const edges = [[0, unit, unit, unit], [unit, unit, unit, 0], [unit, 0, 0, 0], [0, 0, 0, unit]];
  const descriptions = tile.ground.map((index) => plan.assets[index - 1] ?? "Default ground");
  return <g data-room-tile={tile.kind} transform={`translate(${x} ${y})`}>
    <title>{`Tile ${tile.x}, ${tile.y} (${tile.kind})\nKey size ${tile.width} x ${tile.height}; origin ${tile.origin}\n${descriptions.join("\n")}\nFeature ${tile.feature}\nElevations ${tile.elevations.join(", ")}`}</title>
    <rect width={unit} height={unit} fill={tile.kind === "s" ? "#344945" : "#33413f"} />
    {tile.kind === "k" && corners.map((corner, index) => {
      const next = corners[(index + 1) % 4]!;
      return <polygon key={index} points={`${unit / 2},${unit / 2} ${corner.join(",")} ${next.join(",")}`} fill={terrainColor(descriptions[index]!)} fillOpacity={0.85} />;
    })}
    {tile.kind === "k" && edges.map((edge, index) => tile.edges[index] > 0 && <line key={index} x1={edge[0]} y1={edge[1]} x2={edge[2]} y2={edge[3]} stroke="#bcc6b6" strokeWidth={2} vectorEffect="non-scaling-stroke"><title>{plan.assets[tile.edges[index] - 1]}</title></line>)}
    {tile.kind === "f" && <circle cx={unit / 2} cy={unit / 2} r={unit / 5} fill="#e3cd6e"><title>{plan.assets[tile.feature - 1] ?? `Feature ${tile.feature}`}</title></circle>}
  </g>;
}

function terrainColor(path: string): string {
  const value = path.toLowerCase();
  if (/ocean|water|river|shore/.test(value)) return "#327b99";
  if (/cliff|rock|wall|black/.test(value)) return "#777e84";
  if (/grass|forest|moss|jungle/.test(value)) return "#588b68";
  if (/sand|dirt|scorched/.test(value)) return "#a99b68";
  if (/lava|fire/.test(value)) return "#bd6d68";
  return "#4e827d";
}
