import { useEffect } from "react";
import { CornerDownLeft } from "lucide-react";
import { useStore } from "@/lib/store";
import { navigate } from "@/lib/router";
import { ACCOUNT_STATUS_LABEL } from "@/lib/types";
import { NAV } from "./nav";
import { ServiceLogo, Dot, STATUS_COLOR } from "./common";
import { CommandDialog, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "./ui/command";
import { Kbd } from "./ui/kbd";

export function CommandPalette() {
  const open = useStore((s) => s.paletteOpen);
  const setOpen = useStore((s) => s.setPalette);
  const snap = useStore((s) => s.snap);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        useStore.getState().setPalette(!useStore.getState().paletteOpen);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const go = (fn: () => void) => { setOpen(false); fn(); };

  return (
    <CommandDialog open={open} onOpenChange={setOpen}>
      <CommandInput placeholder="Account oder Seite suchen …" />
      <CommandList>
        <CommandEmpty>Nichts gefunden.</CommandEmpty>
        <CommandGroup heading="Seiten">
          {NAV.map(({ page, label, icon: Icon }) => (
            <CommandItem key={page} value={`seite ${label}`} onSelect={() => go(() => navigate({ page } as never))}>
              <Icon strokeWidth={1.75} /> {label}
            </CommandItem>
          ))}
        </CommandGroup>
        {snap && (
          <CommandGroup heading="Accounts">
            {snap.accounts.map((a) => (
              <CommandItem
                key={a.id}
                value={`${a.name} ${a.domain} ${a.identifiers.join(" ")}`}
                onSelect={() => go(() => navigate({ page: "accounts", id: a.id }))}
              >
                <ServiceLogo name={a.name} domain={a.domain} size={20} className="!size-5" />
                <span className="font-medium">{a.name}</span>
                <span className="flex-1 truncate text-fg-faint">{a.domain}</span>
                <span className="flex items-center gap-1.5 text-sm text-fg-muted">
                  <Dot className={STATUS_COLOR[a.status]} />{ACCOUNT_STATUS_LABEL[a.status]}
                </span>
              </CommandItem>
            ))}
          </CommandGroup>
        )}
      </CommandList>
      <div className="flex h-9 items-center gap-4 border-t border-border px-4 text-sm text-fg-faint">
        <span className="flex items-center gap-1.5"><Kbd>↑</Kbd><Kbd>↓</Kbd> Navigieren</span>
        <span className="flex items-center gap-1.5"><Kbd><CornerDownLeft className="size-3" /></Kbd> Öffnen</span>
        <span className="ml-auto flex items-center gap-1.5"><Kbd>Esc</Kbd> Schließen</span>
      </div>
    </CommandDialog>
  );
}
