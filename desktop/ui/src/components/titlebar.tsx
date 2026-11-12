import { useEffect, useState } from "react";
import { ChevronRight, Copy, Minus, Search, Square, X } from "lucide-react";
import { inTauri } from "@/lib/api";
import { useStore } from "@/lib/store";
import { useRoute } from "@/lib/router";
import { cn } from "@/lib/utils";
import { PAGE_TITLE } from "./nav";
import { Kbd } from "./ui/kbd";

export type Platform = "mac" | "win" | "linux";

export function detectPlatform(): Platform {
  const q = new URLSearchParams(location.search).get("platform");
  if (q === "mac" || q === "win" || q === "linux") return q;
  const ua = navigator.userAgent;
  if (/Mac/i.test(ua)) return "mac";
  if (/Linux|X11/i.test(ua) && !/Windows/i.test(ua)) return "linux";
  return "win";
}
export const platform = detectPlatform();
export const modKey = platform === "mac" ? "⌘" : "Strg";

async function win() {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

function WindowControls() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!inTauri) return;
    let un: (() => void) | undefined;
    (async () => {
      const w = await win();
      setMaximized(await w.isMaximized());
      un = await w.onResized(async () => setMaximized(await w.isMaximized()));
    })();
    return () => un?.();
  }, []);

  const btn = "flex h-9 w-[46px] items-center justify-center text-fg-muted transition-colors hover:bg-muted hover:text-fg [&_svg]:size-[14px]";
  const run = (f: (w: Awaited<ReturnType<typeof win>>) => Promise<unknown>) => async () => {
    if (inTauri) await f(await win());
  };
  return (
    <div className="flex h-9 items-stretch">
      <button className={btn} aria-label="Minimieren" onClick={run((w) => w.minimize())}><Minus strokeWidth={1.5} /></button>
      <button className={btn} aria-label={maximized ? "Wiederherstellen" : "Maximieren"} onClick={run((w) => w.toggleMaximize())}>
        {maximized ? <Copy strokeWidth={1.5} className="!size-[12px] -scale-x-100" /> : <Square strokeWidth={1.5} className="!size-[12px]" />}
      </button>
      <button className={cn(btn, "hover:bg-[#e81123] hover:text-white")} aria-label="Schließen" onClick={run((w) => w.close())}><X strokeWidth={1.5} /></button>
    </div>
  );
}

/** Top bar of the content column: breadcrumb left, global search + window controls right. */
export function Titlebar({ minimal = false }: { minimal?: boolean }) {
  const route = useRoute();
  const setPalette = useStore((s) => s.setPalette);
  const snap = useStore((s) => s.snap);
  const account = route.page === "accounts" && route.id ? snap?.accounts.find((a) => a.id === route.id) : undefined;

  return (
    <header data-tauri-drag-region className="relative flex h-9 shrink-0 items-center border-b border-border bg-bg pl-4">
      <nav data-tauri-drag-region className={cn("pointer-events-none flex min-w-0 items-center gap-1.5 text-base", minimal && "invisible")}>
        <span className="text-fg-faint">Spurlos</span>
        <ChevronRight className="size-3 text-fg-faint" />
        <span className={cn("font-medium", account ? "text-fg-muted" : "text-fg")}>{PAGE_TITLE[route.page]}</span>
        {account && (
          <>
            <ChevronRight className="size-3 text-fg-faint" />
            <span className="truncate font-medium text-fg">{account.name}</span>
          </>
        )}
      </nav>
      <div data-tauri-drag-region className="flex-1 self-stretch" />
      {!minimal && <button
        onClick={() => setPalette(true)}
        className="mr-2 flex h-6 w-[220px] items-center gap-2 rounded-md bg-muted px-2 text-base text-fg-faint transition-colors hover:bg-muted-hover max-md:w-6 max-md:justify-center max-md:px-0"
      >
        <Search className="size-3.5" />
        <span className="flex-1 text-left max-md:hidden">Suchen …</span>
        <span className="flex gap-0.5 max-md:hidden"><Kbd className="h-4 border-0 bg-transparent px-0.5">{modKey}</Kbd><Kbd className="h-4 border-0 bg-transparent px-0.5">K</Kbd></span>
      </button>}
      {platform !== "mac" ? <WindowControls /> : <div className="w-2" />}
    </header>
  );
}
