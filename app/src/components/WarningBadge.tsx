import type { ReactNode } from "react";
import { AlertTriangle } from "lucide-react";
import { Badge } from "./ui/badge";
import { HoverPopover } from "./ui/hover-popover";

export function WarningBadge({ warnings, label, children }: {
  warnings: string[];
  label: string;
  children?: ReactNode;
}) {
  if (warnings.length === 0) return null;

  return (
    <HoverPopover
      label={label}
      trigger={
        <button
          aria-label={`${label} (${warnings.length})`}
          className="inline-flex shrink-0 cursor-pointer rounded-md focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring"
          type="button"
        >
          <Badge variant="warning">
            <AlertTriangle aria-hidden="true" className="mr-1 size-3" />
            {children ?? `${warnings.length} ${warnings.length === 1 ? "warning" : "warnings"}`}
          </Badge>
        </button>
      }
    >
      {() => (
        <>
          <h3 className="border-b px-3 py-2 text-sm font-semibold">{label}</h3>
          <ul aria-label={`${label} messages`} className="max-h-[min(360px,60vh)] overflow-y-auto overscroll-contain px-3 focus-visible:outline focus-visible:outline-ring" tabIndex={0}>
            {warnings.map((warning, index) => (
              <li className="whitespace-pre-wrap break-words border-b py-3 text-xs leading-5 last:border-0 [overflow-wrap:anywhere]" key={index}>
                {warning}
              </li>
            ))}
          </ul>
        </>
      )}
    </HoverPopover>
  );
}
