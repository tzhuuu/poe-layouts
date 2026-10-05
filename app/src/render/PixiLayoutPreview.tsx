import { useEffect, useRef } from "react";
import { Application, Container, Graphics } from "pixi.js";
import { TerrainFileStatus } from "../generated/poe-layouts/terrain-file-status";
import type { TerrainFileSummary, ZoneSummary } from "../data/layoutDatabase";

type PixiLayoutPreviewProps = {
  zone: ZoneSummary | null;
  terrainFiles: TerrainFileSummary[];
};

export function PixiLayoutPreview({
  zone,
  terrainFiles,
}: PixiLayoutPreviewProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const currentHost = hostRef.current;
    if (!currentHost) {
      return;
    }
    const hostElement: HTMLDivElement = currentHost;

    let cancelled = false;
    let app: Application | null = null;
    let initialized = false;

    async function mount() {
      app = new Application();
      await app.init({
        antialias: true,
        autoDensity: true,
        background: "#101716",
        resizeTo: hostElement,
        resolution: window.devicePixelRatio || 1,
      });
      initialized = true;
      if (cancelled || !app) {
        app?.destroy();
        return;
      }
      hostElement.replaceChildren(app.canvas);
      drawPreview(app, zone, terrainFiles);
    }

    void mount();

    return () => {
      cancelled = true;
      if (initialized) {
        app?.destroy();
      }
      hostElement.replaceChildren();
    };
  }, [terrainFiles, zone]);

  return <div className="pixiStage" ref={hostRef} />;
}

function drawPreview(
  app: Application,
  zone: ZoneSummary | null,
  terrainFiles: TerrainFileSummary[],
) {
  const width = app.renderer.width / app.renderer.resolution;
  const height = app.renderer.height / app.renderer.resolution;
  const scene = new Container();
  scene.position.set(width / 2, height / 2);
  app.stage.addChild(scene);

  const grid = new Graphics();
  drawGrid(grid, width, height);
  app.stage.addChildAt(grid, 0);

  if (!zone) {
    return;
  }

  const nodeCount = Math.max(3, Math.min(zone.topologyIndices.length, 28));
  const radius = Math.max(58, Math.min(width, height) * 0.28);
  const points = Array.from({ length: nodeCount }, (_, index) =>
    rotatePoint(
      Math.cos((index / nodeCount) * Math.PI * 2) * radius,
      Math.sin((index / nodeCount) * Math.PI * 2) * radius * 0.62,
      -Math.PI / 5,
    ),
  );

  const edges = new Graphics();
  for (let index = 0; index < points.length; index += 1) {
    const current = points[index];
    const next = points[(index + 1) % points.length];
    if (!current || !next) {
      continue;
    }
    edges.moveTo(current.x, current.y);
    edges.lineTo(next.x, next.y);
  }
  edges.stroke({ color: 0x48615a, width: 2, alpha: 0.86 });
  scene.addChild(edges);

  for (const [index, point] of points.entries()) {
    const candidate = terrainFiles[index % Math.max(terrainFiles.length, 1)];
    const node = new Graphics();
    node.circle(0, 0, zone.isTown ? 8 : 6);
    node.fill(nodeColor(candidate?.status));
    node.stroke({ color: 0xe7ece5, width: 1, alpha: 0.9 });
    node.position.set(point.x, point.y);
    scene.addChild(node);
  }
}

function drawGrid(grid: Graphics, width: number, height: number) {
  const spacing = 32;
  grid.rect(0, 0, width, height);
  grid.fill(0x101716);
  grid.stroke({ color: 0x101716, width: 0 });
  for (let x = -spacing; x < width + spacing; x += spacing) {
    grid.moveTo(x, 0);
    grid.lineTo(x - height * 0.36, height);
  }
  for (let y = -spacing; y < height + spacing; y += spacing) {
    grid.moveTo(0, y);
    grid.lineTo(width, y + width * 0.36);
  }
  grid.stroke({ color: 0x21302d, width: 1, alpha: 0.52 });
}

function rotatePoint(x: number, y: number, radians: number) {
  const cos = Math.cos(radians);
  const sin = Math.sin(radians);
  return {
    x: x * cos - y * sin,
    y: x * sin + y * cos,
  };
}

function nodeColor(status: TerrainFileStatus | undefined) {
  switch (status) {
    case TerrainFileStatus.Extracted:
      return 0x74b087;
    case TerrainFileStatus.Missing:
      return 0xce8f61;
    default:
      return 0x6ea0b8;
  }
}
