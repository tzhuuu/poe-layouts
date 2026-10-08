import type { LayoutGraph } from "./layoutGraph";
import type { NodeRotation, RotationDirection } from "./nodeRotation";

export type GraphWorkspaceState = {
  graph: LayoutGraph | null;
  hoveredNodeIndex: number | null;
  selectedNodeIndex: number | null;
  rotations: Partial<Record<number, NodeRotation>>;
  direction: RotationDirection;
};

export type GraphWorkspaceAction =
  | { type: "load"; graph: LayoutGraph }
  | { type: "hover"; index: number | null }
  | { type: "leave"; index: number }
  | { type: "select"; index: number | null }
  | { type: "rotate"; index: number; rotation: NodeRotation | undefined }
  | { type: "direction"; direction: RotationDirection };

export function initialGraphWorkspaceState(): GraphWorkspaceState {
  return { graph: null, hoveredNodeIndex: null, selectedNodeIndex: null, rotations: {}, direction: "counterclockwise" };
}

export function graphWorkspaceReducer(state: GraphWorkspaceState, action: GraphWorkspaceAction): GraphWorkspaceState {
  if (action.type === "load") return { ...initialGraphWorkspaceState(), graph: action.graph };
  if (action.type === "direction") return { ...state, direction: action.direction };
  if (action.type === "leave") return state.hoveredNodeIndex === action.index ? { ...state, hoveredNodeIndex: null } : state;
  if (action.index !== null && !state.graph?.nodes.some((node) => node.index === action.index)) return state;
  switch (action.type) {
    case "hover": return state.hoveredNodeIndex === action.index ? state : { ...state, hoveredNodeIndex: action.index };
    case "select": return { ...state, selectedNodeIndex: action.index, hoveredNodeIndex: null };
    case "rotate": {
      const rotations = { ...state.rotations };
      if (action.rotation === undefined) delete rotations[action.index];
      else rotations[action.index] = action.rotation;
      return { ...state, rotations };
    }
  }
}

export function inspectedGraphNode(state: GraphWorkspaceState) {
  const index = state.hoveredNodeIndex ?? state.selectedNodeIndex;
  return state.graph?.nodes.find((node) => node.index === index) ?? null;
}
