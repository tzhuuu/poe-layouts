export type LayoutEnvironmentKind = "indoor" | "outdoor" | "mixed" | "unknown";

export type LayoutEnvironment = {
  logicalPath: string;
  environment: LayoutEnvironmentKind;
  basis: string;
  masterFile: string | null;
  roomSet: string | null;
  outerGroundType: string | null;
  evidence: string[];
};

export type LayoutEnvironmentIndex = {
  patchVersion: string;
  layouts: LayoutEnvironment[];
  warnings: string[];
};

export async function loadLayoutEnvironments(): Promise<LayoutEnvironmentIndex> {
  const response = await fetch("/api/layout-environments", { cache: "no-store" });
  if (!response.ok) throw new Error(`Could not load environment classifications (${response.status})`);
  if (!response.headers.get("content-type")?.includes("application/json")) {
    throw new Error("Environment classifications API did not return JSON");
  }
  const raw = await response.json() as {
    patch_version: string;
    layouts: {
      logical_path: string;
      environment: LayoutEnvironmentKind;
      basis: string;
      master_file: string | null;
      room_set: string | null;
      outer_ground_type: string | null;
      evidence: string[];
    }[];
    warnings: string[];
  };
  return {
    patchVersion: raw.patch_version,
    warnings: raw.warnings,
    layouts: raw.layouts.map((layout) => ({
      logicalPath: layout.logical_path,
      environment: layout.environment,
      basis: layout.basis,
      masterFile: layout.master_file,
      roomSet: layout.room_set,
      outerGroundType: layout.outer_ground_type,
      evidence: layout.evidence,
    })),
  };
}
