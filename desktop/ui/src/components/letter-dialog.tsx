import { useEffect, useState } from "react";
import { create } from "zustand";
import { toast } from "sonner";
import { AlertTriangle, Copy, FileDown, Loader2, Mail } from "lucide-react";
import { api } from "@/lib/api";
import { navigate } from "@/lib/router";
import { openUrl, pickSavePath } from "@/lib/platform";
import { useAsync } from "@/lib/hooks";
import { de, errorMessage, formatDate } from "@/lib/utils";
import { LETTER_LABEL, type LetterArgs, type LetterKind, type Party, type RequestKind } from "@/lib/types";
import { ErrorState } from "./common";
import { Button } from "./ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "./ui/dialog";
import { Input, Textarea } from "./ui/input";

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

const REQUEST_KIND: Record<LetterKind, RequestKind> = {
  Erasure: "Erasure", Access: "Access", ObjectionAndAccess: "Objection", Cancellation: "Cancellation",
  CeaseAndDesist: "CeaseAndDesist", CreditBureauAccess: "CreditBureauAccess", AddressBrokerObjection: "AdvertisingObjection",
  Complaint: "Complaint", Reminder: "Reminder",
};

const INTRO: Partial<Record<LetterKind, string>> = {
  Erasure: "Der Anbieter muss binnen eines Monats antworten.",
  Access: "Der Anbieter muss binnen eines Monats antworten.",
  ObjectionAndAccess: "Der Anbieter muss binnen eines Monats antworten.",
  Cancellation: "Bitte Bestätigung der Kündigung abwarten.",
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
  const [busy, setBusy] = useState<null | "pdf" | "mail" | "sent">(null);
  const [rcName, setRcName] = useState("");
  const [rcAddr, setRcAddr] = useState("");
  const [rcMail, setRcMail] = useState("");
  const p = pv.data;
  const label = LETTER_LABEL[req.args.kind];
  useEffect(() => { setRecipient(req.args.recipient); }, [req]);

  const complete = !!p && p.missing_sender_fields.length === 0 && !p.missing_recipient;

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
  }

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
            <div className="flex items-start gap-2 rounded-md bg-muted px-3 py-2 text-base text-fg-muted">
              <AlertTriangle className="mt-0.5 size-3.5 shrink-0 text-s-yellow" />
              <span>
                Deine Angaben fehlen: {p.missing_sender_fields.join(", ")}.{" "}
                <button className="text-fg underline underline-offset-2" onClick={() => { close(); navigate({ page: "settings", section: "identity" }); }}>
                  In den Einstellungen ergänzen
                </button>
              </span>
            </div>
          )}
          {p.missing_recipient && (
            <div className="flex flex-col gap-2 rounded-md border border-border p-3">
              <p className="text-base text-fg-muted">Für diesen Empfänger ist keine Adresse bekannt. Trage sie ein.</p>
              <Input placeholder="Name der Firma" value={rcName} onChange={(e) => setRcName(e.target.value)} />
              <Textarea placeholder={"Straße\nPLZ Ort"} rows={2} value={rcAddr} onChange={(e) => setRcAddr(e.target.value)} />
              <Input placeholder="Mailadresse (optional)" value={rcMail} onChange={(e) => setRcMail(e.target.value)} />
              <div><Button onClick={applyRecipient} disabled={!rcName.trim() || (!rcAddr.trim() && !rcMail.trim())}>Übernehmen</Button></div>
            </div>
          )}
          <dl className="grid grid-cols-[56px_1fr] gap-x-3 gap-y-1 text-base">
            <dt className="text-fg-faint">An</dt>
            <dd className="truncate">{de(p.recipient.name)}{p.recipient_email ? <span className="text-fg-muted">, {p.recipient_email}</span> : <span className="text-fg-muted">, per Post</span>}</dd>
            <dt className="text-fg-faint">Betreff</dt><dd className="truncate">{p.subject}</dd>
          </dl>
          <pre className="whitespace-pre-wrap rounded-md border border-border bg-muted/40 p-4 font-sans text-base leading-6 text-fg">{p.text}</pre>
        </div>
      )}

      <DialogFooter className="flex-wrap">
        <Button variant="ghost" disabled={!p} onClick={() => p && navigator.clipboard?.writeText(`${p.subject}\n\n${p.text}`).then(() => toast("Brief kopiert"), () => toast.error("Kopieren nicht möglich"))}>
          <Copy /> Kopieren
        </Button>
        <span className="flex-1" />
        <Button variant="secondary" disabled={!complete || busy !== null} onClick={() => void savePdf()}>
          {busy === "pdf" ? <Loader2 className="animate-spin" /> : <FileDown />} Als PDF speichern
        </Button>
        <Button variant="secondary" disabled={!p?.recipient_email || busy !== null} onClick={() => void mailto()}>
          <Mail /> In Mailprogramm öffnen
        </Button>
        <Button variant="primary" disabled={!complete || busy !== null} onClick={() => void markSent()}>
          {busy === "sent" && <Loader2 className="animate-spin" />} Als gesendet markieren
        </Button>
      </DialogFooter>
    </DialogContent>
  );
}
