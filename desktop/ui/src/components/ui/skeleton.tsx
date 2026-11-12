import { cn } from "@/lib/utils";

export function Skeleton({ className }: { className?: string }) {
  return <div className={cn("animate-pulse rounded-md bg-muted", className)} />;
}

export function RowsSkeleton({ rows = 8, className }: { rows?: number; className?: string }) {
  return (
    <div className={cn("flex flex-col", className)} aria-busy>
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="flex h-11 items-center gap-3 border-b border-border px-7">
          <Skeleton className="size-6" />
          <Skeleton className="h-3 w-40" />
          <Skeleton className="ml-auto h-3 w-24" />
        </div>
      ))}
    </div>
  );
}
