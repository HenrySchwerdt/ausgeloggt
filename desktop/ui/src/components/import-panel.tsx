import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { Check, ExternalLink, FileText, FileUp, Loader2, X } from "lucide-react";
import { api, runJob } from "@/lib/api";
import { useStore } from "@/lib/store";
import { openUrl, pickFiles } from "@/lib/platform";
import { useDrop } from "@/lib/drop";
import { PASSWORD_MANAGERS, type PasswordManager } from "@/lib/guides";
import type { ImportDetection, ImportDonePayload } from "@/lib/types";
import { cn, de, errorMessage, plural } from "@/lib/utils";
import { Dot, ServiceLogo } from "./common";
import { Button } from "./ui/button";

interface Item {
  id: number;
  path: string;
  name: string;
  state: "detecting" | "ready" | "unknown" | "importing" | "done" | "error";
  label?: string;
  note?: string;
  entries?: number;
  found?: number;
}

const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;
let nextId = 1;

function ManagerLogo({ m, size = 24 }: { m: PasswordManager; size?: number }) {
  if (!m.logo && !m.domain) return <span className="flex shrink-0 items-center justify-center rounded-md bg-muted text-fg-muted" style={{ width: size, height: size }} aria-hidden><FileText className="size-3.5" /></span>;
  return <ServiceLogo name={m.logo ?? m.name} domain={m.logo ? `${m.logo}.com` : m.domain || "x.invalid"} size={size} />;
}

/** Password exports from one or several managers: pick a manager, follow its steps, add files, import. */
export function ImportPanel({ onImported }: { onImported?(files: number): void }) {
  const checkPwned = useStore((s) => s.snap?.settings.check_pwned_passwords ?? false);
  const [manager, setManager] = useState<PasswordManager | null>(null);
  const [items, setItems] = useState<Item[]>([]);
  const [busy, setBusy] = useState(false);
  const pending = useDrop((s) => s.pending);
  const seen = useRef(new Set<string>());

  const patch = (id: number, p: Partial<Item>) => setItems((l) => l.map((x) => (x.id === id ? { ...x, ...p } : x)));

  async function add(paths: string[]) {
    const fresh = paths.filter((p) => !seen.current.has(p));
    fresh.forEach((p) => seen.current.add(p));
    const list: Item[] = fresh.map((p) => ({ id: nextId++, path: p, name: fileName(p), state: "detecting" }));
    if (!list.length) return;
    setItems((l) => [...l, ...list]);
    for (const it of list) {
      try {
        const d = (await api.imports.detect(it.path)) as ImportDetection | null;
        patch(it.id, d ? { state: "ready", label: de(d.label_de) } : { state: "unknown", note: "Nicht erkannt" });
      } catch (e) { patch(it.id, { state: "unknown", note: errorMessage(e) }); }
    }
  }

  useEffect(() => {
    if (!pending.length) return;
    const p = pending;
    useDrop.getState().clear();
    void add(p);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pending]);

  const choose = async () => {
    try { const p = await pickFiles("passwords"); if (p.length) void add(p); } catch (e) { toast.error(errorMessage(e)); }
  };

  const remove = (it: Item) => { seen.current.delete(it.path); setItems((l) => l.filter((x) => x.id !== it.id)); };

  const ready = items.filter((i) => i.state === "ready");
  const done = items.filter((i) => i.state === "done");

  async function run() {
    setBusy(true);
    for (const it of ready) {
      patch(it.id, { state: "importing" });
      try {
        const d = await runJob<ImportDonePayload>(() => api.imports.start([it.path], checkPwned));
        if (d.ok && d.payload) patch(it.id, { state: "done", entries: d.payload.imported, found: d.payload.newAccounts });
        else patch(it.id, { state: "error", note: d.error?.message ?? d.summary });
      } catch (e) { patch(it.id, { state: "error", note: errorMessage(e) }); }
    }
    setBusy(false);
  }

  useEffect(() => { onImported?.(done.length); }, [done.length, onImported]);

  return (
    <div className="flex flex-col gap-4">
      {items.length > 0 && (
        <div className="overflow-hidden rounded-lg border border-border">
          {items.map((it) => (
            <div key={it.id} className="anim-fade flex min-h-11 items-center gap-3 border-b border-border px-3 py-1.5 last:border-b-0">
              <div className="min-w-0 flex-1">
                <div className="truncate text-base font-medium leading-[18px]" title={it.path}>{it.name}</div>
                <div className="truncate text-sm leading-4 text-fg-muted">
                  {it.state === "detecting" && "Wird erkannt"}
                  {it.state === "ready" && `${it.label}, bereit zum Import`}
                  {it.state === "unknown" && (it.note ?? "Nicht erkannt")}
                  {it.state === "importing" && `${it.label}, wird gelesen`}
                  {it.state === "done" && `${it.label}, ${plural(it.entries ?? 0, "Eintrag", "Einträge")} gelesen, ${plural(it.found ?? 0, "neuer Fund", "neue Funde")}`}
                  {it.state === "error" && (it.note ?? "Import fehlgeschlagen")}
                </div>
              </div>
              {it.state === "detecting" || it.state === "importing" ? <Loader2 className="size-3.5 animate-spin text-fg-muted" />
                : it.state === "done" ? <Check className="size-4 text-s-green" />
                  : <Button variant="ghost" size="icon-sm" aria-label={`${it.name} entfernen`} onClick={() => remove(it)}><X /></Button>}
            </div>
          ))}
        </div>
      )}

      {done.length > 0 && (
        <p className="anim-fade flex items-start gap-2 text-base text-fg-muted">
          <Dot className="mt-[7px] bg-s-yellow" />Lösche die Exportdateien jetzt. Sie enthalten deine Passwörter im Klartext.
        </p>
      )}

      {!manager ? (
        <div className="flex flex-col gap-2.5">
          <p className="text-sm text-fg-faint">{items.length ? "Noch eine Datei aus einem anderen Manager?" : "Aus welchem Passwortmanager kommt deine Datei?"}</p>
          <div className="grid grid-cols-3 gap-2 max-[560px]:grid-cols-2">
            {PASSWORD_MANAGERS.map((m) => (
              <button key={m.id} onClick={() => setManager(m)} className="flex h-[52px] items-center gap-2.5 rounded-lg border border-border px-3 text-left text-base font-medium transition-colors hover:bg-muted">
                <ManagerLogo m={m} />
                <span className="min-w-0 flex-1 truncate">{m.name}</span>
              </button>
            ))}
          </div>
        </div>
      ) : (
        <div className="anim-fade flex flex-col gap-3">
          <div className="flex items-center gap-3">
            <ManagerLogo m={manager} size={28} />
            <span className="flex-1 text-lg font-medium">{manager.name}</span>
            <button onClick={() => setManager(null)} className="text-base text-fg-muted transition-colors hover:text-fg">Anderer Manager</button>
          </div>
          {done.length === 0 && <ol className="flex flex-col gap-2.5 rounded-lg bg-muted px-4 py-3.5">
            {manager.steps.map((s, i) => (
              <li key={s} className="flex gap-3 text-base"><span className="tnum w-3 shrink-0 text-fg-faint">{i + 1}</span><span>{de(s)}</span></li>
            ))}
            {manager.link && <li className="pl-6"><Button size="sm" variant="outline" onClick={() => void openUrl(manager.link!.url)}>{manager.link.label} <ExternalLink /></Button></li>}
          </ol>}
          <button onClick={() => void choose()} className={cn("flex flex-col items-center gap-1.5 rounded-lg border border-dashed border-border-strong px-4 py-7 text-center transition-colors hover:bg-muted")}>
            <FileUp className="size-5 text-fg-muted" strokeWidth={1.5} />
            <span className="text-base font-medium">{items.length ? "Weitere Datei auswählen oder ins Fenster ziehen" : "Datei auswählen oder ins Fenster ziehen"}</span>
          </button>
        </div>
      )}

      {ready.length > 0 && (
        <div className="flex justify-end">
          <Button variant="primary" disabled={busy} onClick={() => void run()}>{busy && <Loader2 className="animate-spin" />} {plural(ready.length, "Datei", "Dateien")} importieren</Button>
        </div>
      )}
    </div>
  );
}
