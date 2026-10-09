import type { LayoutGraph } from "./layoutGraph";

export type GraphPreviewState = {
  request: string | null;
  view: { graph: LayoutGraph; outdoor: boolean; request: string } | null;
  error: string | null;
};

export type GraphPreviewAction =
  | { type: "clear" }
  | { type: "request"; request: string }
  | { type: "ready"; request: string; graph: LayoutGraph; outdoor: boolean }
  | { type: "error"; request: string; message: string };

export function initialGraphPreviewState(): GraphPreviewState {
  return { request: null, view: null, error: null };
}

export function graphPreviewReducer(state: GraphPreviewState, action: GraphPreviewAction): GraphPreviewState {
  if (action.type === "clear") return initialGraphPreviewState();
  if (action.type === "request") return { ...state, request: action.request, error: null };
  if (action.request !== state.request) return state;
  if (action.type === "error") return { ...state, error: action.message };
  return { request: action.request, view: { graph: action.graph, outdoor: action.outdoor, request: action.request }, error: null };
}
