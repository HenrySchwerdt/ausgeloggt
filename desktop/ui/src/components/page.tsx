import { cn } from "@/lib/utils";

/** Scrolling page with centred content column. */
export function PageShell({ children, className, wide }: { children: React.ReactNode; className?: string; wide?: boolean }) {
  return (
    <div className="h-full overflow-y-auto">
      <div className={cn("mx-auto flex w-full flex-col gap-7 px-7 py-7", wide ? "max-w-[1280px]" : "max-w-[1120px]", className)}>{children}</div>
    </div>
  );
}

/** Hairline separated list in a bordered container. */
export function Panel({ children, className }: { children: React.ReactNode; className?: string }) {
  return <div className={cn("overflow-hidden rounded-lg border border-border", className)}>{children}</div>;
}
