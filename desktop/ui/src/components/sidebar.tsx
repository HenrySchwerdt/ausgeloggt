import { Lock, Moon, Sun } from "lucide-react";
import { useStore } from "@/lib/store";
import { href, useRoute } from "@/lib/router";
import { cn } from "@/lib/utils";
import { isHidden } from "@/lib/derive";
import { NAV } from "./nav";
import { LogoMark, Dot } from "./common";
import { platform } from "./titlebar";
import { Button } from "./ui/button";
import { Tooltip } from "./ui/tooltip";

export function Sidebar() {
  const route = useRoute();
  const snap = useStore((s) => s.snap);
  const theme = useStore((s) => s.theme);
  const setTheme = useStore((s) => s.setTheme);
  const lock = useStore((s) => s.lock);

  const counts: Partial<Record<string, number>> = snap
    ? {
        accounts: snap.accounts.filter((a) => !isHidden(a)).length,
        newsletter: snap.subscriptions.filter((s) => s.status === "Active" || s.status === "StillReceiving").length,
        deletions: snap.accounts.filter((a) => a.status === "DeletionPending").length,
        sources: snap.runs.length ? new Set(snap.runs.map((r) => JSON.stringify(r.source))).size : 0,
      }
    : {};

  return (
    <aside className="flex w-[220px] shrink-0 flex-col border-r border-border bg-sidebar max-md:w-[196px]">
      <div data-tauri-drag-region className={cn("flex h-9 shrink-0 items-center gap-2 px-4", platform === "mac" && "pl-[78px]")}>
        <LogoMark size={20} className="pointer-events-none" />
        <span className="pointer-events-none text-lg font-semibold tracking-[-0.01em]">ausgeloggt</span>
      </div>

      <nav className="mt-3 flex flex-col gap-0.5 px-2">
        {NAV.map(({ page, label, icon: Icon }) => {
          const active = route.page === page;
          const count = counts[page];
          return (
            <a
              key={page}
              href={href({ page } as never)}
              draggable={false}
              className={cn(
                "flex h-[30px] items-center gap-2.5 rounded-md px-2.5 text-base transition-colors",
                active ? "bg-muted text-fg" : "text-fg-muted hover:bg-muted hover:text-fg",
              )}
            >
              <Icon className={cn("size-4", active ? "text-fg" : "text-fg-muted")} strokeWidth={1.75} />
              <span className="flex-1">{label}</span>
              {count ? <span className="tnum text-sm text-fg-faint">{count}</span> : null}
            </a>
          );
        })}
      </nav>

      <div className="mt-auto flex flex-col gap-1.5 border-t border-border p-2">
        <div className="flex h-6 items-center gap-2 px-2.5 text-sm text-fg-muted">
          <Dot className="bg-s-green" />
          Tresor entsperrt
        </div>
        <div className="flex items-center gap-1">
          <Button variant="ghost" className="flex-1 justify-start gap-2.5 px-2.5" onClick={() => void lock()}>
            <Lock className="!size-4" strokeWidth={1.75} /> Sperren
          </Button>
          <Tooltip content={theme === "dark" ? "Helles Design" : "Dunkles Design"} side="top">
            <Button variant="ghost" size="icon" aria-label="Design wechseln" onClick={() => setTheme(theme === "dark" ? "light" : "dark")}>
              {theme === "dark" ? <Sun strokeWidth={1.75} /> : <Moon strokeWidth={1.75} />}
            </Button>
          </Tooltip>
        </div>
      </div>
    </aside>
  );
}
