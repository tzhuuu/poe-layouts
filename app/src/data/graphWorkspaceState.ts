import type { LayoutGraph } from "./layoutGraph";

export type GraphWorkspaceState = {
  graph: LayoutGraph | null;
  hoveredNodeIndex: number | null;
  selectedNodeIndex: number | null;
};

export type GraphWorkspaceAction =
  | { type: "clear" }
  | { type: "load"; graph: LayoutGraph }
  | { type: "hover"; index: number | null }
  | { type: "leave"; index: number }
  | { type: "select"; index: number | null };

export function initialGraphWorkspaceState(): GraphWorkspaceState {
  return { graph: null, hoveredNodeIndex: null, selectedNodeIndex: null };
}

export function graphWorkspaceReducer(state: GraphWorkspaceState, action: GraphWorkspaceAction): GraphWorkspaceState {
  if (action.type === "clear") return initialGraphWorkspaceState();
  if (action.type === "load") return { ...initialGraphWorkspaceState(), graph: action.graph };
  if (action.type === "leave") return state.hoveredNodeIndex === action.index ? { ...state, hoveredNodeIndex: null } : state;
  if (action.index !== null && !state.graph?.nodes.some((node) => node.index === action.index)) return state;
  switch (action.type) {
    case "hover": return state.hoveredNodeIndex === action.index ? state : { ...state, hoveredNodeIndex: action.index };
    case "select": return { ...state, selectedNodeIndex: action.index, hoveredNodeIndex: null };
  }
}

export function inspectedGraphNode(state: GraphWorkspaceState) {
  const index = state.hoveredNodeIndex ?? state.selectedNodeIndex;
  return state.graph?.nodes.find((node) => node.index === index) ?? null;
}
