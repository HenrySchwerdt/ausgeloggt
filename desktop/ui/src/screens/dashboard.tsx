import { ArrowRight, Check, CalendarClock, Sparkles } from "lucide-react";
import { useStore } from "@/lib/store";
import { navigate, href } from "@/lib/router";
import { daysFromNow, de, money, formatDate, plural } from "@/lib/utils";
import { isOverdue, requestDays, setupState } from "@/lib/derive";
import { REQUEST_KIND_LABEL } from "@/lib/types";
import { Dot, EmptyState, ErrorState, Figure, PageHeader, riskDot, SectionLabel, ServiceLogo } from "@/components/common";
import { PageShell, Panel } from "@/components/page";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Progress } from "@/components/ui/progress";
import { openLetter } from "@/components/letter-dialog";
import { cn } from "@/lib/utils";

type Setup = ReturnType<typeof setupState>;

/** The one thing to do next, with its button. */
function nextStep(snap: ReturnType<typeof useStore.getState>["snap"] & object, st: Setup) {
  const overdue = snap.requests.filter(isOverdue).length;
  if (!st.hasSource) {
    return { title: "Quelle verbinden", text: "Ein Postfach oder ein Passwortexport zeigt uns, wo du Accounts hast.", button: "Quelle verbinden", go: () => navigate({ page: "sources" }) };
  }
  if (st.review > 0) {
    const s = st.stats;
    return {
      title: `${plural(st.review, "Fund", "Funde")} prüfen`,
      text: `${s.high} sicher, ${s.mid} wahrscheinlich${s.low ? `, ${s.low} unsicher` : ""}. Du entscheidest bei jedem mit einem Klick, ob du dort einen Account hast.`,
      button: "Prüfen", go: () => navigate({ page: "review" }),
    };
  }
  if (overdue > 0) {
    return { title: `${plural(overdue, "Antrag", "Anträge")} überfällig`, text: "Der Dienst hat nicht in der Frist geantwortet. Du kannst nachfassen.", button: "Zu den Anträgen", go: () => navigate({ page: "requests" }) };
  }
  if (st.confirmed > 0) {
    return { title: `${plural(st.confirmed, "Account", "Accounts")} aufräumen`, text: "Löschen, kündigen oder behalten. Wir führen dich Schritt für Schritt.", button: "Aufräumen", go: () => navigate({ page: "cleanup", tidy: true }) };
  }
  if (st.weak > 0) {
    return { title: `${plural(st.weak, "unsicherer Fund", "unsichere Funde")}`, text: "Hier haben wir kaum Hinweise, zum Beispiel nur Newsletter. Du kannst sie trotzdem durchsehen.", button: "Ansehen", go: () => navigate({ page: "review" }) };
  }
  return { title: "Alles erledigt", text: "Prüfe dein Postfach ab und zu neu, damit neue Accounts nicht untergehen.", button: "Postfach prüfen", go: () => navigate({ page: "sources" }) };
}

function Steps({ st }: { st: Setup }) {
  const s2done = st.hasSource && st.review === 0 && st.decided;
  const s3done = s2done && st.confirmed === 0;
  const steps = [
    {
      title: "Quelle verbinden",
      text: "Dein Postfach oder ein Passwortexport zeigt uns, wo du Accounts hast. Alles bleibt auf deinem Gerät.",
      done: st.hasSource, status: st.hasSource ? "Verbunden" : "Noch keine Quelle",
      button: "Quelle verbinden", go: () => navigate({ page: "sources" }), ready: true,
    },
    {
      title: "Funde prüfen",
      text: "Wir zeigen dir jeden Fund einzeln, mit den Gründen. Du sagst nur ja oder nein.",
      done: s2done, status: !st.hasSource ? "Zuerst Schritt 1" : st.review > 0 ? `${st.review} zu prüfen` : st.decided ? "Geprüft" : "Nichts zu prüfen",
      button: "Prüfen", go: () => navigate({ page: "review" }), ready: st.hasSource && st.review > 0,
    },
    {
      title: "Aufräumen",
      text: "Account löschen, Abo kündigen oder behalten. Wir öffnen die richtige Seite und helfen bei Briefen.",
      done: s3done, status: st.confirmed > 0 ? `${st.confirmed} bereit` : s2done ? "Nichts offen" : "Nach Schritt 2",
      button: "Aufräumen", go: () => navigate({ page: "cleanup", tidy: true }), ready: st.confirmed > 0,
    },
  ];
  const current = steps.findIndex((x) => !x.done);
  return (
    <section>
      <SectionLabel>In drei Schritten zum aufgeräumten Postfach</SectionLabel>
      <Panel className="mt-1">
        {steps.map((s, i) => {
          const isCurrent = i === current;
          return (
            <div key={s.title} className={cn("flex items-center gap-4 border-b border-border px-4 py-4 last:border-b-0", !isCurrent && !s.done && "opacity-70")}>
              <span className={cn("flex size-6 shrink-0 items-center justify-center rounded-full border text-sm", s.done ? "border-transparent bg-muted text-fg-muted" : isCurrent ? "border-fg text-fg" : "border-border-strong text-fg-faint")}>
                {s.done ? <Check className="size-3.5" /> : <span className="tnum">{i + 1}</span>}
              </span>
              <div className="min-w-0 flex-1">
                <div className="flex items-baseline gap-2.5">
                  <span className="text-lg font-medium">{s.title}</span>
                  <span className="text-sm text-fg-faint">{s.status}</span>
                </div>
                <p className="mt-0.5 text-base text-fg-muted">{s.text}</p>
              </div>
              <Button variant={isCurrent ? "primary" : "secondary"} disabled={!s.ready && !isCurrent} onClick={s.go}>{s.button}</Button>
            </div>
          );
        })}
      </Panel>
    </section>
  );
}

export function Dashboard() {
  const snap = useStore((s) => s.snap)!;
  const ins = useStore((s) => s.insights);
  const insErr = useStore((s) => s.insightsError);
  const refresh = useStore((s) => s.refresh);

  const byId = new Map(snap.accounts.map((a) => [a.id, a]));
  const st = setupState(snap);

  // Until the user decided anything there is nothing meaningful to score: show the guided start instead.
  const firstRun = !st.decided || !st.hasSource || (ins ? !ins.score.has_data : false);
  if (firstRun) {
    return (
      <PageShell>
        <PageHeader title="Übersicht" description="Wir finden, wo du angemeldet bist, und räumen mit dir auf." />
        <Steps st={st} />
        {snap.accounts.length === 0 && (
          <p className="text-base text-fg-muted">Noch nichts gefunden. Nach Schritt 1 erscheinen hier deine Accounts.</p>
        )}
      </PageShell>
    );
  }
  if (!ins) {
    return (
      <PageShell>
        <PageHeader title="Übersicht" />
        {insErr ? <ErrorState message={insErr} onRetry={() => void refresh()} /> : (
          <div className="grid grid-cols-4 gap-6 max-lg:grid-cols-2"><Skeleton className="h-16" /><Skeleton className="h-16" /><Skeleton className="h-16" /><Skeleton className="h-16" /></div>
        )}
      </PageShell>
    );
  }

  const sc = ins.score;
  const step = nextStep(snap, st);
  const risks = ins.risks
    .filter((r) => {
      const s = byId.get(r.account_id)?.status;
      return r.score > 0 && (s === "Confirmed" || s === "Keep");
    })
    .slice(0, 6);
  const renewals = ins.billing
    .filter((b) => b.active_subscription && b.next_charge_estimate)
    .sort((x, y) => (x.next_charge_estimate! < y.next_charge_estimate! ? -1 : 1))
    .slice(0, 5);
  const overdue = snap.requests.filter(isOverdue).sort((a, b) => (requestDays(a) ?? 0) - (requestDays(b) ?? 0));
  const monthly = ins.totals[0];

  return (
    <PageShell>
      <PageHeader title="Übersicht" />

      <section className="flex items-center gap-5 rounded-lg border border-border px-5 py-4">
        <div className="min-w-0 flex-1">
          <div className="text-sm text-fg-faint">Nächster Schritt</div>
          <div className="mt-0.5 text-lg font-semibold">{step.title}</div>
          <p className="mt-0.5 text-base text-fg-muted">{step.text}</p>
        </div>
        <Button variant="primary" onClick={step.go}>{step.button} <ArrowRight /></Button>
      </section>

      <div className="grid grid-cols-4 gap-6 max-lg:grid-cols-2 max-[620px]:grid-cols-1">
        <Figure
          label="Datenschutz Score"
          value={sc.has_data ? <>{sc.score}<span className="ml-1 text-lg font-normal text-fg-faint">von 100</span></> : <span className="text-lg font-normal text-fg-muted">noch keine Daten</span>}
          hint={sc.has_data ? undefined : ""}
        />
        <Figure label="Zu prüfen" value={st.review} hint={st.weak ? `${st.weak} unsichere dazu` : ""} />
        <Figure label="Zum Aufräumen" value={st.confirmed} hint={sc.deleted_accounts ? `${sc.deleted_accounts} gelöscht` : ""} />
        <Figure
          label="Offene Anträge" value={sc.pending_requests}
          hint={sc.overdue_requests ? `${sc.overdue_requests} überfällig` : ""} hintClass={sc.overdue_requests ? "text-s-red" : undefined}
        />
      </div>

      <div className="grid grid-cols-[1.35fr_1fr] gap-7 max-lg:grid-cols-1">
        <section>
          <SectionLabel right={<a href={href({ page: "cleanup" })} className="text-sm text-fg-faint hover:text-fg">Alle</a>}>Größte Risiken</SectionLabel>
          <Panel className="mt-1">
            {risks.length === 0 ? (
              <EmptyState icon={Sparkles} title="Keine Risiken gefunden" description="Hier erscheinen bestätigte Accounts mit Risiken, zum Beispiel ohne 2FA oder mit geleaktem Passwort." className="py-10" />
            ) : risks.map((r) => {
              const a = byId.get(r.account_id)!;
              return (
                <a key={r.account_id} href={href({ page: "accounts", id: a.id })} className="flex h-[52px] items-center gap-3 border-b border-border px-3 transition-colors last:border-b-0 hover:bg-muted">
                  <ServiceLogo name={a.name} domain={a.domain} />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-base font-medium leading-[18px]">{a.name}</div>
                    <div className="truncate text-sm leading-4 text-fg-muted">{de(r.next_step_de)}</div>
                  </div>
                  <span className="flex items-center gap-2 text-sm text-fg-muted"><Dot className={riskDot(r.score)} /><span className="tnum">{r.score}</span></span>
                </a>
              );
            })}
          </Panel>
        </section>

        <div className="flex flex-col gap-7">
          <section>
            <SectionLabel right={<a href={href({ page: "billing" })} className="text-sm text-fg-faint hover:text-fg">Alle</a>}>
              Nächste Abbuchungen
            </SectionLabel>
            <Panel className="mt-1">
              {renewals.length === 0 ? (
                <EmptyState icon={CalendarClock} title="Keine Abos gefunden" description="Abos erkennen wir an Rechnungen und Zahlungsmails in deinem Postfach." className="py-10" />
              ) : renewals.map((b) => {
                const a = byId.get(b.account_id);
                if (!a) return null;
                const d = daysFromNow(b.next_charge_estimate!);
                return (
                  <a key={b.account_id} href={href({ page: "billing" })} className="flex h-11 items-center gap-3 border-b border-border px-3 transition-colors last:border-b-0 hover:bg-muted">
                    <ServiceLogo name={a.name} domain={a.domain} />
                    <span className="min-w-0 flex-1 truncate text-base font-medium">{a.name}</span>
                    <span className="tnum text-sm text-fg-muted">{d <= 0 ? "heute" : d === 1 ? "morgen" : `in ${d} Tagen`}</span>
                    <span className="tnum w-[70px] text-right text-base">{money(b.amount_minor, b.currency)}</span>
                  </a>
                );
              })}
            </Panel>
            {monthly && <p className="mt-2 text-sm text-fg-muted">{plural(monthly[3], "Abo", "Abos")}, zusammen {money(monthly[1], monthly[0])} im Monat</p>}
          </section>

          {overdue.length > 0 && (
            <section>
              <SectionLabel right={<a href={href({ page: "requests" })} className="text-sm text-fg-faint hover:text-fg">Alle</a>}>Überfällige Anträge</SectionLabel>
              <Panel className="mt-1">
                {overdue.slice(0, 4).map((r) => (
                  <div key={r.id} className="flex min-h-11 items-center gap-3 border-b border-border px-3 last:border-b-0">
                    <Dot className="bg-s-red" />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-base font-medium leading-[18px]">{r.target}</div>
                      <div className="truncate text-sm leading-4 text-fg-muted">{REQUEST_KIND_LABEL[r.kind]}, Frist {formatDate(r.deadline)}</div>
                    </div>
                    <Button variant="ghost" onClick={() => openLetter({ args: { kind: "Reminder", requestId: r.id }, target: r.target, domain: r.domain ?? undefined, parentId: r.id })}>
                      Nachfassen <ArrowRight />
                    </Button>
                  </div>
                ))}
              </Panel>
            </section>
          )}
        </div>
      </div>

      {sc.has_data && sc.parts.length > 0 && (
        <section>
          <SectionLabel>So setzt sich der Score zusammen</SectionLabel>
          <div className="mt-2 grid grid-cols-2 gap-x-10 gap-y-3 max-md:grid-cols-1">
            {sc.parts.map(([label, got, max]) => (
              <div key={label} className="flex items-center gap-3">
                <span className="w-[170px] shrink-0 truncate text-base text-fg-muted">{de(label)}</span>
                <Progress value={max ? got / max : 1} className="flex-1" />
                <span className="tnum w-[60px] whitespace-nowrap text-right text-sm text-fg-faint">{got} von {max}</span>
              </div>
            ))}
          </div>
        </section>
      )}
    </PageShell>
  );
}
