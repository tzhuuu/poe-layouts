type Point = { x: number; y: number };
type Projection = { a: number; b: number; c: number; d: number };

export function graphProjection(outdoor: boolean, scale = 1): Projection {
  const angle = -Math.PI / 4;
  const cos = Math.cos(angle) * scale;
  const sin = Math.sin(angle) * scale;
  return { a: cos, b: sin, c: outdoor ? sin : -sin, d: outdoor ? -cos : cos };
}

export function projectGraphPoint(point: Point, projection: Projection): Point {
  return {
    x: point.x * projection.a + point.y * projection.c,
    y: point.x * projection.b + point.y * projection.d,
  };
}

export function graphOrientationDegrees(sourceDegrees: number, outdoor: boolean): number {
  return outdoor ? 135 - sourceDegrees : sourceDegrees - 45;
}
