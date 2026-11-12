import { cn } from "@/lib/utils";

/** 4 px bar. `value` 0..1, undefined = indeterminate. */
export function Progress({ value, className }: { value?: number; className?: string }) {
  return (
    <div className={cn("relative h-1 overflow-hidden rounded-full bg-muted", className)} role="progressbar" aria-valuenow={value === undefined ? undefined : Math.round(value * 100)}>
      {value === undefined
        ? <div className="absolute inset-y-0 w-1/3 animate-[indet_1.2s_ease-in-out_infinite] rounded-full bg-fg-muted" />
        : <div className="h-full rounded-full bg-fg-muted transition-[width] duration-300" style={{ width: `${Math.max(2, Math.min(100, value * 100))}%` }} />}
    </div>
  );
}
