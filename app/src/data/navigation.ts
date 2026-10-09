export type WorkspaceTab = "view" | "files";
export type ViewerKind = "layout" | "room";

export type ExplorerNavigation = {
  zone: string | null;
  tab: WorkspaceTab;
  view: ViewerKind;
  layout: string | null;
  room: string | null;
  variant: string | null;
};

export function isWorkspaceTab(value: string | null): value is WorkspaceTab {
  return value === "view" || value === "files";
}

export function readNavigation(url: URL): ExplorerNavigation {
  const tab = url.searchParams.get("tab");
  const view = url.searchParams.get("view");
  return {
    zone: url.searchParams.get("zone") || null,
    tab: isWorkspaceTab(tab) ? tab : "view",
    view: view === "layout" || view === "room" ? view : tab === "rooms" ? "room" : "layout",
    layout: url.searchParams.get("layout") || null,
    room: url.searchParams.get("room") || null,
    variant: url.searchParams.get("variant") || null,
  };
}

export function updateNavigation(
  current: ExplorerNavigation,
  patch: Partial<ExplorerNavigation>,
): ExplorerNavigation {
  const next = { ...current, ...patch };
  if (next.zone !== current.zone) {
    next.view = patch.view ?? "layout";
    next.layout = patch.layout ?? null;
    next.room = patch.room ?? null;
    next.variant = patch.variant ?? null;
  } else if (next.room !== current.room) {
    next.variant = patch.variant ?? null;
  }
  return next;
}

export function navigationUrl(url: URL, navigation: ExplorerNavigation): string {
  const next = new URL(url);
  for (const key of ["zone", "tab", "view", "layout", "room", "variant"] as const) {
    const value = navigation[key];
    if (value === null) next.searchParams.delete(key);
    else next.searchParams.set(key, value);
  }
  return `${next.pathname}${next.search}${next.hash}`;
}
