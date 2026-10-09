export type LayoutGraphNode = {
  index: number;
  x: number;
  y: number;
  links: number[];
  label: string | null;
  rotation: string | null;
  metadata: string[];
  transitions: LayoutTransition[];
  roomBosses: { roomPaths: string[]; tags: string[]; candidateCount: number } | null;
};

export type LayoutTransition = {
  kind: "zone" | "door" | "unresolved" | "deferred";
  tag: string | null;
  destination: { id: string; name: string } | null;
  objectPath: string | null;
  roomPaths: string[];
  basis: string;
};

export type LayoutGraphEdge = {
  index: number;
  from: number;
  to: number;
  edgeTile: string | null;
  metadata: string[];
};

export type LayoutGraph = {
  logicalPath: string;
  version: number | null;
  width: number | null;
  height: number | null;
  masterFile: string | null;
  nodes: LayoutGraphNode[];
  edges: LayoutGraphEdge[];
  warnings: string[];
};

type RawLayoutGraph = {
  logical_path: string;
  version: number | null;
  width: number | null;
  height: number | null;
  master_file: string | null;
  nodes: RawLayoutGraphNode[];
  edges: RawLayoutGraphEdge[];
  warnings: string[];
  node_transitions?: { node_index: number; transitions: RawLayoutTransition[] }[];
  node_bosses?: { node_index: number; room_paths: string[]; boss_tags: string[]; candidate_count: number }[];
};

type RawLayoutTransition = Omit<LayoutTransition, "objectPath" | "roomPaths"> & {
  object_path: string | null;
  room_paths: string[];
};

type RawLayoutGraphNode = {
  index: number;
  x: number;
  y: number;
  links: number[];
  label: string | null;
  rotation: string | null;
  metadata: string[];
};

type RawLayoutGraphEdge = {
  index: number;
  from: number;
  to: number;
  edge_tile: string | null;
  metadata: string[];
};

export async function loadLayoutGraph(logicalPath: string, zoneId?: string, signal?: AbortSignal): Promise<LayoutGraph> {
  const params = new URLSearchParams({ path: logicalPath });
  if (zoneId) params.set("zone", zoneId);
  const response = await fetch(`/api/layout-graph?${params.toString()}`, {
    cache: "no-store",
    signal,
  });
  if (!response.ok) {
    throw new Error(`Could not load layout graph (${response.status})`);
  }
  const contentType = response.headers.get("content-type") ?? "";
  if (!contentType.includes("application/json")) {
    const body = await response.text();
    throw new Error(
      `Could not load layout graph: expected JSON, got ${contentType || "unknown content type"} (${body.slice(0, 40)})`,
    );
  }
  return normalizeLayoutGraph((await response.json()) as RawLayoutGraph);
}

function normalizeLayoutGraph(raw: RawLayoutGraph): LayoutGraph {
  const transitions = new Map(raw.node_transitions?.map((node) => [node.node_index, node.transitions]));
  const bosses = new Map(raw.node_bosses?.map((node) => [node.node_index, {
    roomPaths: node.room_paths, tags: node.boss_tags, candidateCount: node.candidate_count,
  }]));
  return {
    logicalPath: raw.logical_path,
    version: raw.version,
    width: raw.width,
    height: raw.height,
    masterFile: raw.master_file,
    nodes: raw.nodes.map((node) => ({
      index: node.index,
      x: node.x,
      y: node.y,
      links: node.links,
      label: node.label,
      rotation: node.rotation,
      metadata: node.metadata,
      roomBosses: bosses.get(node.index) ?? null,
      transitions: (transitions.get(node.index) ?? []).map((transition) => ({
        kind: transition.kind,
        tag: transition.tag,
        destination: transition.destination,
        objectPath: transition.object_path,
        roomPaths: transition.room_paths,
        basis: transition.basis,
      })),
    })),
    edges: raw.edges.map((edge) => ({
      index: edge.index,
      from: edge.from,
      to: edge.to,
      edgeTile: edge.edge_tile,
      metadata: edge.metadata,
    })),
    warnings: raw.warnings,
  };
}

export function nodeLabelLines(node: LayoutGraphNode): string[] {
  const destinations = [...new Set(node.transitions.flatMap((transition) =>
    transition.destination ? [transition.destination.name] : [],
  ))];
  if (destinations.length > 0) return destinations;
  if (node.transitions.some((transition) => transition.kind === "deferred")) return ["Internal transition"];
  if (node.transitions.some((transition) => transition.kind === "unresolved")) return ["Unresolved entrance"];
  if (node.transitions.some((transition) => transition.kind === "door")) return node.label ? [node.label, "Possible door"] : ["Possible door"];
  return node.label ? [node.label] : [];
}

export function nodeTransitionDetails(node: LayoutGraphNode): string {
  return node.transitions.map((transition) => {
    const detail = transition.destination
      ? `${transition.destination.name} (${transition.destination.id})`
      : transition.kind === "door"
        ? `Possible door: ${transition.objectPath}`
        : transition.kind === "deferred"
          ? "Ancient Pyramid internal routing deferred"
          : "Destination unresolved";
    const candidates = transition.roomPaths.length > 0 ? `\nCandidates: ${transition.roomPaths.join(", ")}` : "";
    return `\n${transition.tag ?? "Room object"}: ${detail}${candidates}`;
  }).join("");
}
