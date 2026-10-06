import { useEffect, useRef, useState, type ReactElement, type ReactNode } from "react";
import { Popover, PopoverContent, PopoverTrigger } from "./popover";

export function HoverPopover({ trigger, label, children }: {
  trigger: ReactElement;
  label: string;
  children: (close: () => void) => ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const content = useRef<HTMLDivElement | null>(null);

  function cancelClose() {
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
  }

  function close() {
    cancelClose();
    setOpen(false);
  }

  function closeOnLeave(event: React.PointerEvent) {
    if (event.pointerType !== "mouse") return;
    cancelClose();
    timer.current = setTimeout(() => {
      if (!content.current?.contains(document.activeElement)) setOpen(false);
    }, 180);
  }

  useEffect(() => () => {
    if (timer.current !== null) clearTimeout(timer.current);
  }, []);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        asChild
        onClick={(event) => {
          if (open && event.detail > 0) event.preventDefault();
        }}
        onPointerEnter={(event) => {
          cancelClose();
          if (event.pointerType === "mouse") setOpen(true);
        }}
        onPointerLeave={closeOnLeave}
      >
        {trigger}
      </PopoverTrigger>
      <PopoverContent
        align="start"
        aria-label={label}
        className="w-[min(480px,calc(100vw-32px))] overflow-hidden p-0"
        onOpenAutoFocus={(event) => event.preventDefault()}
        onPointerEnter={cancelClose}
        onPointerLeave={closeOnLeave}
        ref={content}
      >
        {children(close)}
      </PopoverContent>
    </Popover>
  );
}
