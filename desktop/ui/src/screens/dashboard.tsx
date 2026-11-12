import { AlarmClock, ArrowRight, Fingerprint, MailX, Plus, Sparkles, UserSearch } from "lucide-react";
import { useStore } from "@/lib/store";
import { navigate, href } from "@/lib/router";
import { deadlines, stats } from "@/lib/derive";
import { cn, deadlineLabel, relativeDays } from "@/lib/utils";
import { sourceGroup } from "@/lib/types";
import { Dot, EmptyState, PageHeader, SectionLabel, ServiceLogo } from "@/components/common";
import { Button } from "@/components/ui/button";

function Stat({ label, value, hint, hintClass }: { label: string; value: number; hint?: string; hintClass?: string }) {
  return (
    <div className="rounded-lg border border-border bg-bg px-4 py-3.5">
      <div className="text-sm text-fg-muted">{label}</div>
      <div className="tnum mt-1 text-2xl font-semibold tracking-[-0.02em]">{value}</div>
      <div className={cn("mt-0.5 h-4 text-sm text-fg-faint", hintClass)}>{hint}</div>
    </div>
  );
}

export function Dashboard() {
  const snap = useStore((s) => s.snap)!;
  const setFilter = useStore((s) => s.setAccountsFilter);
  const st = stats(snap);
  const dl = deadlines(snap);
  const recent = [...snap.accounts]
    .filter((a) => a.status === "Discovered")
    .sort((a, b) => (b.created_at > a.created_at ? 1 : -1))
    .slice(0, 6);
  const newest = [...snap.accounts].sort((a, b) => ((b.last_seen ?? "") > (a.last_seen ?? "") ? 1 : -1)).filter((a) => a.status === "Discovered").slice(0, 6);
  const list = newest.length ? newest : recent;

  const resolved = st.deleted + st.kept;
  const pct = st.total ? Math.round((resolved / st.total) * 100) : 0;
  const seg = (n: number) => (st.total ? (n / st.total) * 100 : 0);

  const relay = snap.accounts.filter((a) => a.identifiers.some((i) => i.endsWith("@privaterelay.appleid.com")) && a.status !== "Deleted").length;
  const stillReceiving = snap.subscriptions.filter((s) => s.status === "StillReceiving").length;

  const next: { icon: typeof Sparkles; title: string; text: string; cta: string; go: () => void }[] = [];
  if (st.overdue)
    next.push({ icon: AlarmClock, title: `${st.overdue} überfällige ${st.overdue === 1 ? "Frist" : "Fristen"}`, text: "Die Antwort des Anbieters steht aus. Nachfassen oder Beschwerde bei der Aufsichtsbehörde.", cta: "Ansehen", go: () => navigate({ page: "deletions" }) });
  if (st.discovered)
    next.push({ icon: UserSearch, title: `${st.discovered} neue Accounts prüfen`, text: "Bestätige, behalte oder lösche die automatisch gefundenen Accounts.", cta: "Prüfen", go: () => { setFilter("Discovered"); navigate({ page: "accounts" }); } });
  if (stillReceiving)
    next.push({ icon: MailX, title: `${stillReceiving} Newsletter kommen trotz Abmeldung`, text: "Diese Absender ignorieren deine Abmeldung. Ein DSGVO-Widerspruch hilft meist.", cta: "Newsletter", go: () => navigate({ page: "newsletter" }) });
  if (relay)
    next.push({ icon: Fingerprint, title: `${relay} Accounts nutzen Apple-Relay-Adressen`, text: "Erst Accounts löschen, danach den Zugriff bei Apple widerrufen.", cta: "Anzeigen", go: () => navigate({ page: "accounts" }) });

  return (
    <div className="mx-auto flex w-full max-w-[1120px] flex-col gap-7 px-7 py-7">
      <PageHeader
        title="Übersicht"
        description="Dein digitaler Fußabdruck, lokal und verschlüsselt."
        actions={<Button variant="secondary" onClick={() => navigate({ page: "sources" })}><Plus /> Quelle hinzufügen</Button>}
      />

      <div className="grid grid-cols-4 gap-3 max-lg:grid-cols-2 max-[620px]:grid-cols-1">
        <Stat label="Accounts gefunden" value={st.total} hint={`${st.sso} über SSO verknüpft`} />
        <Stat label="Zu prüfen" value={st.discovered} hint={st.discovered ? "Neu entdeckt" : undefined} />
        <Stat label="Löschung läuft" value={st.pending} hint={st.overdue ? `${st.overdue} überfällig` : "Keine überfälligen Fristen"} hintClass={st.overdue ? "text-s-red" : undefined} />
        <Stat label="Newsletter aktiv" value={st.subsActive} hint={`von ${snap.subscriptions.length} Absendern`} />
      </div>

      <section>
        <div className="rounded-lg border border-border px-4 py-3.5">
          <div className="flex items-baseline justify-between">
            <h2 className="text-base font-medium">Aufgeräumt</h2>
            <span className="tnum text-base text-fg-muted"><span className="font-medium text-fg">{pct}%</span> · {resolved} von {st.total}</span>
          </div>
          <div className="mt-3 flex h-1 gap-[2px] overflow-hidden rounded-full bg-muted">
            <div className="bg-s-green" style={{ width: `${seg(st.kept)}%` }} />
            <div className="bg-s-gray" style={{ width: `${seg(st.deleted)}%` }} />
            <div className="bg-s-yellow" style={{ width: `${seg(st.pending)}%` }} />
          </div>
          <div className="mt-3 flex flex-wrap gap-x-5 gap-y-1 text-sm text-fg-muted">
            <span className="flex items-center gap-1.5"><Dot className="bg-s-green" /> Behalten <span className="tnum text-fg-faint">{st.kept}</span></span>
            <span className="flex items-center gap-1.5"><Dot className="bg-s-gray" /> Gelöscht <span className="tnum text-fg-faint">{st.deleted}</span></span>
            <span className="flex items-center gap-1.5"><Dot className="bg-s-yellow" /> Löschung läuft <span className="tnum text-fg-faint">{st.pending}</span></span>
            <span className="flex items-center gap-1.5"><Dot className="bg-fg-faint/40" /> Offen <span className="tnum text-fg-faint">{st.open}</span></span>
          </div>
        </div>
      </section>

      <div className="grid grid-cols-2 gap-7 max-lg:grid-cols-1">
        <section>
          <SectionLabel right={<a href={href({ page: "deletions" })} className="text-sm text-fg-faint hover:text-fg">Alle</a>}>Fristen</SectionLabel>
          <div className="mt-1 overflow-hidden rounded-lg border border-border">
            {dl.length === 0 ? (
              <EmptyState icon={AlarmClock} title="Keine offenen Fristen" description="Sobald du eine Löschung startest, erscheint hier die Frist." className="py-10" />
            ) : (
              dl.slice(0, 6).map((d) => (
                <a
                  key={d.account.id}
                  href={href({ page: "accounts", id: d.account.id })}
                  className="flex h-11 items-center gap-3 border-b border-border px-3 transition-colors last:border-b-0 hover:bg-muted"
                >
                  <ServiceLogo name={d.account.name} domain={d.account.domain} />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-base font-medium">{d.account.name}</div>
                  </div>
                  <span className="text-sm text-fg-faint">{d.kind === "gdpr" ? "DSGVO-Antrag" : "Löschseite"}</span>
                  <span className={cn("flex w-[150px] items-center justify-end gap-2 whitespace-nowrap text-base", d.days < 0 ? "text-fg" : "text-fg-muted")}>
                    <Dot className={d.days < 0 ? "bg-s-red" : d.days <= 7 ? "bg-s-yellow" : "bg-s-gray"} />
                    <span className="tnum">{deadlineLabel(d.at)}</span>
                  </span>
                </a>
              ))
            )}
          </div>
        </section>

        <section>
          <SectionLabel right={<a href={href({ page: "accounts" })} className="text-sm text-fg-faint hover:text-fg">Alle</a>}>Zuletzt gefunden</SectionLabel>
          <div className="mt-1 overflow-hidden rounded-lg border border-border">
            {list.map((a) => (
              <a
                key={a.id}
                href={href({ page: "accounts", id: a.id })}
                className="flex h-11 items-center gap-3 border-b border-border px-3 transition-colors last:border-b-0 hover:bg-muted"
              >
                <ServiceLogo name={a.name} domain={a.domain} />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-base font-medium">{a.name}</div>
                </div>
                <span className="max-w-[180px] truncate text-sm text-fg-faint">{sourceGroup(a.evidence[0]?.source ?? "Manual")}</span>
                <span className="tnum w-[84px] text-right text-sm text-fg-muted">{relativeDays(a.last_seen)}</span>
              </a>
            ))}
          </div>
        </section>
      </div>

      <section>
        <SectionLabel>Als Nächstes</SectionLabel>
        <div className="mt-1 overflow-hidden rounded-lg border border-border">
          {next.length === 0 ? (
            <EmptyState icon={Sparkles} title="Alles erledigt" description="Im Moment gibt es nichts zu tun." className="py-10" />
          ) : (
            next.map((n) => (
              <div key={n.title} className="flex items-center gap-3 border-b border-border px-4 py-3 last:border-b-0">
                <n.icon className="size-4 shrink-0 text-fg-muted" strokeWidth={1.75} />
                <div className="min-w-0 flex-1">
                  <div className="text-base font-medium">{n.title}</div>
                  <div className="truncate text-sm text-fg-muted">{n.text}</div>
                </div>
                <Button variant="ghost" onClick={n.go}>{n.cta}<ArrowRight /></Button>
              </div>
            ))
          )}
        </div>
      </section>
    </div>
  );
}
