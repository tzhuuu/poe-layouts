import { X } from "lucide-react";
import { inspectedGraphNode } from "../data/graphWorkspaceState";
import { nodeCanvas, rotateNodePoint } from "../data/nodeRotation";
import { nodeBossDetails, nodeHasBoss, nodeRole, nodeRoles } from "../data/nodeHighlights";
import { useGraphWorkspace } from "./GraphWorkspace";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";

export function GraphNodeDetails() {
  const { state, dispatch } = useGraphWorkspace();
  const node = inspectedGraphNode(state);
  if (!node || !state.graph) return <section aria-label="Node details" className="text-xs text-muted-foreground">No node selected</section>;
  const canvas = nodeCanvas(state.graph.width, state.graph.height);
  const rotation = state.rotations[node.index];
  const point = canvas && rotation ? rotateNodePoint(node, canvas, rotation, state.direction) : node;
  const neighbors = [...new Set(state.graph.edges.flatMap((edge) => edge.from === node.index ? [edge.to] : edge.to === node.index ? [edge.from] : []))];
  const mode = state.selectedNodeIndex === node.index ? "Selected" : "Hovered";
  const direction = state.direction === "clockwise" ? "CW" : "CCW";
  return (
    <section aria-label="Node details" className="space-y-3" data-inspected-node={node.index} data-inspection-mode={mode}>
      <header className="flex items-center justify-between gap-2">
        <h4 className="text-sm font-semibold">Node {node.index}</h4>
        <div className="flex items-center gap-1">
          <Badge variant="secondary">{mode}</Badge>
          {state.selectedNodeIndex !== null && <Button aria-label="Clear node selection" onClick={() => dispatch({ type: "select", index: null })} size="icon" title="Clear node selection" variant="ghost"><X /></Button>}
        </div>
      </header>
      {node.label && <p className="break-all font-mono text-xs">{node.label}</p>}
      <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-2 text-xs [&>dt]:text-muted-foreground [&>dd]:min-w-0 [&>dd]:break-words [&>dd]:text-right">
        <dt>Role</dt><dd>{nodeRoles[nodeRole(node)].label}</dd>
        <dt>Source rotation</dt><dd className="font-mono">{node.rotation ?? "any"}</dd>
        <dt>Test rotation</dt><dd className="font-mono">{rotation ? `${rotation} ${direction}` : "None"}</dd>
        <dt>Raw position</dt><dd className="font-mono">{node.x}, {node.y}</dd>
        <dt>Current position</dt><dd className="font-mono">{point.x}, {point.y}</dd>
        <dt>Canvas</dt><dd className="font-mono">{canvas ? `${canvas.width} x ${canvas.height}` : "Size unavailable"}</dd>
        <dt>Connected nodes</dt><dd>{neighbors.length ? neighbors.join(", ") : "None"}</dd>
      </dl>
      {node.transitions.length > 0 && <section className="space-y-2 border-t pt-3">
        <h5 className="text-xs font-semibold">Transitions</h5>
        {node.transitions.map((transition, index) => <div className="space-y-1 text-xs" key={index}>
          <p className="break-words font-medium">{transition.destination ? `${transition.destination.name} (${transition.destination.id})` : transition.kind}</p>
          {transition.tag && <p className="break-all font-mono text-muted-foreground">{transition.tag}</p>}
          {transition.objectPath && <p className="break-all font-mono text-muted-foreground">{transition.objectPath}</p>}
          <p className="break-words text-muted-foreground">{transition.basis}</p>
        </div>)}
      </section>}
      {nodeHasBoss(node) && <section className="space-y-2 border-t pt-3"><h5 className="text-xs font-semibold">Boss</h5><p className="whitespace-pre-wrap break-all text-xs text-muted-foreground">{nodeBossDetails(node)}</p></section>}
      <section className="space-y-2 border-t pt-3"><h5 className="text-xs font-semibold">Metadata</h5><p className="break-all font-mono text-xs text-muted-foreground">{node.metadata.join(" ") || "None"}</p></section>
    </section>
  );
}
