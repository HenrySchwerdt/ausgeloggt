import { Tooltip as TooltipPrimitive } from "radix-ui";

export const TooltipProvider = ({ children }: { children: React.ReactNode }) => (
  <TooltipPrimitive.Provider delayDuration={350} skipDelayDuration={200}>{children}</TooltipPrimitive.Provider>
);

export function Tooltip({ content, children, side = "bottom", shortcut }: { content: React.ReactNode; children: React.ReactElement; side?: "top" | "bottom" | "left" | "right"; shortcut?: string }) {
  return (
    <TooltipPrimitive.Root>
      <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
      <TooltipPrimitive.Portal>
        <TooltipPrimitive.Content
          side={side}
          sideOffset={6}
          className="anim-fade z-50 flex items-center gap-2 rounded-md border border-border bg-surface px-2 py-1 text-sm text-fg shadow-popover"
        >
          {content}
          {shortcut && <span className="text-fg-faint">{shortcut}</span>}
        </TooltipPrimitive.Content>
      </TooltipPrimitive.Portal>
    </TooltipPrimitive.Root>
  );
}
