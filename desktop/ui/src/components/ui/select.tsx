import { ChevronDown } from "lucide-react";
import { cn } from "@/lib/utils";

/** Native select styled like Input (keyboard and screen reader friendly). */
export function Select({ className, children, ...p }: React.SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <div className={cn("relative inline-block", className)}>
      <select
        className="h-[30px] w-full cursor-default appearance-none rounded-md border border-border-strong bg-transparent pl-2.5 pr-7 text-base text-fg outline-none transition-colors hover:border-fg-faint/60 focus-visible:border-fg-muted disabled:opacity-50 [&>option]:bg-surface"
        {...p}
      >
        {children}
      </select>
      <ChevronDown className="pointer-events-none absolute right-2 top-1/2 size-3 -translate-y-1/2 text-fg-faint" />
    </div>
  );
}
