import { useEffect, useState } from "react";
import { toast } from "sonner";
import { AlertTriangle, Check, ChevronDown, Copy, ExternalLink, FileText, X } from "lucide-react";
import { api } from "@/lib/api";
import { useStore } from "@/lib/store";
import { navigate } from "@/lib/router";
import { openUrl } from "@/lib/platform";
import { useAsync } from "@/lib/hooks";
import {
  ACCOUNT_STATUS_LABEL, connectionsUrl, DIFFICULTY_LABEL, EVIDENCE_LABEL, INTERVAL_LABEL, methodTag, paymentMethodLabel,
  REQUEST_KIND_LABEL, REQUEST_STATUS_LABEL, sourceGroup, ssoKey, ssoLabel,
  type Account, type AccountStatus, type DeletionStep,
} from "@/lib/types";
import { daysFromNow, de, deadlineLabel, formatDate, isApplePrivateRelay, money, relativeDays } from "@/lib/utils";
import { Dot, ServiceLogo, STATUS_COLOR } from "./common";
import { Button } from "./ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuRadioGroup, DropdownMenuRadioItem, DropdownMenuTrigger } from "./ui/dropdown-menu";
import { Textarea } from "./ui/input";
import { Tooltip } from "./ui/tooltip";
import { openLetter } from "./letter-dialog";

const STATUS_ORDER: AccountStatus[] = ["Discovered", "Confirmed", "Keep", "DeletionPending", "Deleted", "NotAnAccount", "Ignored"];

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

function copy(text: string) {
  navigator.clipboard?.writeText(text).then(() => toast("Kopiert"), () => toast.error("Kopieren nicht möglich"));
}

function Meta({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-sm text-fg-faint">{label}</dt>
      <dd className="text-base text-fg">{children}</dd>
    </div>
  );
}

function KV({ k, v, dot }: { k: string; v: React.ReactNode; dot?: string }) {
  return (
    <div className="flex min-h-7 items-start justify-between gap-4 text-base">
      <span className="text-fg-muted">{k}</span>
      <span className="flex items-center gap-2 text-right">{dot && <Dot className={dot} />}{v}</span>
    </div>
  );
}

function stepText(s: DeletionStep): string {
  const t = methodTag(s.method);
  return t === "DirectLink" ? "Löschseite geöffnet"
    : t === "GdprEmail" ? "Löschantrag versendet"
      : t === "SsoRevoked" ? "Zugriff beim Anbieter widerrufen"
        : t === "Confirmed" ? "Löschung bestätigt" : "Sonstiger Schritt";
}

export function AccountDetail({ account: a }: { account: Account }) {
  const setStatus = useStore((s) => s.setAccountStatus);
  const setNotes = useStore((s) => s.setNotes);
  const insights = useStore((s) => s.insights);
  const requests = useStore((s) => s.snap?.requests ?? []);
  const catalog = useAsync(() => api.data.catalogInfo({ domain: a.domain, name: a.name }), [a.domain]);
  const [notes, setNotesLocal] = useState(a.notes);
  useEffect(() => { setNotesLocal(a.notes); }, [a.id, a.notes]);

  const cat = catalog.data;
  const del = cat?.deletion ?? null;
  const diff = del ? DIFFICULTY_LABEL[del.difficulty] : null;
  const billing = insights?.billing.find((b) => b.account_id === a.id) ?? null;
  const risk = insights?.risks.find((r) => r.account_id === a.id) ?? null;
  const mine = requests.filter((r) => r.account_id === a.id).sort((x, y) => (y.created_at > x.created_at ? 1 : -1));
  const sources = Array.from(new Set(a.evidence.map((e) => sourceGroup(e.source))));
  const evidence = [...a.evidence].sort((x, y) => ((y.date ?? "") > (x.date ?? "") ? 1 : -1));
  const steps = [...a.deletion].sort((x, y) => (y.at > x.at ? 1 : -1));
  const hasRelay = a.identifiers.some(isApplePrivateRelay);
  const sec = a.security;
  const unresolved = a.domain.startsWith("unresolved:");
  const letter = (kind: "Erasure" | "Access" | "Cancellation") =>
    openLetter({ args: { kind, accountId: a.id }, target: a.name, domain: unresolved ? undefined : a.domain });

  const quick: { label: string; status: AccountStatus; primary?: boolean }[] =
    a.status === "Discovered" ? [{ label: "Bestätigen", status: "Confirmed", primary: true }, { label: "Behalten", status: "Keep" }]
      : a.status === "Confirmed" ? [{ label: "Löschung starten", status: "DeletionPending", primary: true }, { label: "Behalten", status: "Keep" }]
        : a.status === "DeletionPending" ? [{ label: "Als gelöscht markieren", status: "Deleted", primary: true }]
          : a.status === "Keep" ? [{ label: "Löschung starten", status: "DeletionPending" }]
            : [{ label: "Wiederherstellen", status: "Confirmed" }];

  const via = billing?.via ? billing.via : billing?.methods.some((m) => m === "PayPal") ? "PayPal" : null;
  const hub = via ? ({ Apple: "Apple Abos", "Google Play": "Google Play Abos", PayPal: "PayPal Automatische Zahlungen" } as Record<string, string>)[via] : null;

  return (
    <div key={a.id} className="selectable flex h-full flex-col overflow-y-auto">
      <div className="flex items-start gap-3 px-5 pb-4 pt-5">
        <ServiceLogo name={a.name} domain={a.domain} size={36} />
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-xl font-semibold leading-6 tracking-[-0.01em]" title={a.name}>{a.name}</h2>
          {unresolved ? <span className="mt-0.5 block text-base text-fg-faint">Domain unbekannt</span> : (
            <button onClick={() => void openUrl(`https://${a.domain}`)} className="mt-0.5 flex max-w-full items-center gap-1 text-base text-fg-muted transition-colors hover:text-fg">
              <span className="truncate">{a.domain}</span> <ExternalLink className="size-3 shrink-0" />
            </button>
          )}
        </div>
        <Tooltip content="Schließen" shortcut="Esc">
          <Button variant="ghost" size="icon-sm" aria-label="Schließen" onClick={() => navigate({ page: "accounts" })}><X /></Button>
        </Tooltip>
      </div>

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
          <Button key={q.status} variant={q.primary ? "primary" : "secondary"} onClick={() => void setStatus([a.id], q.status)}>{q.label}</Button>
        ))}
      </div>

      <dl className="grid grid-cols-3 gap-4 px-5 pb-5">
        <Meta label="Erkannt"><span className="tnum">{Math.round(a.confidence * 100)}%</span></Meta>
        <Meta label="Zuerst gesehen">{formatDate(a.first_seen)}</Meta>
        <Meta label="Zuletzt gesehen">{relativeDays(a.last_seen)}</Meta>
      </dl>

      {risk && risk.score > 0 && (
        <div className="mx-5 mb-4 flex items-start gap-2 rounded-md bg-muted px-3 py-2 text-base">
          <Dot className={`mt-[7px] ${risk.score >= 60 ? "bg-s-red" : "bg-s-yellow"}`} />
          <span>{de(risk.next_step_de)}</span>
        </div>
      )}

      <Section title="Kennungen" aside={hasRelay ? <span className="text-sm text-fg-faint">Apple Relay</span> : undefined}>
        <ul className="flex flex-col">
          {a.identifiers.length === 0 && <li className="text-base text-fg-faint">Keine Kennung bekannt</li>}
          {a.identifiers.map((id) => (
            <li key={id} className="group flex h-8 items-center gap-2">
              <span className="min-w-0 flex-1 truncate font-mono text-sm text-fg-muted" title={id}>{id}</span>
              <button aria-label="Kopieren" onClick={() => copy(id)} className="flex size-6 items-center justify-center rounded-md text-fg-faint opacity-0 transition hover:bg-muted hover:text-fg focus-visible:opacity-100 group-hover:opacity-100">
                <Copy className="size-3.5" />
              </button>
            </li>
          ))}
        </ul>
        <p className="mt-1 text-sm text-fg-faint">Quellen: {sources.join(", ")}</p>
      </Section>

      {a.sso.length > 0 && (
        <Section title="Anmeldung über Drittanbieter">
          <div className="flex flex-wrap gap-1.5">
            {a.sso.map((p) => <span key={ssoKey(p)} className="rounded-[4px] bg-muted px-1.5 py-0.5 text-xs text-fg-muted">{ssoLabel(p)}</span>)}
          </div>
          <div className="mt-3 flex items-start gap-2 text-base text-fg-muted">
            <AlertTriangle className="mt-0.5 size-3.5 shrink-0 text-s-yellow" />
            <span>Erst den Account löschen, dann den Zugriff beim Anbieter widerrufen. Sonst verlierst du den Zugang.</span>
          </div>
          <div className="mt-2 flex flex-col items-start">
            {a.sso.map((p) => {
              const url = connectionsUrl(p);
              if (!url) return null;
              return (
                <Button key={ssoKey(p)} variant="ghost" size="sm" className="-ml-2 h-7 text-base" onClick={() => void openUrl(url)}>
                  Verbundene Apps bei {ssoLabel(p)} <ExternalLink />
                </Button>
              );
            })}
          </div>
        </Section>
      )}

      <Section title="Sicherheit">
        <div className="flex flex-col">
          <KV k="Passwort" v={sec.password_pwned ? "in einem Leak gefunden" : (sec.password_reuse_count ?? 0) > 0 ? `bei ${sec.password_reuse_count} weiteren Diensten genutzt` : sec.password_pwned === false ? "unauffällig" : "nicht geprüft"}
            dot={sec.password_pwned ? "bg-s-red" : (sec.password_reuse_count ?? 0) > 0 ? "bg-s-yellow" : sec.password_pwned === false ? "bg-s-green" : "bg-s-gray"} />
          <KV k="2FA" v={sec.has_2fa ? "aktiv" : cat?.two_factor ? "nicht aktiv, verfügbar" : sec.has_2fa === false ? "nicht aktiv" : "unbekannt"}
            dot={sec.has_2fa ? "bg-s-green" : cat?.two_factor ? "bg-s-yellow" : "bg-s-gray"} />
          {sec.phone_login && <KV k="Anmeldung" v="mit Telefonnummer" dot="bg-s-yellow" />}
        </div>
        {!sec.has_2fa && cat?.two_factor?.documentation && (
          <Button variant="ghost" size="sm" className="-ml-2 mt-1 h-7 text-base" onClick={() => void openUrl(cat.two_factor!.documentation!)}>
            2FA einrichten ({cat.two_factor.methods.slice(0, 2).join(", ")}) <ExternalLink />
          </Button>
        )}
        {sec.breaches.length > 0 && (
          <ul className="mt-3 flex flex-col gap-2.5">
            {sec.breaches.map((b) => (
              <li key={b.name} className="flex items-start gap-2.5 text-base">
                <Dot className={`mt-[7px] ${b.confirmed_for_user ? "bg-s-red" : "bg-s-yellow"}`} />
                <div className="min-w-0 flex-1">
                  <div>{b.title}{b.breach_date ? `, ${new Date(b.breach_date).getFullYear()}` : ""}</div>
                  <div className="text-sm text-fg-faint">{b.confirmed_for_user ? "Deine Adresse ist betroffen. " : "Der Dienst war betroffen. "}{b.data_classes.join(", ")}</div>
                </div>
              </li>
            ))}
          </ul>
        )}
      </Section>

      {billing && (
        <Section title="Zahlungen">
          <div className="flex flex-col">
            {billing.active_subscription && (
              <KV k={billing.plan ?? "Abo"} v={<span className="tnum">{money(billing.amount_minor, billing.currency)} {billing.interval ? INTERVAL_LABEL[billing.interval] : ""}</span>} dot="bg-s-blue" />
            )}
            {billing.next_charge_estimate && billing.active_subscription && <KV k="Nächste Abbuchung" v={<span className="tnum">{formatDate(billing.next_charge_estimate)}</span>} />}
            {billing.price_increased && <KV k="Preis" v="wurde erhöht" dot="bg-s-yellow" />}
            {billing.possibly_unused && <KV k="Nutzung" v="vermutlich ungenutzt" dot="bg-s-yellow" />}
            {billing.cancelled_at && <KV k="Gekündigt" v={formatDate(billing.cancelled_at)} />}
            {billing.spent_last_12m.map(([c, v]) => <KV key={c} k="Letzte 12 Monate" v={<span className="tnum">{money(v, c)}</span>} />)}
            <KV k="Zahlart" v={billing.methods.map(paymentMethodLabel).join(", ") || "unbekannt"} />
          </div>
          {billing.active_subscription && (
            <div className="mt-2 flex flex-wrap gap-2">
              {hub && <span className="text-base text-fg-muted">Läuft über {via}. Kündigen unter „{hub}“.</span>}
              <Button variant="secondary" onClick={() => letter("Cancellation")}><FileText /> Kündigung</Button>
            </div>
          )}
        </Section>
      )}

      <Section
        title="Löschen"
        aside={diff && <span className="flex items-center gap-1.5 text-sm text-fg-muted"><Dot className={diff.dot} /> {diff.label}</span>}
      >
        {del?.notes && <p className="mb-3 text-base text-fg-muted">{de(del.notes)}</p>}
        <div className="flex flex-wrap gap-2">
          <Button variant="secondary" disabled={!del?.url} onClick={() => del?.url && void openUrl(del.url)}><ExternalLink /> Löschseite öffnen</Button>
          <Button variant="secondary" onClick={() => letter("Erasure")}><FileText /> Löschantrag</Button>
          <Button variant="secondary" onClick={() => letter("Access")}><FileText /> Auskunft</Button>
        </div>
        {catalog.error && <p className="mt-2 text-sm text-fg-faint">Katalog nicht verfügbar. Der Löschantrag funktioniert trotzdem.</p>}
        {!del?.url && cat && !catalog.error && <p className="mt-2 text-sm text-fg-faint">Keine Löschseite bekannt. Nutze den Löschantrag.</p>}
        {steps.length > 0 && (
          <ul className="mt-4 flex flex-col gap-2.5">
            {steps.map((s, i) => {
              const dl = s.deadline ? daysFromNow(s.deadline) : null;
              return (
                <li key={i} className="flex items-start gap-2.5 text-base">
                  <Check className="mt-0.5 size-3.5 shrink-0 text-fg-faint" />
                  <div className="min-w-0 flex-1">
                    <div>{stepText(s)}</div>
                    <div className="text-sm text-fg-faint">{relativeDays(s.at)}{s.note ? `, ${s.note}` : ""}</div>
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

      {mine.length > 0 && (
        <Section title="Anträge" aside={<button className="text-sm text-fg-faint hover:text-fg" onClick={() => navigate({ page: "requests" })}>Alle</button>}>
          <ul className="flex flex-col gap-2.5">
            {mine.map((r) => {
              const dl = r.deadline && r.status === "Sent" ? daysFromNow(r.deadline) : null;
              return (
                <li key={r.id} className="flex items-start gap-2.5 text-base">
                  <Dot className={`mt-[7px] ${dl !== null && dl < 0 ? "bg-s-red" : r.status === "Sent" ? "bg-s-yellow" : r.status === "Draft" ? "bg-s-gray" : "bg-s-green"}`} />
                  <div className="min-w-0 flex-1">
                    <div>{REQUEST_KIND_LABEL[r.kind]}, {REQUEST_STATUS_LABEL[r.status]}</div>
                    <div className="text-sm text-fg-faint">{r.sent_at ? `Gesendet ${relativeDays(r.sent_at)}` : "Noch nicht gesendet"}</div>
                  </div>
                  {dl !== null && <span className="shrink-0 text-sm text-fg-muted">{deadlineLabel(r.deadline!)}</span>}
                </li>
              );
            })}
          </ul>
        </Section>
      )}

      <Section title="Hinweise" aside={<span className="tnum text-sm text-fg-faint">{evidence.length}</span>}>
        <ol className="relative flex flex-col gap-3.5">
          <span className="absolute bottom-1 left-[2.5px] top-1 w-px bg-border" />
          {evidence.map((e, i) => (
            <li key={i} className="relative flex gap-3 pl-0.5">
              <span className="relative mt-[7px] size-[5px] shrink-0 rounded-full bg-fg-faint ring-4 ring-bg" />
              <div className="min-w-0 flex-1">
                <div className="truncate text-base" title={e.summary}>{de(e.summary)}</div>
                <div className="text-sm text-fg-faint">{EVIDENCE_LABEL[e.kind]}, {sourceGroup(e.source)}, {relativeDays(e.date)}</div>
              </div>
            </li>
          ))}
        </ol>
      </Section>

      <Section title="Notizen">
        <Textarea
          value={notes} placeholder="Eigene Notizen" rows={3}
          onChange={(e) => setNotesLocal(e.target.value)}
          onBlur={() => notes !== a.notes && void setNotes(a.id, notes)}
        />
      </Section>
      <div className="h-6 shrink-0" />
    </div>
  );
}
