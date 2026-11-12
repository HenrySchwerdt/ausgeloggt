import { cn } from "@/lib/utils";

export interface SegmentOption<T extends string> { value: T; label: string; count?: number }

/** Segment control: muted container, active segment on bg. */
export function Segmented<T extends string>({ value, options, onChange, className }: {
  value: T; options: SegmentOption<T>[]; onChange: (v: T) => void; className?: string;
}) {
  return (
    <div role="tablist" className={cn("inline-flex h-[30px] items-center gap-0.5 rounded-md bg-muted p-[3px]", className)}>
      {options.map((o) => {
        const active = o.value === value;
        return (
          <button
            key={o.value}
            role="tab"
            aria-selected={active}
            onClick={() => onChange(o.value)}
            className={cn(
              "flex h-6 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-[4px] px-2 text-base font-medium transition-colors",
              active ? "bg-bg text-fg shadow-[0_0_0_1px_var(--border)]" : "text-fg-muted hover:text-fg",
            )}
          >
            {o.label}
            {o.count !== undefined && <span className={cn("tnum text-sm font-normal", active ? "text-fg-muted" : "hidden text-fg-faint @[720px]:inline")}>{o.count}</span>}
          </button>
        );
      })}
    </div>
  );
}
