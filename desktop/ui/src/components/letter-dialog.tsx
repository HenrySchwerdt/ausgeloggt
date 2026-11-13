import { useEffect, useState } from "react";
import { create } from "zustand";
import { toast } from "sonner";
import { AlertTriangle, Copy, FileDown, Loader2, Mail, MoreHorizontal, Send } from "lucide-react";
import { api, runJob } from "@/lib/api";
import { mailsLeft } from "@/lib/automation";
import { markErasureSent, useQuota, useSendMailbox } from "@/lib/send";
import type { LetterSendResult } from "@/lib/ipc";
import { navigate } from "@/lib/router";
import { openUrl, pickSavePath } from "@/lib/platform";
import { useAsync } from "@/lib/hooks";
import { de, errorMessage, formatDate } from "@/lib/utils";
import { LETTER_LABEL, type LetterArgs, type LetterKind, type Party, type RequestKind } from "@/lib/types";
import { ErrorState } from "./common";
import { Button } from "./ui/button";
import { Checkbox } from "./ui/checkbox";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "./ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "./ui/dropdown-menu";
import { Input, Textarea } from "./ui/input";
import { Select } from "./ui/select";

export interface LetterRequest {
  args: LetterArgs;
  /** Display name of the other side, used for the request record and the file name. */
  target: string;
  domain?: string;
  parentId?: string;
}

const useLetter = create<{ req: LetterRequest | null; open(r: LetterRequest): void; close(): void }>((set) => ({
  req: null,
  open: (req) => set({ req }),
  close: () => set({ req: null }),
}));

/** Open the letter preview from anywhere. */
export const openLetter = (r: LetterRequest) => useLetter.getState().open(r);
// Navigating (back button, links) closes the dialog instead of leaving it over another page.
if (typeof window !== "undefined") window.addEventListener("hashchange", () => useLetter.getState().close());

const REQUEST_KIND: Record<LetterKind, RequestKind> = {
  Erasure: "Erasure", Access: "Access", ObjectionAndAccess: "Objection", Cancellation: "Cancellation",
  CeaseAndDesist: "CeaseAndDesist", CreditBureauAccess: "CreditBureauAccess", AddressBrokerObjection: "AdvertisingObjection",
  Complaint: "Complaint", Reminder: "Reminder",
};

const INTRO: Partial<Record<LetterKind, string>> = {
  Erasure: "Der Anbieter muss binnen eines Monats antworten.",
  Access: "Der Anbieter muss binnen eines Monats antworten.",
  ObjectionAndAccess: "Der Anbieter muss binnen eines Monats antworten.",
  Cancellation: "Das Abo endet zum nächstmöglichen Termin. Die Bestätigung des Anbieters ist dein Nachweis.",
  CeaseAndDesist: "Frist für den Absender: 14 Tage.",
  CreditBureauAccess: "Die erste Datenkopie ist kostenlos. Antwort binnen eines Monats.",
  AddressBrokerObjection: "Antwort binnen eines Monats.",
  Complaint: "Die Aufsichtsbehörde prüft deinen Fall und meldet sich bei dir.",
  Reminder: "Frist für den Anbieter: 14 Tage.",
};

export function LetterHost() {
  const req = useLetter((s) => s.req);
  const close = useLetter((s) => s.close);
  return <Dialog open={!!req} onOpenChange={(o) => !o && close()}>{req && <LetterContent req={req} close={close} />}</Dialog>;
}

function LetterContent({ req, close }: { req: LetterRequest; close(): void }) {
  const [recipient, setRecipient] = useState<Party | undefined>(req.args.recipient);
  const args: LetterArgs = { ...req.args, recipient };
  const pv = useAsync(() => api.letters.preview(args), [JSON.stringify(args)]);
  const [busy, setBusy] = useState<null | "pdf" | "mail" | "sent" | "send">(null);
  const [rcName, setRcName] = useState("");
  const [rcAddr, setRcAddr] = useState("");
  const [rcMail, setRcMail] = useState("");
  const [editRc, setEditRc] = useState(false);
  const [attachPdf, setAttachPdf] = useState(true);
  const [progress, setProgress] = useState("");
  const [jobId, setJobId] = useState<string | null>(null);
  const [sendError, setSendError] = useState<string | null>(null);
  const { mailboxes, mailbox, setMailboxId } = useSendMailbox();
  const quota = useQuota(mailbox?.id ?? null);
  const p = pv.data;
  const label = LETTER_LABEL[req.args.kind];
  useEffect(() => { setRecipient(req.args.recipient); }, [req]);

  const complete = !!p && p.missing_sender_fields.length === 0 && !p.missing_recipient;
  const hasMail = !!p?.recipient_email;
  const left = quota.data?.remaining ?? null;
  const canSend = complete && hasMail && !!mailbox && (left === null || left > 0);
  const showForm = !!p && (p.missing_recipient || editRc);

  async function savePdf() {
    if (!p) return;
    setBusy("pdf");
    try {
      const path = await pickSavePath(`${label} ${req.target}.pdf`.replace(/[\\/:*?"<>|]/g, " "), "pdf", "PDF");
      if (!path) return;
      await api.letters.savePdf(args, path);
      toast(`PDF gespeichert`);
    } catch (e) { toast.error(errorMessage(e)); } finally { setBusy(null); }
  }

  async function mailto() {
    if (!p) return;
    setBusy("mail");
    try {
      const url = await api.letters.mailtoUrl(p.subject, p.text, p.recipient_email ?? undefined);
      await openUrl(url);
    } catch (e) { toast.error(errorMessage(e)); } finally { setBusy(null); }
  }

  async function sendNow() {
    if (!p || !mailbox) return;
    setBusy("send"); setSendError(null); setProgress("Brief wird vorbereitet");
    try {
      const d = await runJob<LetterSendResult>(
        () => api.sending.letterSend({ ...args, mailboxId: mailbox.id, attachPdf }),
        { onStarted: setJobId, onProgress: (x) => setProgress(x.message) },
      );
      if (!d.ok) {
        if (d.summary !== "Abgebrochen") setSendError(d.error?.message ? errorMessage(d.error) : d.summary);
        setBusy(null); setJobId(null);
        return;
      }
      if (req.args.kind === "Erasure" && req.args.accountId && p.recipient_email) await markErasureSent(req.args.accountId, p.recipient_email);
      toast(p.deadline ? `Gesendet. Antwort bis ${formatDate(p.deadline)}.` : "Gesendet", {
        description: d.payload?.appendError ? "Die Kopie in Gesendet konnte nicht abgelegt werden." : undefined,
        action: { label: "Zu den Anträgen", onClick: () => navigate({ page: "requests" }) },
        duration: 8000,
      });
      close();
    } catch (e) { setSendError(errorMessage(e)); setBusy(null); setJobId(null); }
  }

  async function markSent() {
    if (!p) return;
    setBusy("sent");
    try {
      const email = p.recipient_email;
      await api.requests.create({
        kind: REQUEST_KIND[req.args.kind], target: req.target, domain: req.domain, accountId: req.args.accountId,
        subscriptionId: req.args.subscriptionId, parentId: req.parentId ?? req.args.requestId, channel: email ? "Email" : "Post",
        recipient: email ?? [p.recipient.name, ...p.recipient.address_lines].join("\n"), subject: p.subject, body: p.text, sent: true,
      });
      if (req.args.kind === "Erasure" && req.args.accountId && email) {
        await api.accounts.addDeletionStep(req.args.accountId, { GdprEmail: { to: email } }, `${label} versendet.`).catch(() => {});
      }
      toast(p.deadline ? `Als gesendet markiert. Antwort bis ${formatDate(p.deadline)}.` : "Als gesendet markiert");
      close();
    } catch (e) { toast.error(errorMessage(e)); setBusy(null); }
  }

  function applyRecipient() {
    const lines = rcAddr.split("\n").map((l) => l.trim()).filter(Boolean);
    setRecipient({ name: rcName.trim() || "Empfänger", address_lines: lines, email: rcMail.trim() || null });
    setEditRc(false);
  }
  function editRecipient() {
    if (p && !p.missing_recipient) { setRcName(p.recipient.name); setRcAddr(p.recipient.address_lines.join("\n")); setRcMail(p.recipient_email ?? ""); }
    setEditRc(true);
  }

  const sending = busy === "send";
  const showSend = mailboxes.length > 0 && (hasMail || !complete);
  const mailMissing = complete && !hasMail;
  const fixSettings = () => { close(); navigate({ page: "settings", section: "identity" }); };

  return (
    <DialogContent className="max-w-[720px]">
      <DialogHeader>
        <DialogTitle>{label}{req.target ? `: ${req.target}` : ""}</DialogTitle>
        {INTRO[req.args.kind] && <DialogDescription>{INTRO[req.args.kind]}</DialogDescription>}
      </DialogHeader>

      {pv.error && !p ? <ErrorState message={pv.error} onRetry={pv.reload} className="py-10" /> : !p ? (
        <div className="flex h-48 items-center justify-center text-fg-faint"><Loader2 className="size-4 animate-spin" /></div>
      ) : (
        <div className="selectable flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-5 pb-4">
          {p.missing_sender_fields.length > 0 && (
            <div className="flex items-center gap-3 rounded-md bg-muted px-3 py-2.5 text-base">
              <AlertTriangle className="size-3.5 shrink-0 text-s-yellow" />
              <span className="flex-1 text-fg-muted">Zum Senden fehlen deine Angaben: {p.missing_sender_fields.join(", ")}.</span>
              <Button onClick={fixSettings}>Angaben ergänzen</Button>
            </div>
          )}
          {showForm && (
            <div className="flex flex-col gap-2 rounded-md border border-border p-3">
              <p className="text-base text-fg-muted">{p.missing_recipient ? "Für diesen Empfänger ist keine Adresse bekannt. Trage sie ein." : "Empfänger ändern."}</p>
              <Input placeholder="Name der Firma" value={rcName} onChange={(e) => setRcName(e.target.value)} />
              <Textarea placeholder={"Straße\nPLZ Ort"} rows={2} value={rcAddr} onChange={(e) => setRcAddr(e.target.value)} />
              <Input placeholder="Mailadresse (zum Senden nötig)" value={rcMail} onChange={(e) => setRcMail(e.target.value)} />
              <div className="flex gap-2">
                <Button onClick={applyRecipient} disabled={!rcName.trim() || (!rcAddr.trim() && !rcMail.trim())}>Übernehmen</Button>
                {editRc && !p.missing_recipient && <Button variant="ghost" onClick={() => setEditRc(false)}>Abbrechen</Button>}
              </div>
            </div>
          )}
          <dl className="grid grid-cols-[56px_1fr] gap-x-3 gap-y-1 text-base">
            <dt className="text-fg-faint">An</dt>
            <dd className="flex min-w-0 items-center gap-2">
              <span className="truncate">{de(p.recipient.name)}{p.recipient_email ? <span className="text-fg-muted">, {p.recipient_email}</span> : <span className="text-fg-muted">, per Post</span>}</span>
              {!showForm && !p.missing_recipient && <button className="shrink-0 text-sm text-fg-faint underline underline-offset-2 hover:text-fg" onClick={editRecipient}>Ändern</button>}
            </dd>
            <dt className="text-fg-faint">Betreff</dt><dd className="truncate">{p.subject}</dd>
          </dl>
          <pre className="whitespace-pre-wrap rounded-md border border-border bg-muted/40 p-4 font-sans text-base leading-6 text-fg">{p.text}</pre>
        </div>
      )}

      {p && (
        <div className="border-t border-border px-5 py-3">
          {mailboxes.length === 0 ? (
            <p className="text-base text-fg-muted">
              Verbinde ein Postfach, dann senden wir den Brief direkt von dir.{" "}
              <button className="text-fg underline underline-offset-2" onClick={() => { close(); navigate({ page: "sources" }); }}>Postfach verbinden</button>
            </p>
          ) : sending ? (
            <div className="flex items-center gap-3 text-base">
              <Loader2 className="size-3.5 animate-spin text-fg-muted" />
              <span className="flex-1 text-fg-muted">{progress}</span>
              {jobId && <Button variant="ghost" onClick={() => void api.jobs.cancel(jobId)}>Abbrechen</Button>}
            </div>
          ) : mailMissing ? (
            <div className="flex items-center gap-3 text-base">
              <span className="flex-1 text-fg-muted">Dieser Empfänger hat keine Mailadresse. Speichere den Brief als PDF für die Post oder trage eine Mailadresse ein.</span>
              <Button onClick={editRecipient}>Mailadresse eintragen</Button>
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
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
              <p className="text-sm text-fg-muted">
                {left !== null && left <= 0
                  ? `Das Tageslimit von ${quota.data?.cap} Mails ist erreicht. Morgen geht es weiter.`
                  : <>{left !== null && <>{mailsLeft(left)}. </>}Wir senden von deinem Postfach. Eine Kopie liegt danach in Gesendet.</>}
              </p>
              {sendError && <p className="flex items-start gap-2 text-sm"><span className="mt-1.5 size-1.5 shrink-0 rounded-full bg-s-red" />{sendError}</p>}
            </div>
          )}
        </div>
      )}

      <DialogFooter className="flex-wrap">
        <DropdownMenu>
          <DropdownMenuTrigger asChild><Button variant="ghost" disabled={!p} aria-label="Weitere Möglichkeiten"><MoreHorizontal /> Mehr</Button></DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            <DropdownMenuItem onSelect={() => p && navigator.clipboard?.writeText(`${p.subject}\n\n${p.text}`).then(() => toast("Brief kopiert"), () => toast.error("Kopieren nicht möglich"))}><Copy /> Brief kopieren</DropdownMenuItem>
            {canSend && <DropdownMenuItem disabled={!complete || busy !== null} onSelect={() => void markSent()}>Als gesendet markieren</DropdownMenuItem>}
          </DropdownMenuContent>
        </DropdownMenu>
        <span className="flex-1" />
        <Button variant="secondary" disabled={!complete || busy !== null} onClick={() => void savePdf()}>
          {busy === "pdf" ? <Loader2 className="animate-spin" /> : <FileDown />} Als PDF speichern
        </Button>
        <Button variant="secondary" disabled={!p?.recipient_email || busy !== null} onClick={() => void mailto()}>
          <Mail /> In Mailprogramm öffnen
        </Button>
        {showSend ? (
          <Button variant="primary" disabled={!canSend || busy !== null} onClick={() => void sendNow()}>
            {sending ? <Loader2 className="animate-spin" /> : <Send />} Jetzt senden
          </Button>
        ) : (
          <Button variant="primary" disabled={!complete || busy !== null} onClick={() => void markSent()}>
            {busy === "sent" && <Loader2 className="animate-spin" />} Als gesendet markieren
          </Button>
        )}
      </DialogFooter>
    </DialogContent>
  );
}
