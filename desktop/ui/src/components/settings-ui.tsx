import { cn } from "@/lib/utils";

export function Section({ id, title, intro, children }: { id: string; title: string; intro?: React.ReactNode; children: React.ReactNode }) {
  return (
    <section id={`s-${id}`} className="scroll-mt-6">
      <h2 className="border-b border-border pb-2 text-lg font-semibold">{title}</h2>
      {intro && <p className="border-b border-border py-3 text-base text-fg-muted">{intro}</p>}
      <div className="flex flex-col">{children}</div>
    </section>
  );
}

export function Row({ label, hint, children, stack }: { label: string; hint?: React.ReactNode; children: React.ReactNode; stack?: boolean }) {
  return (
    <div className={cn("flex gap-6 border-b border-border py-3 last:border-b-0", stack ? "flex-col gap-2" : "items-center justify-between max-md:flex-col max-md:items-stretch max-md:gap-2")}>
      <div className="min-w-0 max-w-[460px]">
        <div className="text-base">{label}</div>
        {hint && <div className="mt-0.5 text-sm text-fg-muted">{hint}</div>}
      </div>
      <div className={cn("shrink-0", stack ? "w-full" : "flex w-[300px] justify-end max-md:w-full max-md:justify-start")}>{children}</div>
    </div>
  );
}
