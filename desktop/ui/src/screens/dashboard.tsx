import { ArrowRight, CalendarClock, Inbox, Sparkles } from "lucide-react";
import { useStore } from "@/lib/store";
import { navigate, href } from "@/lib/router";
import { daysFromNow, de, money, formatDate, plural } from "@/lib/utils";
import { isOverdue, requestDays } from "@/lib/derive";
import { REQUEST_KIND_LABEL } from "@/lib/types";
import { Dot, EmptyState, ErrorState, Figure, PageHeader, riskDot, SectionLabel, ServiceLogo } from "@/components/common";
import { PageShell, Panel } from "@/components/page";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Progress } from "@/components/ui/progress";
import { openLetter } from "@/components/letter-dialog";

export function Dashboard() {
  const snap = useStore((s) => s.snap)!;
  const ins = useStore((s) => s.insights);
  const insErr = useStore((s) => s.insightsError);
  const refresh = useStore((s) => s.refresh);

  const byId = new Map(snap.accounts.map((a) => [a.id, a]));
  const noData = snap.accounts.length === 0 && snap.mailboxes.length === 0;

  if (noData) {
    return (
      <PageShell>
        <PageHeader title="Übersicht" />
        <EmptyState
          icon={Inbox} className="py-24"
          title="Noch keine Daten"
          description="Verbinde ein Postfach oder importiere einen Passwortexport. ausgeloggt findet damit deine Accounts."
          action={<div className="flex gap-2"><Button variant="primary" onClick={() => navigate({ page: "sources" })}>Quelle hinzufügen</Button></div>}
        />
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
  const risks = ins.risks.filter((r) => r.score > 0 && byId.get(r.account_id)?.status !== "DeletionPending").slice(0, 6);
  const renewals = ins.billing
    .filter((b) => b.active_subscription && b.next_charge_estimate)
    .sort((x, y) => (x.next_charge_estimate! < y.next_charge_estimate! ? -1 : 1))
    .slice(0, 5);
  const overdue = snap.requests.filter(isOverdue).sort((a, b) => (requestDays(a) ?? 0) - (requestDays(b) ?? 0));
  const monthly = ins.totals[0];

  return (
    <PageShell>
      <PageHeader
        title="Übersicht"
        actions={<Button variant="primary" onClick={() => navigate({ page: "cleanup", tidy: true })}><Sparkles /> Aufräumen</Button>}
      />

      <div className="grid grid-cols-4 gap-6 max-lg:grid-cols-2 max-[620px]:grid-cols-1">
        <Figure
          label="Datenschutz Score"
          value={sc.has_data ? <>{sc.score}<span className="ml-1 text-lg font-normal text-fg-faint">von 100</span></> : <span className="text-lg font-normal text-fg-muted">noch keine Daten</span>}
          hint={sc.has_data ? undefined : ""}
        />
        <Figure label="Offene Accounts" value={sc.open_accounts} hint={sc.deleted_accounts ? `${sc.deleted_accounts} gelöscht` : ""} />
        <Figure label="Hohes Risiko" value={sc.high_risk_accounts} hint={sc.high_risk_accounts ? "zuerst erledigen" : ""} />
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
              <EmptyState icon={Sparkles} title="Keine Risiken gefunden" className="py-10" />
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
                <EmptyState icon={CalendarClock} title="Keine Abos gefunden" className="py-10" />
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
