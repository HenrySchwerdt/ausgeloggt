// Bulk send of erasure requests: a review sheet (what goes where, what has problems), then sending with progress.
import { useEffect, useMemo, useRef, useState } from "react";
import { create } from "zustand";
import { toast } from "sonner";
import { Check, Loader2 } from "lucide-react";
import { api, runJob } from "@/lib/api";
import { mailsLeft, sendDuration } from "@/lib/automation";
import type { BulkSendItem, BulkSendResult, SendCheck } from "@/lib/ipc";
import { markErasureSent, missingSenderFields, useQuota, useSendMailbox } from "@/lib/send";
import { navigate } from "@/lib/router";
import { useStore } from "@/lib/store";
import { errorMessage, plural } from "@/lib/utils";
import type { Party } from "@/lib/types";
import { Dot } from "./common";
import { Button } from "./ui/button";
import { Checkbox } from "./ui/checkbox";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "./ui/dialog";
import { Input } from "./ui/input";
import { Progress } from "./ui/progress";
import { Select } from "./ui/select";

const useBulk = create<{ ids: string[] | null; open(ids: string[]): void; close(): void }>((set) => ({
  ids: null,
  open: (ids) => set({ ids }),
  close: () => set({ ids: null }),
}));

/** Open the review sheet for erasure requests to these accounts. */
export const openBulkSend = (ids: string[]) => useBulk.getState().open(ids);

export function BulkSendHost() {
  const ids = useBulk((s) => s.ids);
  const close = useBulk((s) => s.close);
  const lock = useRef(false);
  return (
    <Dialog open={!!ids} onOpenChange={(o) => { if (!o && !lock.current) close(); }}>
      {ids && <BulkContent ids={ids} close={close} lock={lock} />}
    </Dialog>
  );
}

type Phase = "check" | "running" | "done";
type Problem = "address" | "sender" | "other";

const classify = (reason: string): Problem =>
  /ungültig/i.test(reason) ? "other"
    : /Mailadresse/i.test(reason) ? "address"
      : /Angaben|Absender|Name|Straße/i.test(reason) ? "sender" : "other";

function BulkContent({ ids, close, lock }: { ids: string[]; close(): void; lock: React.MutableRefObject<boolean> }) {
  const snap = useStore((s) => s.snap)!;
  const { mailboxes, mailbox, setMailboxId } = useSendMailbox();
  const quota = useQuota(mailbox?.id ?? null);
  const [attachPdf, setAttachPdf] = useState(true);
  const [skip, setSkip] = useState<Set<string>>(new Set());
  const [recipients, setRecipients] = useState<Record<string, Party>>({});
  const [typed, setTyped] = useState<Record<string, string>>({});
  const [check, setCheck] = useState<SendCheck | null>(null);
  const [checkError, setCheckError] = useState<string | null>(null);
  const [phase, setPhase] = useState<Phase>("check");
  const [progress, setProgress] = useState({ done: 0, total: 0, message: "" });
  const [log, setLog] = useState<{ id: string; ok: boolean; message: string }[]>([]);
  const [result, setResult] = useState<BulkSendResult | null>(null);
  const [jobId, setJobId] = useState<string | null>(null);
  const emails = useRef<Map<string, string>>(new Map());

  const name = (id: string) => snap.accounts.find((a) => a.id === id)?.name ?? id;
  const items: BulkSendItem[] = useMemo(
    () => ids.filter((id) => !skip.has(id)).map((id) => ({ kind: "Erasure" as const, accountId: id, ...(recipients[id] ? { recipient: recipients[id] } : {}) })),
    [ids, skip, recipients],
  );

  useEffect(() => {
    if (phase !== "check" || !mailbox) return;
    let live = true;
    setCheck(null); setCheckError(null);
    api.sending.lettersSendCheck(items, mailbox.id, attachPdf).then(
      (c) => { if (!live) return; setCheck(c); for (const o of c.ok) emails.current.set(o.accountId, o.recipientEmail); },
      (e) => live && setCheckError(errorMessage(e)),
    );
    return () => { live = false; };
  }, [items, mailbox?.id, attachPdf, phase]);

  const missing = missingSenderFields(snap);
  const ready = check?.ok.length ?? 0;
  const problems = check?.problems ?? [];
  const left = check?.remainingToday ?? quota.data?.remaining ?? null;
  const gap = check?.gapSeconds ?? quota.data?.gapSeconds ?? 20;
  const sendable = ready > 0 && missing.length === 0 && !!mailbox;
  const willSend = left === null ? ready : Math.min(ready, left);

  function applyAddress(id: string) {
    const mail = (typed[id] ?? "").trim();
    if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(mail)) { toast.error("Das ist keine gültige Mailadresse."); return; }
    setRecipients((r) => ({ ...r, [id]: { name: name(id), address_lines: [], email: mail } }));
  }
  const skipOne = (id: string) => setSkip((s) => new Set(s).add(id));

  async function start() {
    if (!mailbox) return;
    const ok = new Set(check?.ok.map((o) => o.accountId));
    setPhase("running"); lock.current = true;
    setProgress({ done: 0, total: ready, message: "Wird vorbereitet" });
    try {
      const d = await runJob<BulkSendResult>(() => api.sending.lettersSendBulk(items.filter((it) => ok.has(it.accountId)), mailbox.id, attachPdf), {
        onStarted: setJobId,
        onProgress: (p) => {
          setProgress({ done: p.done, total: p.total, message: p.message });
          if (p.item) setLog((l) => [...l, { id: p.item!.id, ok: p.item!.ok, message: p.item!.message }]);
        },
      });
      const payload = d.payload ?? { sent: 0, failed: 0, skipped: 0, capReached: false, results: [] };
      for (const r of payload.results) {
        const to = emails.current.get(r.id);
        if (r.ok && to) await markErasureSent(r.id, to);
      }
      setResult(d.ok || d.payload ? payload : null);
      if (!d.payload) { toast.error(d.summary === "Abgebrochen" ? "Abgebrochen" : errorMessage(d.error ?? d.summary)); }
      setPhase("done");
    } catch (e) {
      toast.error(errorMessage(e));
      setPhase("check");
    } finally { lock.current = false; setJobId(null); }
  }

  /* ---------- sending ---------- */
  if (phase === "running") {
    return (
      <DialogContent className="max-w-[560px]" onEscapeKeyDown={(e) => e.preventDefault()} onInteractOutside={(e) => e.preventDefault()}>
        <DialogHeader>
          <DialogTitle>Löschanträge werden gesendet</DialogTitle>
          <DialogDescription>Zwischen zwei Mails warten wir {gap} Sekunden. Du kannst das Fenster geöffnet lassen oder abbrechen.</DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-3 px-5 pb-4">
          <Progress value={progress.total ? progress.done / progress.total : undefined} />
          <div className="flex items-center justify-between text-base">
            <span className="truncate text-fg-muted">{progress.message}</span>
            <span className="tnum shrink-0 text-fg-muted">{progress.done} von {progress.total}</span>
          </div>
          <ul className="flex max-h-[220px] flex-col overflow-y-auto">
            {log.map((l, i) => (
              <li key={i} className="flex h-8 items-center gap-2.5 border-b border-border text-base last:border-b-0">
                <Dot className={l.ok ? "bg-s-green" : "bg-s-red"} />
                <span className="flex-1 truncate">{name(l.id)}</span>
                <span className="max-w-[55%] truncate text-sm text-fg-muted" title={l.message}>{l.ok ? "Gesendet" : l.message}</span>
              </li>
            ))}
          </ul>
        </div>
        <DialogFooter>
          <Button variant="ghost" disabled={!jobId} onClick={() => jobId && void api.jobs.cancel(jobId)}>Abbrechen</Button>
        </DialogFooter>
      </DialogContent>
    );
  }

  /* ---------- summary ---------- */
  if (phase === "done") {
    const r = result;
    const bad = r?.results.filter((x) => !x.ok && !x.skipped) ?? [];
    const skipped = r?.results.filter((x) => x.skipped) ?? [];
    return (
      <DialogContent className="max-w-[560px]">
        <DialogHeader>
          <DialogTitle>{r && r.sent > 0 ? `${plural(r.sent, "Löschantrag", "Löschanträge")} gesendet` : "Nichts gesendet"}</DialogTitle>
          <DialogDescription>
            {r ? [r.failed ? `${r.failed} fehlgeschlagen` : "", r.skipped ? `${r.skipped} übersprungen` : ""].filter(Boolean).join(", ") || "Alle Anträge sind raus. Die Fristen stehen unter Anträge." : "Der Versand wurde abgebrochen."}
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-3 px-5 pb-4">
          {r?.capReached && <p className="flex items-start gap-2 text-base"><Dot className="mt-[7px] bg-s-yellow" />Das Tageslimit ist erreicht. Die übrigen Anträge sendest du morgen.</p>}
          {(bad.length > 0 || skipped.length > 0) && (
            <ul className="flex max-h-[220px] flex-col overflow-y-auto">
              {[...bad, ...skipped].map((x) => (
                <li key={x.id} className="flex min-h-8 items-center gap-2.5 border-b border-border py-1 text-base last:border-b-0">
                  <Dot className={x.skipped ? "bg-s-gray" : "bg-s-red"} />
                  <span className="flex-1 truncate">{name(x.id)}</span>
                  <span className="max-w-[60%] truncate text-sm text-fg-muted" title={x.message}>{x.message}</span>
                </li>
              ))}
            </ul>
          )}
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={close}>Schließen</Button>
          <Button variant="primary" onClick={() => { close(); navigate({ page: "requests" }); }}>Zu den Anträgen</Button>
        </DialogFooter>
      </DialogContent>
    );
  }

  /* ---------- review ---------- */
  const total = ids.length - skip.size;
  return (
    <DialogContent className="max-w-[680px]">
      <DialogHeader>
        <DialogTitle>{plural(ids.length, "Löschantrag", "Löschanträge")} senden</DialogTitle>
        <DialogDescription>Prüfe, an wen die Mails gehen. Nichts wird gesendet, bevor du auf Senden klickst.</DialogDescription>
      </DialogHeader>

      {mailboxes.length === 0 ? (
        <div className="flex flex-col items-start gap-3 px-5 pb-5">
          <p className="text-base text-fg-muted">Zum Senden brauchst du ein verbundenes Postfach.</p>
          <Button variant="primary" onClick={() => { close(); navigate({ page: "sources" }); }}>Postfach verbinden</Button>
        </div>
      ) : (
        <>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2 px-5 pb-3">
            <span className="text-base text-fg-faint">Von</span>
            {mailboxes.length > 1 ? (
              <Select className="min-w-[220px]" value={mailbox?.id ?? ""} onChange={(e) => setMailboxId(e.target.value)} aria-label="Postfach zum Senden">
                {mailboxes.map((m) => <option key={m.id} value={m.id}>{m.address}</option>)}
              </Select>
            ) : <span className="text-base">{mailbox?.address}</span>}
            <label className="ml-auto flex cursor-default items-center gap-2 text-base text-fg-muted">
              <Checkbox checked={attachPdf} onCheckedChange={(v) => setAttachPdf(v === true)} /> PDF anhängen
            </label>
          </div>

          {missing.length > 0 && (
            <div className="mx-5 mb-3 flex items-center gap-3 rounded-md bg-muted px-3 py-2.5 text-base">
              <Dot className="bg-s-yellow" />
              <span className="flex-1 text-fg-muted">Deine Angaben fehlen: {missing.join(", ")}.</span>
              <Button onClick={() => { close(); navigate({ page: "settings", section: "identity" }); }}>Angaben ergänzen</Button>
            </div>
          )}

          <div className="selectable min-h-0 flex-1 overflow-y-auto border-t border-border">
            {checkError ? <p className="px-5 py-6 text-base text-fg-muted">{checkError}</p> : !check ? (
              <div className="flex h-32 items-center justify-center text-fg-faint"><Loader2 className="size-4 animate-spin" /></div>
            ) : (
              <ul>
                {problems.map((p) => {
                  const kind = classify(p.reason);
                  return (
                    <li key={p.accountId} className="flex items-center gap-3 border-b border-border px-5 py-2 text-base">
                      <Dot className="bg-s-red" />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate font-medium">{name(p.accountId)}</span>
                        <span className="block truncate text-sm text-fg-muted" title={p.reason}>
                          {kind === "address" ? "Keine Mailadresse bekannt" : kind === "sender" ? "Deine Angaben fehlen" : p.reason}
                        </span>
                      </span>
                      {kind === "address" ? (
                        <>
                          <Input
                            className="h-7 w-[200px] shrink-0" placeholder="Mailadresse eintragen" value={typed[p.accountId] ?? ""}
                            onChange={(e) => setTyped((t) => ({ ...t, [p.accountId]: e.target.value }))}
                            onKeyDown={(e) => e.key === "Enter" && applyAddress(p.accountId)}
                          />
                          <Button disabled={!(typed[p.accountId] ?? "").trim()} onClick={() => applyAddress(p.accountId)}>Übernehmen</Button>
                          <Button variant="ghost" onClick={() => skipOne(p.accountId)}>Überspringen</Button>
                        </>
                      ) : kind === "other" ? (
                        <span className="text-sm text-fg-faint">Wird übersprungen</span>
                      ) : null}
                    </li>
                  );
                })}
                {check.ok.map((o) => (
                  <li key={o.accountId} className="flex h-10 items-center gap-3 border-b border-border px-5 text-base last:border-b-0">
                    <Check className="size-3.5 shrink-0 text-fg-faint" />
                    <span className="min-w-0 flex-1 truncate font-medium">{o.name}</span>
                    <span className="max-w-[55%] truncate text-sm text-fg-muted">{o.recipientEmail}</span>
                  </li>
                ))}
                {check.ok.length === 0 && problems.length === 0 && <li className="px-5 py-6 text-base text-fg-muted">Alle Accounts wurden übersprungen.</li>}
              </ul>
            )}
          </div>

          <div className="flex flex-col gap-0.5 border-t border-border px-5 py-3 text-sm text-fg-muted">
            {check && (
              <>
                <span>
                  {plural(ready, "Mail bereit", "Mails bereit")}{problems.length ? `, ${plural(problems.length, "Problem", "Probleme")}` : ""}
                  {total !== ids.length ? `, ${ids.length - total} übersprungen` : ""}.
                  {left !== null && <> {mailsLeft(left)}.</>}
                </span>
                {ready > 0 && (
                  <span>
                    {left !== null && left < ready
                      ? `Heute gehen nur ${left} raus, den Rest sendest du morgen. `
                      : ""}
                    Das Senden dauert {sendDuration(willSend, gap)}, mit {gap} Sekunden Abstand zwischen den Mails.
                  </span>
                )}
              </>
            )}
          </div>
        </>
      )}

      <DialogFooter>
        <Button variant="ghost" onClick={close}>Abbrechen</Button>
        <Button variant="primary" disabled={!sendable || willSend === 0} onClick={() => void start()}>
          Senden{ready > 0 ? ` (${willSend})` : ""}
        </Button>
      </DialogFooter>
    </DialogContent>
  );
}
