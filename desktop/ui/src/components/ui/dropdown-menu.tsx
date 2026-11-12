import { DropdownMenu as M } from "radix-ui";
import { Check, ChevronDown } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "./button";

export const DropdownMenu = M.Root;
export const DropdownMenuTrigger = M.Trigger;

export function DropdownMenuContent({ className, align = "start", ...props }: React.ComponentProps<typeof M.Content>) {
  return (
    <M.Portal>
      <M.Content
        align={align}
        sideOffset={6}
        collisionPadding={12}
        className={cn("anim-pop z-50 min-w-[180px] overflow-hidden rounded-lg border border-border bg-surface p-1 text-base text-fg shadow-popover", className)}
        {...props}
      />
    </M.Portal>
  );
}

const itemCls =
  "relative flex h-7 cursor-default items-center gap-2 rounded-[4px] px-2 text-base outline-none select-none data-[highlighted]:bg-muted data-[disabled]:opacity-50 [&_svg]:size-3.5 [&_svg]:text-fg-muted";

export function DropdownMenuItem({ className, ...props }: React.ComponentProps<typeof M.Item>) {
  return <M.Item className={cn(itemCls, className)} {...props} />;
}

export function DropdownMenuRadioItem({ className, children, ...props }: React.ComponentProps<typeof M.RadioItem>) {
  return (
    <M.RadioItem className={cn(itemCls, "pr-7", className)} {...props}>
      {children}
      <M.ItemIndicator className="absolute right-2 flex items-center">
        <Check className="!text-fg" />
      </M.ItemIndicator>
    </M.RadioItem>
  );
}
export const DropdownMenuRadioGroup = M.RadioGroup;
export const DropdownMenuSeparator = ({ className, ...p }: React.ComponentProps<typeof M.Separator>) => (
  <M.Separator className={cn("-mx-1 my-1 h-px bg-border", className)} {...p} />
);
export const DropdownMenuLabel = ({ className, ...p }: React.ComponentProps<typeof M.Label>) => (
  <M.Label className={cn("px-2 py-1 text-sm text-fg-faint", className)} {...p} />
);

/** Compact filter dropdown, e.g. "Quelle  Alle". */
export function FilterMenu({ label, value, options, onChange }: {
  label: string; value: string; options: { value: string; label: string }[]; onChange: (v: string) => void;
}) {
  const current = options.find((o) => o.value === value);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" className="gap-1.5 px-2.5 font-normal data-[state=open]:bg-muted">
          <span className="text-fg-muted">{label}</span>
          <span className="text-fg">{current?.label ?? "Alle"}</span>
          <ChevronDown className="!size-3 text-fg-faint" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent className="max-h-[320px] overflow-y-auto">
        <DropdownMenuRadioGroup value={value} onValueChange={onChange}>
          {options.map((o) => (
            <DropdownMenuRadioItem key={o.value} value={o.value}>{o.label}</DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
