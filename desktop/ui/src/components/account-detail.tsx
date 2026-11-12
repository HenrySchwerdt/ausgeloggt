import { useEffect, useState } from "react";
import { toast } from "sonner";
import {
  AlertTriangle, Check, ChevronDown, Copy, ExternalLink, FileText, Mail, X,
} from "lucide-react";
import { api, type CatalogInfo, type GdprLetter } from "@/lib/api";
import { useStore } from "@/lib/store";
import { navigate } from "@/lib/router";
import {
  ACCOUNT_STATUS_LABEL, connectionsUrl, EVIDENCE_LABEL, methodTag, sourceGroup, ssoLabel, ssoKey,
  type Account, type AccountStatus, type DeletionStep,
} from "@/lib/types";
import { cn, deadlineLabel, daysFromNow, formatDate, isApplePrivateRelay, relativeDays } from "@/lib/utils";
import { Dot, ServiceLogo, STATUS_COLOR } from "./common";
import { Button } from "./ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "./ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuRadioGroup, DropdownMenuRadioItem, DropdownMenuTrigger } from "./ui/dropdown-menu";
import { Textarea } from "./ui/input";
import { Tooltip } from "./ui/tooltip";

const STATUS_ORDER: AccountStatus[] = ["Discovered", "Confirmed", "Keep", "DeletionPending", "Deleted", "NotAnAccount", "Ignored"];

const DIFFICULTY: Record<CatalogInfo["difficulty"], { label: string; dot: string }> = {
  easy: { label: "Einfach", dot: "bg-s-green" },
  medium: { label: "Mittel", dot: "bg-s-yellow" },
  hard: { label: "Aufwendig", dot: "bg-s-red" },
  impossible: { label: "Nicht möglich", dot: "bg-s-red" },
  unknown: { label: "Unbekannt", dot: "bg-s-gray" },
};

function Section({ title, children, aside }: { title: string; children: React.ReactNode; aside?: React.ReactNode }) {
  return (
    <section className="border-t border-border px-5 py-4">
      <div className="mb-2.5 flex items-center justify-between">
        <h3 className="text-sm font-medium text-fg-faint">{title}</h3>
        {aside}
      </div>
      {children}
    </section>
  );
}

function copy(text: string, what = "Kopiert") {
  navigator.clipboard?.writeText(text).then(
    () => toast(what),
    () => toast("Kopieren nicht möglich"),
  );
}

function Meta({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-sm text-fg-faint">{label}</dt>
      <dd className="text-base text-fg">{children}</dd>
    </div>
  );
}

function stepText(s: DeletionStep): string {
  const t = methodTag(s.method);
  return t === "DirectLink" ? "Löschseite geöffnet"
    : t === "GdprEmail" ? "DSGVO-Antrag versendet"
      : t === "SsoRevoked" ? "SSO-Zugriff widerrufen"
        : t === "Confirmed" ? "Löschung bestätigt" : "Sonstiger Schritt";
}

export function AccountDetail({ account: a }: { account: Account }) {
  const setStatus = useStore((s) => s.setAccountStatus);
  const setNotes = useStore((s) => s.setNotes);
  const markSent = useStore((s) => s.markDeletionSent);
  const [catalog, setCatalog] = useState<CatalogInfo | null>(null);
  const [letter, setLetter] = useState<GdprLetter | null>(null);
  const [letterOpen, setLetterOpen] = useState(false);
  const [notes, setNotesLocal] = useState(a.notes);

  useEffect(() => {
    let live = true;
    setCatalog(null);
    void api.catalogFor(a.domain).then((c) => live && setCatalog(c));
    return () => { live = false; };
  }, [a.domain]);

  useEffect(() => { setNotesLocal(a.notes); }, [a.id, a.notes]);

  useEffect(() => {
    if (new URLSearchParams(location.search).has("gdpr")) void openLetter();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [a.id]);

  async function openLetter() {
    setLetter(await api.gdprLetter(a.id));
    setLetterOpen(true);
  }

  const sources = Array.from(new Set(a.evidence.map((e) => sourceGroup(e.source))));
  const diff = catalog ? DIFFICULTY[catalog.difficulty] : null;
  const evidence = [...a.evidence].sort((x, y) => ((y.date ?? "") > (x.date ?? "") ? 1 : -1));
  const steps = [...a.deletion].sort((x, y) => (y.at > x.at ? 1 : -1));
  const hasRelay = a.identifiers.some(isApplePrivateRelay);

  const quick: { label: string; status: AccountStatus; primary?: boolean }[] =
    a.status === "Discovered" ? [{ label: "Bestätigen", status: "Confirmed", primary: true }, { label: "Behalten", status: "Keep" }]
      : a.status === "Confirmed" ? [{ label: "Löschung starten", status: "DeletionPending", primary: true }, { label: "Behalten", status: "Keep" }]
        : a.status === "DeletionPending" ? [{ label: "Als gelöscht markieren", status: "Deleted", primary: true }]
          : a.status === "Keep" ? [{ label: "Löschung starten", status: "DeletionPending" }]
            : [{ label: "Wiederherstellen", status: "Confirmed" }];

  return (
    <div key={a.id} className="selectable flex h-full flex-col overflow-y-auto">
      {/* header */}
      <div className="flex items-start gap-3 px-5 pb-4 pt-5">
        <ServiceLogo name={a.name} domain={a.domain} size={36} />
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-xl font-semibold leading-6 tracking-[-0.01em]">{a.name}</h2>
          <button
            onClick={() => void api.openUrl(`https://${a.domain}`)}
            className="mt-0.5 flex items-center gap-1 text-base text-fg-muted transition-colors hover:text-fg"
          >
            {a.domain} <ExternalLink className="size-3" />
          </button>
        </div>
        <Tooltip content="Schließen" shortcut="Esc">
          <Button variant="ghost" size="icon-sm" aria-label="Schließen" onClick={() => navigate({ page: "accounts" })}><X /></Button>
        </Tooltip>
      </div>

      {/* status actions */}
      <div className="flex flex-wrap items-center gap-2 px-5 pb-4">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" className="gap-2 pl-2.5 pr-2">
              <Dot className={STATUS_COLOR[a.status]} /> {ACCOUNT_STATUS_LABEL[a.status]} <ChevronDown className="!size-3 text-fg-faint" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent>
            <DropdownMenuRadioGroup value={a.status} onValueChange={(v) => void setStatus([a.id], v as AccountStatus)}>
              {STATUS_ORDER.map((s) => (
                <DropdownMenuRadioItem key={s} value={s}><Dot className={STATUS_COLOR[s]} />{ACCOUNT_STATUS_LABEL[s]}</DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>
        {quick.map((q) => (
          <Button key={q.status} variant={q.primary ? "primary" : "secondary"} onClick={() => void setStatus([a.id], q.status)}>
            {q.label}
          </Button>
        ))}
      </div>

      {/* meta */}
      <dl className="grid grid-cols-3 gap-4 px-5 pb-5">
        <Meta label="Sicherheit">
          <span className="tnum">{Math.round(a.confidence * 100)}%</span>
        </Meta>
        <Meta label="Zuerst gesehen">{formatDate(a.first_seen)}</Meta>
        <Meta label="Zuletzt gesehen">{relativeDays(a.last_seen)}</Meta>
      </dl>

      <Section title="Kennungen" aside={hasRelay ? <span className="text-sm text-fg-faint">Apple-Relay</span> : undefined}>
        <ul className="flex flex-col">
          {a.identifiers.map((id) => (
            <li key={id} className="group flex h-8 items-center gap-2">
              <span className="min-w-0 flex-1 truncate font-mono text-sm text-fg-muted" title={id}>{id}</span>
              <button
                aria-label="Kopieren"
                onClick={() => copy(id)}
                className="flex size-6 items-center justify-center rounded-md text-fg-faint opacity-0 transition hover:bg-muted hover:text-fg group-hover:opacity-100"
              >
                <Copy className="size-3.5" />
              </button>
            </li>
          ))}
        </ul>
        <p className="mt-1 text-sm text-fg-faint">Quellen: {sources.join(", ")}</p>
      </Section>

      {a.sso.length > 0 && (
        <Section title="Anmeldung über SSO">
          <div className="flex flex-wrap gap-1.5">
            {a.sso.map((p) => (
              <span key={ssoKey(p)} className="rounded-[4px] bg-muted px-1.5 py-0.5 text-xs text-fg-muted">{ssoLabel(p)}</span>
            ))}
          </div>
          <div className="mt-3 flex items-start gap-2 text-base text-fg-muted">
            <AlertTriangle className="mt-0.5 size-3.5 shrink-0 text-s-yellow" />
            <span>Erst löschen, dann SSO-Zugriff widerrufen. Sonst verlierst du den Zugang zum Konto.</span>
          </div>
          <div className="mt-2 flex flex-col items-start">
            {a.sso.map((p) => {
              const url = connectionsUrl(p);
              if (!url) return null;
              return (
                <Button key={ssoKey(p)} variant="ghost" size="sm" className="-ml-2 h-7 text-base" onClick={() => void api.openUrl(url)}>
                  Verbindungen bei {ssoLabel(p)} <ExternalLink />
                </Button>
              );
            })}
          </div>
        </Section>
      )}

      <Section
        title="Löschen"
        aside={diff && (
          <span className="flex items-center gap-1.5 text-sm text-fg-muted"><Dot className={diff.dot} /> {diff.label}</span>
        )}
      >
        {catalog?.note && <p className="mb-3 text-base text-fg-muted">{catalog.note}</p>}
        <div className="flex flex-wrap gap-2">
          <Button
            variant="secondary" disabled={!catalog?.deletion_url}
            onClick={() => catalog?.deletion_url && void api.openUrl(catalog.deletion_url)}
          >
            <ExternalLink /> Löschseite öffnen
          </Button>
          <Button variant="secondary" onClick={() => void openLetter()}><FileText /> DSGVO-Antrag</Button>
        </div>
        {!catalog?.deletion_url && catalog && <p className="mt-2 text-sm text-fg-faint">Keine direkte Löschseite bekannt. Nutze den DSGVO-Antrag.</p>}
        {steps.length > 0 && (
          <ul className="mt-4 flex flex-col gap-2.5">
            {steps.map((s, i) => {
              const dl = s.deadline ? daysFromNow(s.deadline) : null;
              return (
                <li key={i} className="flex items-start gap-2.5 text-base">
                  <Check className="mt-0.5 size-3.5 shrink-0 text-fg-faint" />
                  <div className="min-w-0 flex-1">
                    <div>{stepText(s)}</div>
                    <div className="text-sm text-fg-faint">{relativeDays(s.at)}{s.note ? ` · ${s.note}` : ""}</div>
                  </div>
                  {dl !== null && a.status === "DeletionPending" && (
                    <span className="flex shrink-0 items-center gap-1.5 text-sm text-fg-muted">
                      <Dot className={dl < 0 ? "bg-s-red" : dl <= 7 ? "bg-s-yellow" : "bg-s-gray"} />{deadlineLabel(s.deadline!)}
                    </span>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </Section>

      <Section title="Hinweise" aside={<span className="tnum text-sm text-fg-faint">{evidence.length}</span>}>
        <ol className="relative flex flex-col gap-3.5">
          <span className="absolute bottom-1 left-[2.5px] top-1 w-px bg-border" />
          {evidence.map((e, i) => (
            <li key={i} className="relative flex gap-3 pl-0.5">
              <span className="relative mt-[7px] size-[5px] shrink-0 rounded-full bg-fg-faint ring-4 ring-bg" />
              <div className="min-w-0 flex-1">
                <div className="truncate text-base">{e.summary}</div>
                <div className="text-sm text-fg-faint">{EVIDENCE_LABEL[e.kind]} · {sourceGroup(e.source)} · {relativeDays(e.date)}</div>
              </div>
            </li>
          ))}
        </ol>
      </Section>

      <Section title="Notizen">
        <Textarea
          value={notes} placeholder="Eigene Notizen zu diesem Account …" rows={3}
          onChange={(e) => setNotesLocal(e.target.value)}
          onBlur={() => notes !== a.notes && void setNotes(a.id, notes)}
        />
      </Section>
      <div className="h-6 shrink-0" />

      <Dialog open={letterOpen} onOpenChange={setLetterOpen}>
        <DialogContent className="max-w-[600px]">
          <DialogHeader>
            <DialogTitle>DSGVO-Löschantrag</DialogTitle>
            <DialogDescription>Auskunft und Löschung nach Art. 17 DSGVO. Der Anbieter muss binnen eines Monats antworten.</DialogDescription>
          </DialogHeader>
          {letter && (
            <div className="selectable flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-5 pb-4">
              <dl className="grid grid-cols-[56px_1fr] gap-x-3 gap-y-1 text-base">
                <dt className="text-fg-faint">An</dt><dd className="truncate">{letter.to}</dd>
                <dt className="text-fg-faint">Betreff</dt><dd className="truncate">{letter.subject}</dd>
              </dl>
              <pre className={cn("whitespace-pre-wrap rounded-md border border-border bg-muted/40 p-4 font-sans text-base leading-6 text-fg")}>{letter.body}</pre>
            </div>
          )}
          <DialogFooter>
            <Button variant="ghost" onClick={() => letter && copy(`${letter.subject}\n\n${letter.body}`, "Brief kopiert")}><Copy /> Kopieren</Button>
            <Button
              variant="secondary"
              onClick={() => letter && void api.openUrl(`mailto:${letter.to}?subject=${encodeURIComponent(letter.subject)}&body=${encodeURIComponent(letter.body)}`)}
            >
              <Mail /> Im Mailprogramm öffnen
            </Button>
            <Button
              variant="primary"
              onClick={async () => {
                if (!letter) return;
                await markSent(a.id, { GdprEmail: { to: letter.to } }, "DSGVO-Antrag nach Art. 17 versendet.");
                setLetterOpen(false);
                toast("Als gesendet markiert. Frist: 30 Tage.");
              }}
            >
              Als gesendet markieren
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
