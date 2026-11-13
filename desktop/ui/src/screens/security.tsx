import { useState } from "react";
import { toast } from "sonner";
import { ExternalLink, Loader2, ShieldCheck } from "lucide-react";
import { api, runJob } from "@/lib/api";
import { useStore } from "@/lib/store";
import { href, navigate } from "@/lib/router";
import { openUrl } from "@/lib/platform";
import type { Account, BreachDonePayload, RiskReason } from "@/lib/types";
import { de, errorMessage, formatDate, plural } from "@/lib/utils";
import { Dot, EmptyState, ErrorState, Figure, PageHeader, SectionLabel, ServiceLogo } from "@/components/common";
import { PageShell, Panel } from "@/components/page";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

interface Row { a: Account; detail: string; dot: string; changePw?: boolean }

const realDomain = (a: Account) => !a.domain.startsWith("unresolved:") && a.domain.includes(".");

function Issue({ title, rows, empty, hint }: { title: string; rows: Row[]; empty?: string; hint?: string }) {
  const [all, setAll] = useState(false);
  const shown = all ? rows : rows.slice(0, 6);
  return (
    <section>
      <SectionLabel right={rows.length > 6 ? <button className="text-sm text-fg-faint hover:text-fg" onClick={() => setAll(!all)}>{all ? "Weniger" : `Alle ${rows.length}`}</button> : undefined}>
        {title} <span className="tnum ml-1">{rows.length}</span>
      </SectionLabel>
      {hint && <p className="mt-0.5 text-base text-fg-muted">{hint}</p>}
      <Panel className="mt-1.5">
        {rows.length === 0 ? <div className="flex h-11 items-center gap-2 px-3 text-base text-fg-muted"><Dot className="bg-s-green" />{empty ?? "Nichts gefunden"}</div> : shown.map(({ a, detail, dot, changePw }) => (
          <a key={a.id} href={href({ page: "accounts", id: a.id })} className="flex h-11 items-center gap-3 border-b border-border px-3 transition-colors last:border-b-0 hover:bg-muted">
            <ServiceLogo name={a.name} domain={a.domain} />
            <span className="min-w-0 flex-1 truncate text-base font-medium">{a.name}</span>
            <span className="flex max-w-[55%] items-center gap-2 truncate text-sm text-fg-muted"><Dot className={dot} /><span className="truncate">{detail}</span></span>
            {changePw && realDomain(a) && (
              <Button
                size="sm" className="ml-1"
                onClick={(e) => { e.preventDefault(); e.stopPropagation(); void openUrl(`https://${a.domain}/.well-known/change-password`); }}
              >Passwort ändern <ExternalLink /></Button>
            )}
          </a>
        ))}
      </Panel>
    </section>
  );
}

export function SecurityPage() {
  const snap = useStore((s) => s.snap)!;
  const ins = useStore((s) => s.insights);
  const err = useStore((s) => s.insightsError);
  const refresh = useStore((s) => s.refresh);
  const reference = useStore((s) => s.reference);
  const ensure = useStore((s) => s.ensureReference);
  const [busy, setBusy] = useState(false);
  if (!reference) void ensure();

  async function check() {
    setBusy(true);
    try { await runJob<BreachDonePayload>(() => api.security.breachCheckStart()); } catch (e) { toast.error(errorMessage(e)); } finally { setBusy(false); }
  }

  const head = (
    <PageHeader title="Sicherheit" actions={<Button variant="primary" disabled={busy} onClick={() => void check()}>{busy ? <Loader2 className="animate-spin" /> : <ShieldCheck />} Datenlecks prüfen</Button>} />
  );
  if (!ins) return <PageShell>{head}{err ? <ErrorState message={err} onRetry={() => void refresh()} /> : <Skeleton className="h-40" />}</PageShell>;
  if (snap.accounts.length === 0) {
    return <PageShell>{head}<EmptyState icon={ShieldCheck} className="py-20" title="Noch nichts zu prüfen" description="Importiere einen Passwortexport oder scanne ein Postfach. Dann siehst du hier geleakte und mehrfach genutzte Passwörter." action={<Button variant="primary" onClick={() => navigate({ page: "sources" })}>Zu den Quellen</Button>} /></PageShell>;
  }

  const open = snap.accounts.filter((a) => !["Deleted", "NotAnAccount", "Ignored"].includes(a.status));
  const reason = (r: RiskReason) => new Set(ins.risks.filter((x) => x.reasons.includes(r)).map((x) => x.account_id));
  const no2fa = reason("No2faButSupported"), phone = reason("PhoneLogin"), foreign = reason("ForeignRecoveryAddress");
  const byId = new Map(snap.accounts.map((a) => [a.id, a]));

  const pwned: Row[] = open.filter((a) => a.security.password_pwned).map((a) => ({ a, detail: "Passwort in einem Leak gefunden", dot: "bg-s-red", changePw: true }));
  const reused: Row[] = open.filter((a) => (a.security.password_reuse_count ?? 0) > 0).sort((x, y) => (y.security.password_reuse_count ?? 0) - (x.security.password_reuse_count ?? 0))
    .map((a) => ({ a, detail: `auch bei ${plural(a.security.password_reuse_count!, "anderem Dienst", "anderen Diensten")} genutzt`, dot: "bg-s-yellow", changePw: true }));
  const breached: Row[] = open.filter((a) => a.security.breaches.length > 0).map((a) => {
    const b = a.security.breaches[0]!;
    return { a, detail: `${b.title}${b.breach_date ? ` ${new Date(b.breach_date).getFullYear()}` : ""}${b.confirmed_for_user ? ", deine Adresse betroffen" : ""}`, dot: a.security.breaches.some((x) => x.confirmed_for_user) ? "bg-s-red" : "bg-s-yellow", changePw: a.security.breaches.some((x) => x.confirmed_for_user) };
  }).sort((x, y) => (x.dot === "bg-s-red" ? 0 : 1) - (y.dot === "bg-s-red" ? 0 : 1));
  const rowsFor = (set: Set<string>, detail: string): Row[] => [...set].map((id) => byId.get(id)).filter((a): a is Account => !!a).map((a) => ({ a, detail, dot: "bg-s-yellow" }));
  const aliasRows: Row[] = ins.aliases.map((x) => ({ a: byId.get(x.account_id)!, detail: x.current_identifier, dot: "bg-s-gray" })).filter((r) => r.a);
  const checked = open.some((a) => a.security.checked_at);

  return (
    <PageShell>
      {head}
      {!snap.settings.has_hibp_key && (
        <p className="-mt-3 text-base text-fg-muted">
          Ohne Schlüssel für Have I Been Pwned prüft ausgeloggt nur die öffentliche Leckliste. Mit Schlüssel werden auch deine Adressen abgefragt.{" "}
          <button className="text-fg underline underline-offset-2" onClick={() => navigate({ page: "settings", section: "security" })}>Schlüssel hinterlegen</button>
        </p>
      )}
      <div className="grid grid-cols-4 gap-6 max-lg:grid-cols-2">
        <Figure label="Geleakte Passwörter" value={pwned.length} hint={checked ? "" : "noch nicht geprüft"} />
        <Figure label="Mehrfach genutzt" value={reused.length} />
        <Figure label="Betroffene Dienste" value={breached.length} />
        <Figure label="2FA fehlt" value={no2fa.size} hint="obwohl verfügbar" />
      </div>
      <Issue title="Geleakte Passwörter" rows={pwned} empty={checked ? "Kein geleaktes Passwort gefunden" : "Importiere einen Passwortexport mit Prüfung"} />
      <Issue title="Mehrfach genutzte Passwörter" rows={reused} empty="Kein Passwort mehrfach genutzt" />
      <Issue title="Datenlecks bei Diensten" rows={breached} empty="Keine Datenlecks bekannt" />
      <Issue title="2FA fehlt" rows={rowsFor(no2fa, "2FA ist verfügbar, aber nicht aktiv")} empty="Überall aktiv, wo möglich" />
      <Issue title="Anmeldung per Telefonnummer" rows={rowsFor(phone, "Telefonnummer als Login")} empty="Keine gefunden" hint="Eine Rufnummer lässt sich übernehmen. Wer sie bekommt, kommt in den Account." />
      <Issue title="Fremde Wiederherstellungsadresse" rows={rowsFor(foreign, "Adresse nicht mehr unter deiner Kontrolle")} empty="Keine gefunden" hint="Diese Accounts nutzen eine Mailadresse, die dir nicht mehr gehört, zum Beispiel vom früheren Arbeitgeber." />
      <section>
        <SectionLabel>Alias statt Hauptadresse <span className="tnum ml-1">{aliasRows.length}</span></SectionLabel>
        <p className="mt-0.5 text-base text-fg-muted">{ins.aliases[0] ? de(ins.aliases[0].reason_de) : "Für Accounts, die du behältst, lohnt sich eine Aliasadresse."}</p>
        {aliasRows.length > 0 && <Panel className="mt-1.5">
          {aliasRows.slice(0, 5).map(({ a, detail }) => (
            <a key={a.id} href={href({ page: "accounts", id: a.id })} className="flex h-11 items-center gap-3 border-b border-border px-3 transition-colors last:border-b-0 hover:bg-muted">
              <ServiceLogo name={a.name} domain={a.domain} />
              <span className="min-w-0 flex-1 truncate text-base font-medium">{a.name}</span>
              <span className="truncate text-sm text-fg-muted">{detail}</span>
            </a>
          ))}
        </Panel>}
        <div className="mt-2 flex flex-wrap gap-x-4">
          {reference?.alias_services.map((s) => (
            <Button key={s.url} variant="ghost" size="sm" className="-ml-2 h-7 text-base" onClick={() => void openUrl(s.url)}>{s.title_de} <ExternalLink /></Button>
          ))}
        </div>
      </section>
      <p className="text-sm text-fg-faint">{checked ? `Zuletzt geprüft: ${formatDate(open.map((a) => a.security.checked_at).filter(Boolean).sort().reverse()[0])}` : ""}</p>
    </PageShell>
  );
}
