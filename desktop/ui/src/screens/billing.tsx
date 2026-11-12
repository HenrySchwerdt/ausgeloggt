import { useState } from "react";
import { CreditCard, ExternalLink, FileText } from "lucide-react";
import { useStore } from "@/lib/store";
import { href, navigate } from "@/lib/router";
import { openUrl } from "@/lib/platform";
import { INTERVAL_LABEL, paymentMethodKey, paymentMethodLabel, type Account, type BillingSummary, type PaymentMethod } from "@/lib/types";
import { daysFromNow, money, plural } from "@/lib/utils";
import { Dot, EmptyState, ErrorState, Figure, PageHeader, SectionLabel, ServiceLogo } from "@/components/common";
import { PageShell, Panel } from "@/components/page";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import { openLetter } from "@/components/letter-dialog";

function nextLabel(b: BillingSummary): string {
  if (!b.next_charge_estimate) return "";
  const d = daysFromNow(b.next_charge_estimate);
  return d < 0 ? "überfällig" : d === 0 ? "heute" : d === 1 ? "morgen" : `in ${d} Tagen`;
}

export function BillingPage() {
  const snap = useStore((s) => s.snap)!;
  const ins = useStore((s) => s.insights);
  const err = useStore((s) => s.insightsError);
  const refresh = useStore((s) => s.refresh);
  const reference = useStore((s) => s.reference);
  const ensure = useStore((s) => s.ensureReference);
  const [blocked, setBlocked] = useState<{ method: PaymentMethod; ids: string[] } | null>(null);
  if (!reference) void ensure();

  if (!ins) {
    return (
      <PageShell>
        <PageHeader title="Abos und Zahlungen" />
        {err ? <ErrorState message={err} onRetry={() => void refresh()} /> : <Skeleton className="h-40" />}
      </PageShell>
    );
  }

  const byId = new Map<string, Account>(snap.accounts.map((a) => [a.id, a]));
  const subs = ins.billing.filter((b) => b.active_subscription && byId.has(b.account_id))
    .sort((x, y) => (y.monthly_minor ?? 0) - (x.monthly_minor ?? 0));
  const hubFor = (via: string | null, methods: PaymentMethod[]) => {
    const key = via ?? (methods.includes("PayPal") ? "PayPal" : null);
    if (!key) return null;
    return reference?.payment_hubs.find((h) => h.title_de.toLowerCase().includes(key.split(" ")[0]!.toLowerCase())) ?? null;
  };

  if (ins.billing.length === 0) {
    return (
      <PageShell>
        <PageHeader title="Abos und Zahlungen" />
        <EmptyState icon={CreditCard} className="py-20" title="Keine Zahlungen gefunden"
          description="Rechnungen und Zahlungsbestätigungen in deinen Postfächern zeigen Abos und Zahlarten. Scanne ein Postfach, um sie zu finden."
          action={<Button variant="primary" onClick={() => navigate({ page: "sources" })}>Zu den Quellen</Button>} />
      </PageShell>
    );
  }

  return (
    <PageShell>
      <PageHeader title="Abos und Zahlungen" />

      <div className="grid grid-cols-4 gap-6 max-lg:grid-cols-2">
        {ins.totals.length === 0 ? <Figure label="Aktive Abos" value={0} /> : ins.totals.map(([c, m, y, n]) => (
          <div key={c} className="contents">
            <Figure label="Pro Monat" value={money(m, c)} />
            <Figure label="Pro Jahr" value={money(y, c)} />
            <Figure label="Aktive Abos" value={n} />
          </div>
        ))}
        <Figure label="Möglicherweise ungenutzt" value={subs.filter((b) => b.possibly_unused).length} hint={subs.some((b) => b.possibly_unused) ? money(subs.filter((b) => b.possibly_unused).reduce((t, b) => t + (b.monthly_minor ?? 0), 0), "EUR") + " im Monat" : ""} />
      </div>

      <section>
        <SectionLabel>Aktive Abos</SectionLabel>
        <Panel className="mt-1">
          {subs.length === 0 && <EmptyState icon={CreditCard} title="Keine aktiven Abos" className="py-10" />}
          {subs.map((b) => {
            const a = byId.get(b.account_id)!;
            const hub = hubFor(b.via, b.methods);
            const flags = [b.price_increased && "Preis erhöht", b.possibly_unused && "vermutlich ungenutzt"].filter(Boolean) as string[];
            return (
              <div key={b.account_id} className="flex min-h-[52px] items-center gap-3 border-b border-border px-3 py-1.5 last:border-b-0">
                <ServiceLogo name={a.name} domain={a.domain} />
                <a href={href({ page: "accounts", id: a.id })} className="min-w-0 flex-1">
                  <div className="truncate text-base font-medium leading-[18px]">{a.name}{b.plan ? <span className="font-normal text-fg-muted">, {b.plan}</span> : null}</div>
                  <div className="flex flex-wrap items-center gap-x-3 text-sm leading-4 text-fg-muted">
                    <span>{b.methods.map(paymentMethodLabel).join(", ")}{b.via ? `, über ${b.via}` : ""}</span>
                    {flags.map((f) => <span key={f} className="flex items-center gap-1.5"><Dot className="bg-s-yellow" />{f}</span>)}
                  </div>
                </a>
                <div className="hidden w-[150px] text-right @[700px]:block">
                  <div className="tnum text-base">{money(b.amount_minor, b.currency)}</div>
                  <div className="text-sm leading-4 text-fg-faint">{b.interval ? INTERVAL_LABEL[b.interval] : ""}</div>
                </div>
                <span className="tnum hidden w-[84px] text-right text-sm text-fg-muted sm:block">{nextLabel(b)}</span>
                {hub ? (
                  <Button variant="secondary" onClick={() => void openUrl(hub.url)}>Bei {b.via ?? "PayPal"} kündigen <ExternalLink /></Button>
                ) : (
                  <Button variant="secondary" onClick={() => openLetter({ args: { kind: "Cancellation", accountId: a.id }, target: a.name, domain: a.domain })}><FileText /> Kündigen</Button>
                )}
              </div>
            );
          })}
        </Panel>
      </section>

      <section>
        <SectionLabel>Gespeicherte Zahlarten</SectionLabel>
        <Panel className="mt-1">
          {ins.payment_index.length === 0 && <EmptyState icon={CreditCard} title="Keine Zahlarten gefunden" className="py-10" />}
          {ins.payment_index.map(([m, ids]) => (
            <div key={paymentMethodKey(m)} className="flex h-11 items-center gap-3 border-b border-border px-3 last:border-b-0">
              <CreditCard className="size-4 text-fg-muted" strokeWidth={1.75} />
              <span className="min-w-0 flex-1 truncate text-base"><span className="font-medium">{paymentMethodLabel(m)}</span> <span className="text-fg-muted">bei {plural(ids.length, "Dienst", "Diensten")}</span></span>
              {typeof m !== "string" && ("Card" in m || "Sepa" in m) && <Button variant="secondary" onClick={() => setBlocked({ method: m, ids })}>Karte gesperrt?</Button>}
            </div>
          ))}
        </Panel>
      </section>

      {reference && reference.payment_hubs.length > 0 && (
        <section>
          <SectionLabel>Abos bei Anbietern verwalten</SectionLabel>
          <div className="mt-1 grid grid-cols-2 gap-x-6 max-md:grid-cols-1">
            {reference.payment_hubs.map((h) => (
              <button key={h.url} onClick={() => void openUrl(h.url)} className="flex h-11 items-center gap-3 border-b border-border text-left text-base transition-colors hover:text-fg">
                <span className="min-w-0 flex-1"><span className="font-medium">{h.title_de}</span><span className="ml-2 text-fg-muted">{h.description_de}</span></span>
                <ExternalLink className="size-3.5 shrink-0 text-fg-faint" />
              </button>
            ))}
          </div>
        </section>
      )}

      <BlockedDialog blocked={blocked} accounts={byId} onClose={() => setBlocked(null)} />
    </PageShell>
  );
}

function BlockedDialog({ blocked, accounts, onClose }: { blocked: { method: PaymentMethod; ids: string[] } | null; accounts: Map<string, Account>; onClose(): void }) {
  const [done, setDone] = useState<Set<string>>(new Set());
  const list = (blocked?.ids ?? []).map((id) => accounts.get(id)).filter((a): a is Account => !!a);
  const toggle = (id: string) => setDone((s) => { const n = new Set(s); if (n.has(id)) n.delete(id); else n.add(id); return n; });
  return (
    <Dialog open={!!blocked} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-[520px]">
        <DialogHeader>
          <DialogTitle>{blocked ? paymentMethodLabel(blocked.method) : ""} ersetzen</DialogTitle>
          <DialogDescription>Hinterlege bei diesen {list.length} Diensten eine neue Zahlart. Hake ab, was erledigt ist.</DialogDescription>
        </DialogHeader>
        <ul className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
          {list.map((a) => (
            <li key={a.id} className="flex h-10 items-center gap-3 rounded-md px-3 hover:bg-muted">
              <Checkbox checked={done.has(a.id)} onCheckedChange={() => toggle(a.id)} aria-label={`${a.name} erledigt`} />
              <ServiceLogo name={a.name} domain={a.domain} size={20} className="!size-5" />
              <span className={`min-w-0 flex-1 truncate text-base ${done.has(a.id) ? "text-fg-faint line-through" : ""}`}>{a.name}</span>
              {!a.domain.startsWith("unresolved:") && <Button variant="ghost" size="sm" onClick={() => void openUrl(`https://${a.domain}`)}>Öffnen <ExternalLink /></Button>}
            </li>
          ))}
        </ul>
        <DialogFooter>
          <span className="tnum flex-1 text-base text-fg-muted">{done.size} von {list.length}</span>
          <Button variant="primary" onClick={onClose}>Fertig</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
