export const NODE_ROTATIONS = ["I", "R90", "R180", "R270"] as const;
export type NodeRotation = typeof NODE_ROTATIONS[number];
export type RotationDirection = "clockwise" | "counterclockwise";
export type NodePoint = { x: number; y: number };
export type NodeCanvas = { width: number; height: number };

export function nodeCanvas(width: number | null, height: number | null): NodeCanvas | null {
  if (width === null || height === null || width <= 0 || height <= 0
    || !Number.isFinite(width * 24) || !Number.isFinite(height * 24)) return null;
  return { width: width * 24, height: height * 24 };
}

export function nodeRotationDegrees(rotation: NodeRotation, direction: RotationDirection = "counterclockwise"): number {
  if (rotation === "I") return 0;
  const degrees = Number(rotation.slice(1));
  return direction === "clockwise" ? degrees : -degrees;
}

export function rotateNodePoint(point: NodePoint, canvas: NodeCanvas, rotation: NodeRotation, direction: RotationDirection = "counterclockwise"): NodePoint {
  switch (rotation) {
    case "I": return { x: point.x, y: point.y };
    case "R90": return direction === "clockwise"
      ? { x: canvas.height - point.y, y: point.x }
      : { x: point.y, y: canvas.width - point.x };
    case "R180": return { x: canvas.width - point.x, y: canvas.height - point.y };
    case "R270": return direction === "clockwise"
      ? { x: point.y, y: canvas.width - point.x }
      : { x: canvas.height - point.y, y: point.x };
  }
}
