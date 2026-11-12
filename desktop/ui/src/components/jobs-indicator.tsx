import { Loader2, X } from "lucide-react";
import { Popover } from "radix-ui";
import { api } from "@/lib/api";
import { JOB_LABEL, useStore } from "@/lib/store";
import { Progress } from "./ui/progress";
import { Button } from "./ui/button";

/** Small indicator in the titlebar: running jobs with progress and cancel. */
export function JobsIndicator() {
  const jobs = Object.values(useStore((s) => s.jobs));
  if (!jobs.length) return null;
  const first = jobs[0]!;
  const frac = (j: typeof first) => (j.total > 0 ? j.done / j.total : undefined);
  const label = jobs.length === 1 ? JOB_LABEL[first.kind] : `${jobs.length} Vorgänge laufen`;
  const cancel = (id: string) => void api.jobs.cancel(id).catch(() => {});
  return (
    <Popover.Root>
      <Popover.Trigger asChild>
        <button className="mr-2 flex h-6 items-center gap-2 rounded-md px-2 text-sm text-fg-muted transition-colors hover:bg-muted hover:text-fg" aria-label="Laufende Vorgänge">
          <Loader2 className="size-3.5 animate-spin" />
          <span className="max-md:hidden">{label}</span>
          {jobs.length === 1 && frac(first) !== undefined && <span className="tnum text-fg-faint">{Math.round(frac(first)! * 100)}%</span>}
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content align="end" sideOffset={6} className="anim-pop z-50 w-[320px] rounded-lg border border-border bg-surface p-3 shadow-popover outline-none">
          <ul className="flex flex-col gap-3">
            {jobs.map((j) => (
              <li key={j.jobId} className="flex flex-col gap-1.5">
                <div className="flex items-center gap-2">
                  <span className="flex-1 text-base">{JOB_LABEL[j.kind]}</span>
                  <Button variant="ghost" size="sm" onClick={() => cancel(j.jobId)}><X /> Abbrechen</Button>
                </div>
                <Progress value={frac(j)} />
                {j.message && <p className="truncate text-sm text-fg-faint">{j.message}</p>}
              </li>
            ))}
          </ul>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
