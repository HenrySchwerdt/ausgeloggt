import { cn } from "@/lib/utils";

export function Kbd({ className, ...props }: React.HTMLAttributes<HTMLElement>) {
  return (
    <kbd
      className={cn(
        "inline-flex h-[18px] min-w-[18px] items-center justify-center rounded-[4px] border border-border-strong bg-bg/40 px-1 font-sans text-xs font-medium text-fg-muted",
        className,
      )}
      {...props}
    />
  );
}
