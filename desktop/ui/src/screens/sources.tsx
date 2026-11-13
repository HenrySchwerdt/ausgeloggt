import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Camera, ExternalLink, Inbox, FileUp, KeyRound, Loader2, MoreHorizontal, Plus, RefreshCw, X } from "lucide-react";
import { api, runJob } from "@/lib/api";
import { EVERY_LABEL } from "@/lib/automation";
import { useStore } from "@/lib/store";
import { navigate } from "@/lib/router";
import { IMAGE_EXT, openUrl, pickFiles } from "@/lib/platform";
import { useDrop } from "@/lib/drop";
import { useAsync } from "@/lib/hooks";
import {
  connectionsUrl, sourceLabel, ssoKey, ssoLabel, type ImportDetection, type ImportDonePayload, type JobProgress,
  type Mailbox, type ScanDonePayload, type ScreenshotEntry, type ScreenshotReadPayload, type SsoProvider,
} from "@/lib/types";
import { de, errorMessage, formatDate, nf, plural, relativeDays } from "@/lib/utils";
import { Dot, EmptyState, PageHeader, SectionLabel, ServiceLogo } from "@/components/common";
import { PageShell, Panel } from "@/components/page";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Progress } from "@/components/ui/progress";
import { confirm } from "@/components/confirm";
import { MailboxForm } from "@/components/mailbox-form";
import { ImportPanel } from "@/components/import-panel";

const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;

export function SourcesPage() {
  const snap = useStore((s) => s.snap)!;
  const jobs = useStore((s) => s.jobs);
  const [wizard, setWizard] = useState(false);
  const [panel, setPanel] = useState(false);
  const [scanning, setScanning] = useState(false);
  const [scanMsg, setScanMsg] = useState<JobProgress | null>(null);
  const [scanRes, setScanRes] = useState<ScanDonePayload | null>(null);
  const [importPaths, setImportPaths] = useState<string[] | null>(null);
  const [shots, setShots] = useState<string[] | null>(null);
  const pending = useDrop((s) => s.pending);
  const aiVision = snap.settings.ai.provider !== "Off";

  const scanJob = Object.values(jobs).find((j) => j.kind === "scan");

  async function scan(ids: string[], full = false) {
    setScanning(true); setScanRes(null); setScanMsg(null);
    try {
      const d = await runJob<ScanDonePayload>(() => api.mail.scanStart(ids, full), { onProgress: setScanMsg });
      if (d.payload) setScanRes(d.payload);
    } catch (e) { toast.error(errorMessage(e)); } finally { setScanning(false); setScanMsg(null); }
  }

  useEffect(() => {
    if (!pending.length || panel) return;
    const paths = pending;
    useDrop.getState().clear();
    if (paths.every((p) => IMAGE_EXT.test(p))) setShots(paths); else setImportPaths(paths);
  }, [pending, panel]);

  const choose = async (kind: "passwords" | "images") => {
    try {
      const p = await pickFiles(kind);
      if (!p.length) return;
      if (kind === "images") setShots(p); else setImportPaths(p);
    } catch (e) { toast.error(errorMessage(e)); }
  };

  const ssoMap = new Map<string, { p: SsoProvider; n: number }>();
  for (const a of snap.accounts) for (const p of a.sso) { const k = ssoKey(p); ssoMap.set(k, { p, n: (ssoMap.get(k)?.n ?? 0) + 1 }); }
  const providers = [...ssoMap.values()].sort((x, y) => y.n - x.n);

  return (
    <PageShell>
      <PageHeader title="Quellen" />

      <section>
        <SectionLabel right={<div className="flex gap-1">
          {snap.mailboxes.length > 1 && <Button variant="ghost" size="sm" disabled={scanning} onClick={() => void scan(snap.mailboxes.map((m) => m.id))}><RefreshCw /> Alle prüfen</Button>}
          <Button variant="secondary" size="sm" onClick={() => setWizard(true)}><Plus /> Postfach</Button>
        </div>}>Postfächer</SectionLabel>
        {(scanning || scanJob) && (
          <div className="mb-2 mt-1 rounded-md bg-muted px-3 py-2.5">
            <div className="flex items-center gap-2 text-base">
              <Loader2 className="size-3.5 animate-spin text-fg-muted" />
              <span className="min-w-0 flex-1 truncate">{scanMsg?.message ?? scanJob?.message ?? "Postfach wird geprüft"}</span>
              <Button variant="ghost" size="sm" onClick={() => scanJob && void api.jobs.cancel(scanJob.jobId)}><X /> Abbrechen</Button>
            </div>
            <Progress className="mt-2" value={scanMsg && scanMsg.total ? scanMsg.done / scanMsg.total : undefined} />
          </div>
        )}
        {scanRes && !scanning && (
          <div className="mb-2 mt-1 flex items-center gap-2 rounded-md bg-muted px-3 py-2 text-base">
            <Dot className={scanRes.failed ? "bg-s-red" : "bg-s-green"} />
            <span className="flex-1">{plural(scanRes.mails, "Mail", "Mails")} geprüft, {plural(scanRes.newAccounts, "neuer Account", "neue Accounts")}, {plural(scanRes.newSubscriptions, "neuer Newsletter", "neue Newsletter")}{scanRes.failed ? `, ${scanRes.failed} Postfach nicht erreichbar` : ""}</span>
            <Button variant="ghost" size="icon-sm" aria-label="Schließen" onClick={() => setScanRes(null)}><X /></Button>
          </div>
        )}
        <Panel className="mt-1">
          {snap.mailboxes.length === 0 ? (
            <EmptyState icon={Inbox} className="py-12" title="Kein Postfach verbunden"
              description="Mit einem Postfach findet ausgeloggt Accounts, Newsletter und Abos. Die Mails bleiben auf deinem Gerät."
              action={<Button variant="primary" onClick={() => setWizard(true)}>Postfach hinzufügen</Button>} />
          ) : snap.mailboxes.map((m) => <MailboxRow key={m.id} m={m} busy={scanning || !!scanJob} onScan={(full) => void scan([m.id], full)} />)}
        </Panel>
      </section>

      <section>
        <SectionLabel>Importe</SectionLabel>
        <div className="mt-1 grid grid-cols-3 gap-3 max-lg:grid-cols-1">
          <button onClick={() => setPanel(true)} className="flex flex-col items-start gap-2 rounded-lg border border-dashed border-border-strong px-4 py-4 text-left transition-colors hover:bg-muted">
            <KeyRound className="size-5 text-fg-muted" strokeWidth={1.5} />
            <span className="text-base font-medium">Passwortexport</span>
            <span className="text-sm text-fg-muted">Chrome, Firefox, Safari, Bitwarden, 1Password und mehr. Mit Anleitung für jeden Manager.</span>
          </button>
          <button onClick={() => void choose("passwords")} className="flex flex-col items-start gap-2 rounded-lg border border-dashed border-border-strong px-4 py-4 text-left transition-colors hover:bg-muted">
            <FileUp className="size-5 text-fg-muted" strokeWidth={1.5} />
            <span className="text-base font-medium">Takeout, Facebook oder Liste</span>
            <span className="text-sm text-fg-muted">Datei auswählen oder ins Fenster ziehen. Wir erkennen die Art selbst.</span>
          </button>
          <button onClick={() => aiVision ? void choose("images") : navigate({ page: "settings", section: "ai" })} className="flex flex-col items-start gap-2 rounded-lg border border-dashed border-border-strong px-4 py-4 text-left transition-colors hover:bg-muted">
            <Camera className="size-5 text-fg-muted" strokeWidth={1.5} />
            <span className="text-base font-medium">Screenshots lesen</span>
            <span className="text-sm text-fg-muted">{aiVision ? "Zum Beispiel die Liste unter Apple, Mit Apple anmelden. Du bestätigst jeden Treffer." : "Wähle dafür in den Einstellungen Claude, ChatGPT oder ein Ollama Bildmodell."}</span>
          </button>
        </div>
      </section>

      {providers.length > 0 && (
        <section>
          <SectionLabel>Anmeldung über Drittanbieter</SectionLabel>
          <Panel className="mt-1">
            {providers.map(({ p, n }) => {
              const url = connectionsUrl(p);
              return (
                <div key={ssoKey(p)} className="flex h-11 items-center gap-3 border-b border-border px-3 last:border-b-0">
                  <span className="min-w-0 flex-1 truncate text-base font-medium">{ssoLabel(p)}</span>
                  <span className="tnum text-sm text-fg-muted">{plural(n, "Account", "Accounts")}</span>
                  {url && <Button variant="secondary" onClick={() => void openUrl(url)}>Verbundene Apps öffnen <ExternalLink /></Button>}
                </div>
              );
            })}
          </Panel>
        </section>
      )}

      <section>
        <SectionLabel>Verlauf</SectionLabel>
        {snap.runs.length === 0 ? <p className="mt-2 text-base text-fg-muted">Noch kein Lauf.</p> : (
          <Panel className="mt-1">
            {[...snap.runs].sort((a, b) => (b.started_at > a.started_at ? 1 : -1)).slice(0, 30).map((r) => (
              <div key={r.id} className="flex min-h-11 items-center gap-3 border-b border-border px-3 py-1 last:border-b-0">
                <Dot className={r.error ? "bg-s-red" : "bg-s-green"} />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-base">{sourceLabel(r.source)}</div>
                  {r.error && <div className="truncate text-sm text-fg-muted">{de(r.error)}</div>}
                </div>
                <span className="tnum hidden w-[110px] text-right text-sm text-fg-muted sm:block">{nf(r.items_scanned)} geprüft</span>
                <span className="tnum hidden w-[110px] text-right text-sm text-fg-muted md:block">{r.new_accounts} neu</span>
                <span className="w-[100px] text-right text-sm text-fg-faint" title={formatDate(r.started_at)}>{relativeDays(r.started_at)}</span>
              </div>
            ))}
          </Panel>
        )}
      </section>

      <MailboxWizard open={wizard} onClose={() => setWizard(false)} onAdded={(id) => void scan([id])} />
      <Dialog open={panel} onOpenChange={setPanel}>
        <DialogContent className="max-w-[560px]">
          <DialogHeader><DialogTitle>Passwortexport importieren</DialogTitle><DialogDescription>Die Datei wird nur auf diesem Gerät gelesen. Passwörter werden kurz geprüft und nie gespeichert.</DialogDescription></DialogHeader>
          <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-5"><ImportPanel /></div>
        </DialogContent>
      </Dialog>
      <ImportDialog paths={importPaths} onClose={() => setImportPaths(null)} />
      <ShotsDialog paths={shots} onClose={() => setShots(null)} />
    </PageShell>
  );
}

function MailboxRow({ m, busy, onScan }: { m: Mailbox; busy: boolean; onScan(full: boolean): void }) {
  const [testing, setTesting] = useState(false);
  const rule = useStore((s) => s.snap?.automations?.find((r) => r.mailbox_id === m.id));
  const auto = rule?.enabled ? `Automatik ${(EVERY_LABEL[rule.every_days] ?? "").toLowerCase()}` : "";
  return (
    <div className="flex min-h-[52px] items-center gap-3 border-b border-border px-3 py-1.5 last:border-b-0">
      <div className="min-w-0 flex-1">
        <div className="truncate text-base font-medium leading-[18px]">{m.address}</div>
        <div className="truncate text-sm leading-4 text-fg-muted">{m.imap_host}, {m.last_scan ? `zuletzt geprüft ${relativeDays(m.last_scan)}` : "noch nicht geprüft"}{!m.has_password ? ", Passwort fehlt" : ""}{auto ? `, ${auto}` : ""}</div>
      </div>
      <Button variant="secondary" disabled={busy} onClick={() => onScan(false)}>Prüfen</Button>
      <DropdownMenu>
        <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" aria-label="Weitere Aktionen"><MoreHorizontal /></Button></DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem disabled={busy} onSelect={() => onScan(true)}>Komplett neu prüfen</DropdownMenuItem>
          <DropdownMenuItem onSelect={() => navigate({ page: "settings", section: "automation" })}>Automatik einrichten</DropdownMenuItem>
          <DropdownMenuItem disabled={testing} onSelect={async () => {
            setTesting(true);
            try { const f = await api.mail.testSaved(m.id); toast(`Verbindung steht, ${plural(f.length, "Ordner", "Ordner")} gefunden`); } catch (e) { toast.error(errorMessage(e)); } finally { setTesting(false); }
          }}>Verbindung testen</DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem className="text-s-red" onSelect={async () => {
            if (await confirm({ title: `${m.address} entfernen?`, text: "Das Postfach und sein Passwort werden aus ausgeloggt gelöscht. Gefundene Accounts bleiben erhalten.", confirm: "Entfernen", destructive: true })) {
              await api.mail.remove(m.id).catch((e) => toast.error(errorMessage(e)));
            }
          }}>Entfernen</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

function MailboxWizard({ open, onClose, onAdded }: { open: boolean; onClose(): void; onAdded(id: string): void }) {
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-[520px]">
        <DialogHeader><DialogTitle>Postfach hinzufügen</DialogTitle><DialogDescription>Der Zugriff ist nur lesend. Das Passwort wird verschlüsselt im Tresor gespeichert.</DialogDescription></DialogHeader>
        <MailboxForm onCancel={onClose} onSaved={(id) => { onClose(); onAdded(id); }} />
      </DialogContent>
    </Dialog>
  );
}

export function ImportDialog({ paths, onClose }: { paths: string[] | null; onClose(): void }) {
  const settings = useStore((s) => s.snap!.settings);
  const detect = useAsync(async () => {
    if (!paths) return [];
    return Promise.all(paths.map(async (p) => {
      try { return { p, d: await api.imports.detect(p) as ImportDetection | null, err: null as string | null }; } catch (e) { return { p, d: null, err: errorMessage(e) }; }
    }));
  }, [paths?.join("|")]);
  const [pwned, setPwned] = useState(settings.check_pwned_passwords);
  const [busy, setBusy] = useState(false);
  const [prog, setProg] = useState<JobProgress | null>(null);
  const [res, setRes] = useState<ImportDonePayload | null>(null);
  useEffect(() => { if (paths) { setRes(null); setBusy(false); setProg(null); } }, [paths]);
  const items = detect.data ?? [];
  const ok = items.filter((i) => i.d);
  const hasPw = ok.some((i) => i.d!.kind === "passwords");

  async function start() {
    setBusy(true);
    try {
      const d = await runJob<ImportDonePayload>(() => api.imports.start(ok.map((i) => i.p), hasPw && pwned), { onProgress: setProg });
      if (d.payload) setRes(d.payload); else onClose();
      if (!d.ok && d.summary !== "Abgebrochen") toast.error(d.error?.message ?? d.summary);
    } catch (e) { toast.error(errorMessage(e)); } finally { setBusy(false); }
  }

  return (
    <Dialog open={!!paths} onOpenChange={(o) => !o && !busy && onClose()}>
      <DialogContent className="max-w-[520px]">
        <DialogHeader><DialogTitle>{res ? "Import fertig" : "Import"}</DialogTitle></DialogHeader>
        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-5 pb-4">
          {res ? (
            <>
              <p className="text-base">{plural(res.newAccounts, "neuer Account", "neue Accounts")} aus {plural(res.imported, "Datei", "Dateien")}, {plural(res.findings, "Fund", "Funde")}.{res.files > res.imported ? ` ${plural(res.files - res.imported, "Datei", "Dateien")} konnten nicht gelesen werden.` : ""}</p>
              {res.warnings.map((w, i) => <p key={i} className="flex items-start gap-2 text-base text-fg-muted"><Dot className="mt-[7px] bg-s-yellow" />{de(w)}</p>)}
            </>
          ) : (
            <>
              <ul className="flex flex-col">
                {detect.loading && items.length === 0 && <li className="flex h-9 items-center gap-2 text-fg-muted"><Loader2 className="size-3.5 animate-spin" />Dateien werden erkannt</li>}
                {items.map((i) => (
                  <li key={i.p} className="flex min-h-9 items-center gap-3 border-b border-border py-1 last:border-b-0">
                    <span className="min-w-0 flex-1 truncate text-base" title={i.p}>{fileName(i.p)}</span>
                    <span className="flex items-center gap-2 text-sm text-fg-muted"><Dot className={i.d ? "bg-s-green" : "bg-s-red"} />{i.d ? de(i.d.label_de) : i.err ?? "Nicht erkannt"}</span>
                  </li>
                ))}
              </ul>
              {hasPw && (
                <label className="flex items-start gap-2.5 text-base">
                  <Checkbox className="mt-0.5" checked={pwned} onCheckedChange={(v) => setPwned(v === true)} />
                  <span>Passwörter auf Leaks prüfen<span className="block text-sm text-fg-muted">Nur ein kurzer Teil des Hashwerts geht an Have I Been Pwned, nie das Passwort selbst.</span></span>
                </label>
              )}
              {busy && <><Progress value={prog && prog.total ? prog.done / prog.total : undefined} /><p className="truncate text-sm text-fg-muted">{prog?.message}</p></>}
            </>
          )}
        </div>
        <DialogFooter>
          {res ? <Button variant="primary" onClick={onClose}>Fertig</Button> : (
            <>
              <Button variant="ghost" disabled={busy} onClick={onClose}>Abbrechen</Button>
              <Button variant="primary" disabled={busy || ok.length === 0} onClick={() => void start()}>{busy && <Loader2 className="animate-spin" />} {plural(ok.length, "Datei", "Dateien")} importieren</Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function ShotsDialog({ paths, onClose }: { paths: string[] | null; onClose(): void }) {
  const [state, setState] = useState<"idle" | "reading" | "confirm" | "saving">("idle");
  const [data, setData] = useState<ScreenshotReadPayload | null>(null);
  const [sel, setSel] = useState<Set<number>>(new Set());
  const [err, setErr] = useState<string | null>(null);
  const [prog, setProg] = useState<JobProgress | null>(null);

  useEffect(() => {
    if (!paths) return;
    let live = true;
    setState("reading"); setErr(null); setData(null);
    runJob<ScreenshotReadPayload>(() => api.imports.screenshotRead(paths), { onProgress: setProg }).then((d) => {
      if (!live) return;
      if (d.ok && d.payload) { setData(d.payload); setSel(new Set(d.payload.entries.map((_, i) => i))); setState("confirm"); }
      else { setErr(d.summary === "Abgebrochen" ? "Abgebrochen" : d.error?.message ?? d.summary); setState("idle"); }
    }, (e) => { if (live) { setErr(errorMessage(e)); setState("idle"); } });
    return () => { live = false; };
  }, [paths]);

  async function commit() {
    if (!data) return;
    setState("saving");
    try {
      const entries: ScreenshotEntry[] = data.entries.filter((_, i) => sel.has(i));
      const r = await api.imports.screenshotCommit(data.provider, entries);
      toast(`${plural(r.new_accounts, "neuer Account", "neue Accounts")}${r.updated ? `, ${r.updated} aktualisiert` : ""}`);
      onClose();
    } catch (e) { setErr(errorMessage(e)); setState("confirm"); }
  }

  return (
    <Dialog open={!!paths} onOpenChange={(o) => !o && state !== "saving" && onClose()}>
      <DialogContent className="max-w-[520px]">
        <DialogHeader>
          <DialogTitle>Screenshots lesen</DialogTitle>
          {state === "confirm" && data && <DialogDescription>{plural(data.entries.length, "Eintrag", "Einträge")} erkannt{data.provider ? `, Anmeldung über ${ssoLabel(data.provider)}` : ""}. Wähle aus, was übernommen wird.</DialogDescription>}
        </DialogHeader>
        <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto px-5 pb-4">
          {state === "reading" && (
            <div className="flex flex-col gap-2 py-4"><Progress value={prog && prog.total ? prog.done / prog.total : undefined} /><p className="text-base text-fg-muted">{prog?.message ?? "Bilder werden gelesen"}</p></div>
          )}
          {err && <p className="flex items-start gap-2 text-base"><Dot className="mt-[7px] bg-s-red" />{err}</p>}
          {state === "confirm" && data && data.entries.length === 0 && <p className="text-base text-fg-muted">Auf den Bildern wurde nichts erkannt.</p>}
          {(state === "confirm" || state === "saving") && data?.entries.map((e, i) => (
            <label key={i} className="flex h-10 items-center gap-3 rounded-md px-1 hover:bg-muted">
              <Checkbox checked={sel.has(i)} onCheckedChange={() => setSel((s) => { const n = new Set(s); if (n.has(i)) n.delete(i); else n.add(i); return n; })} />
              <ServiceLogo name={e.name} domain={e.domain ?? `unresolved:${e.name}`} size={24} />
              <span className="min-w-0 flex-1 truncate text-base">{e.name}</span>
              <span className="max-w-[45%] truncate text-sm text-fg-faint">{e.domain ?? e.relay_address ?? "Domain unbekannt"}</span>
            </label>
          ))}
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={onClose} disabled={state === "saving"}>{state === "confirm" ? "Verwerfen" : "Schließen"}</Button>
          {state === "confirm" && <Button variant="primary" disabled={sel.size === 0} onClick={() => void commit()}>{plural(sel.size, "Account", "Accounts")} übernehmen</Button>}
          {state === "saving" && <Button variant="primary" disabled><Loader2 className="animate-spin" /> Wird gespeichert</Button>}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
