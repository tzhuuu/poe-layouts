import { createContext, useContext, useReducer, type Dispatch, type ReactNode } from "react";
import { graphWorkspaceReducer, initialGraphWorkspaceState, type GraphWorkspaceAction, type GraphWorkspaceState } from "../data/graphWorkspaceState";

const GraphWorkspaceContext = createContext<{
  state: GraphWorkspaceState;
  dispatch: Dispatch<GraphWorkspaceAction>;
} | null>(null);

export function GraphWorkspaceProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(graphWorkspaceReducer, undefined, initialGraphWorkspaceState);
  return <GraphWorkspaceContext.Provider value={{ state, dispatch }}>{children}</GraphWorkspaceContext.Provider>;
}

export function useGraphWorkspace() {
  const workspace = useContext(GraphWorkspaceContext);
  if (!workspace) throw new Error("Graph workspace is unavailable");
  return workspace;
}
