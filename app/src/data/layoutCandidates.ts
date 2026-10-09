export type LayoutCandidate = {
  logicalPath: string;
  group: string[];
  rootPaths: string[];
};

export type LayoutCandidates = {
  candidates: LayoutCandidate[];
  warnings: string[];
};

export async function loadLayoutCandidates(paths: string[], signal: AbortSignal): Promise<LayoutCandidates> {
  const response = await fetch("/api/layout-candidates", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ paths }),
    signal,
  });
  if (!response.ok) throw new Error(`Could not resolve layouts (${response.status})`);
  if (!response.headers.get("content-type")?.includes("application/json")) {
    throw new Error("Could not resolve layouts: the API did not return JSON");
  }
  const raw = await response.json() as {
    candidates: { logical_path: string; group: string[]; root_paths: string[] }[];
    warnings: string[];
  };
  return {
    candidates: raw.candidates.map((candidate) => ({
      logicalPath: candidate.logical_path,
      group: candidate.group,
      rootPaths: candidate.root_paths,
    })),
    warnings: raw.warnings,
  };
}

export function selectLayoutCandidate(candidates: LayoutCandidate[], path: string | null): LayoutCandidate | null {
  return candidates.find((candidate) => candidate.logicalPath === path)
    ?? candidates.find((candidate) => candidate.rootPaths.includes(path ?? ""))
    ?? candidates[0] ?? null;
}

export function groupLayoutCandidates(candidates: LayoutCandidate[]): { label: string; candidates: LayoutCandidate[] }[] {
  const groups = new Map<string, LayoutCandidate[]>();
  for (const candidate of candidates) {
    const label = candidate.group.join(" / ");
    const items = groups.get(label) ?? [];
    items.push(candidate);
    groups.set(label, items);
  }
  return [...groups].map(([label, candidates]) => ({ label, candidates }));
}
