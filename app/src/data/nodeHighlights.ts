import type { LayoutGraphNode } from "./layoutGraph";

export const nodeRoles = {
  waypoint: { label: "Waypoint", color: "#63cafa" },
  entrance: { label: "Entrance", color: "#6fdfab" },
  exit: { label: "Exit", color: "#fbab65" },
  sideArea: { label: "Side area", color: "#cf9cf8" },
  boss: { label: "Boss", color: "#ff7b85" },
  room: { label: "Named room", color: "#e8ca70" },
  structural: { label: "Structural", color: "#b2c4bf" },
  void: { label: "Void", color: "#586761" },
} as const;

export function nodeRole(node: LayoutGraphNode): keyof typeof nodeRoles {
  const label = node.label?.toLowerCase() ?? "";
  const identifiers = nodeIdentifiers(node);
  if (identifiers.some((tag) => tag.includes("waypoint"))) return "waypoint";
  if (identifiers.some((tag) => tag.includes("sidearea") || tag.includes("sideentrance"))) return "sideArea";
  if (nodeHasBoss(node)) return "boss";
  if (label.includes("exit")) return "exit";
  if (identifiers.some((tag) => tag.includes("entrance"))) return "entrance";
  if (node.label) return "room";
  return node.metadata.at(-1) === "V" ? "void" : "structural";
}

export function nodeHasBoss(node: LayoutGraphNode): boolean {
  return Boolean(node.roomBosses?.roomPaths.length) || nodeIdentifiers(node).some((identifier) => identifier.includes("boss"));
}

export function nodeBossLabel(node: LayoutGraphNode): string {
  return node.roomBosses?.tags.includes("mapboss") || nodeIdentifiers(node).some((identifier) => identifier.includes("mapboss"))
    ? "Map boss"
    : "Boss";
}

export function nodeBossDetails(node: LayoutGraphNode): string {
  if (!node.roomBosses) return `Boss indicated by room ${node.index}'s label or active metadata tags`;
  return `Possible ${node.roomBosses.tags.join(", ")} spawn: ${node.roomBosses.roomPaths.length} of ${node.roomBosses.candidateCount} ARM variants\n${node.roomBosses.roomPaths.join("\n")}`;
}

function nodeIdentifiers(node: LayoutGraphNode): string[] {
  const tags = node.metadata
    .map((tag) => tag.toLowerCase())
    .filter((tag) => !tag.includes("disabled"));
  return [node.label?.toLowerCase() ?? "", ...tags];
}
