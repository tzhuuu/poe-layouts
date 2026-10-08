export function adjacentLayout(
  paths: readonly string[],
  selectedPath: string | null,
  direction: -1 | 1,
): string | null {
  const index = selectedPath === null ? -1 : paths.indexOf(selectedPath);
  if (index < 0) return null;
  return paths[index + direction] ?? null;
}

export type LayoutArrowKey = "ArrowLeft" | "ArrowRight";
export type LayoutKeyRepeat = { key: LayoutArrowKey; nextStepAt: number };

export function layoutKeyStep(
  previous: LayoutKeyRepeat | null,
  key: LayoutArrowKey,
  repeat: boolean,
  now: number,
): { step: boolean; state: LayoutKeyRepeat | null } {
  if (!repeat) return { step: true, state: { key, nextStepAt: now + 450 } };
  if (!previous || previous.key !== key || now < previous.nextStepAt) {
    return { step: false, state: previous };
  }
  return { step: true, state: { key, nextStepAt: now + 300 } };
}
