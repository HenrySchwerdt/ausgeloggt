import { Command as C } from "cmdk";
import { Search } from "lucide-react";
import { Dialog as D } from "radix-ui";
import { cn } from "@/lib/utils";

export function CommandDialog({ open, onOpenChange, children }: { open: boolean; onOpenChange: (o: boolean) => void; children: React.ReactNode }) {
  return (
    <D.Root open={open} onOpenChange={onOpenChange}>
      <D.Portal>
        <D.Overlay className="anim-fade fixed inset-0 z-50 bg-[var(--overlay)]" />
        <D.Content
          aria-describedby={undefined}
          className="anim-pop fixed left-1/2 top-[16vh] z-50 w-[calc(100%-32px)] max-w-[560px] -translate-x-1/2 overflow-hidden rounded-lg border border-border bg-surface shadow-popover outline-none"
        >
          <D.Title className="sr-only">Befehlspalette</D.Title>
          <C label="Befehlspalette" className="flex flex-col">{children}</C>
        </D.Content>
      </D.Portal>
    </D.Root>
  );
}

export function CommandInput({ className, ...props }: React.ComponentProps<typeof C.Input>) {
  return (
    <div className="flex h-11 items-center gap-2.5 border-b border-border px-4">
      <Search className="size-4 shrink-0 text-fg-faint" />
      <C.Input className={cn("h-full flex-1 bg-transparent text-lg text-fg outline-none placeholder:text-fg-faint select-text", className)} {...props} />
    </div>
  );
}

export const CommandList = ({ className, ...p }: React.ComponentProps<typeof C.List>) => (
  <C.List className={cn("max-h-[360px] overflow-y-auto p-1.5", className)} {...p} />
);
export const CommandEmpty = (p: React.ComponentProps<typeof C.Empty>) => (
  <C.Empty className="py-8 text-center text-base text-fg-muted" {...p} />
);
export const CommandGroup = ({ className, ...p }: React.ComponentProps<typeof C.Group>) => (
  <C.Group
    className={cn("[&_[cmdk-group-heading]]:px-2.5 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:pt-2 [&_[cmdk-group-heading]]:text-sm [&_[cmdk-group-heading]]:text-fg-faint", className)}
    {...p}
  />
);
export const CommandItem = ({ className, ...p }: React.ComponentProps<typeof C.Item>) => (
  <C.Item
    className={cn("flex h-9 cursor-default items-center gap-2.5 rounded-md px-2.5 text-base text-fg outline-none data-[selected=true]:bg-muted [&_svg]:size-4 [&_svg]:text-fg-muted", className)}
    {...p}
  />
);
