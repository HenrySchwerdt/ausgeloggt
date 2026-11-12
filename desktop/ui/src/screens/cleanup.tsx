import { useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { ArrowLeft, Check, ExternalLink, FileText, Search, Sparkles } from "lucide-react";
import { api } from "@/lib/api";
import { useStore } from "@/lib/store";
import { navigate } from "@/lib/router";
import { openUrl } from "@/lib/platform";
import { isTyping, useAsync } from "@/lib/hooks";
import { DIFFICULTY_LABEL, RISK_REASON_LABEL, type Account, type BillingSummary, type RiskAssessment } from "@/lib/types";
import { de, errorMessage, money } from "@/lib/utils";
import { Dot, EmptyState, ErrorState, PageHeader, riskDot, ServiceLogo } from "@/components/common";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import { Segmented } from "@/components/ui/segmented";
import { RowsSkeleton } from "@/components/ui/skeleton";
import { VirtualList } from "@/components/virtual-list";
import { openLetter } from "@/components/letter-dialog";

const OPEN = new Set(["Discovered", "Confirmed"]);

function useQueue() {
  const snap = useStore((s) => s.snap)!;
  const ins = useStore((s) => s.insights);
  const risk = useMemo(() => new Map((ins?.risks ?? []).map((r) => [r.account_id, r])), [ins]);
  const bill = useMemo(() => new Map((ins?.billing ?? []).map((b) => [b.account_id, b])), [ins]);
  const rows = useMemo(
    () => snap.accounts.filter((a) => OPEN.has(a.status)).sort((x, y) => (risk.get(y.id)?.score ?? 0) - (risk.get(x.id)?.score ?? 0) || x.name.localeCompare(y.name, "de")),
    [snap.accounts, risk],
  );
  return { snap, ins, risk, bill, rows };
}

function startErasure(a: Account) {
  openLetter({ args: { kind: "Erasure", accountId: a.id }, target: a.name, domain: a.domain.startsWith("unresolved:") ? undefined : a.domain });
}
function startCancel(a: Account) {
  openLetter({ args: { kind: "Cancellation", accountId: a.id }, target: a.name, domain: a.domain.startsWith("unresolved:") ? undefined : a.domain });
}

export function CleanupPage({ tidy }: { tidy: boolean }) {
  const q = useQueue();
  if (!q.ins) return <div className="p-7"><PageHeader title="Löschen und Kündigen" /><RowsSkeleton className="mt-6 -mx-7" /></div>;
  return tidy ? <Tidy {...q} /> : <CleanupList {...q} />;
}

type Q = ReturnType<typeof useQueue>;

function CleanupList({ rows, risk, bill }: Q) {
  const setStatus = useStore((s) => s.setAccountStatus);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<"all" | "sub">("all");
  const [active, setActive] = useState(-1);
  const [opened, setOpened] = useState<Set<string>>(new Set());
  const [later, setLater] = useState<Set<string>>(new Set());
  const catalogs = useAsync(async () => {
    const out = new Map<string, Awaited<ReturnType<typeof api.data.catalogInfo>>>();
    // Load lazily for the first rows only; the rest resolves on click.
    await Promise.all(rows.slice(0, 40).map(async (a) => { try { out.set(a.id, await api.data.catalogInfo({ domain: a.domain, name: a.name })); } catch { /* row falls back to the letter */ } }));
    return out;
  }, [rows.length]);

  const list = rows
    .filter((a) => !later.has(a.id))
    .filter((a) => filter === "all" || bill.get(a.id)?.active_subscription)
    .filter((a) => { const s = query.trim().toLowerCase(); return !s || a.name.toLowerCase().includes(s) || a.domain.includes(s); });
  const subs = rows.filter((a) => bill.get(a.id)?.active_subscription).length;

  const del = async (a: Account) => {
    let info = catalogs.data?.get(a.id);
    if (!info) { try { info = await api.data.catalogInfo({ domain: a.domain, name: a.name }); } catch (e) { toast.error(errorMessage(e)); } }
    const url = info?.deletion?.url;
    if (!url) return startErasure(a);
    await openUrl(url);
    setOpened((s) => new Set(s).add(a.id));
  };

  return (
    <div className="flex h-full min-w-0 flex-col">
      <div className="flex flex-col gap-4 px-7 pb-3 pt-7">
        <PageHeader
          title="Löschen und Kündigen"
          description={`${rows.length} offene Accounts, nach Risiko sortiert`}
          actions={<Button variant="primary" onClick={() => navigate({ page: "cleanup", tidy: true })} disabled={rows.length === 0}><Sparkles /> Aufräumen</Button>}
        />
        <div className="flex items-center gap-2">
          <Segmented value={filter} onChange={setFilter} options={[{ value: "all", label: "Alle", count: rows.length }, { value: "sub", label: "Mit Abo", count: subs }]} />
          <div className="relative min-w-[140px] max-w-[280px] flex-1">
            <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-fg-faint" />
            <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Suchen" className="pl-8" />
          </div>
        </div>
      </div>
      <div className="flex h-8 shrink-0 items-center gap-3 border-y border-border px-7 text-sm text-fg-faint">
        <span className="w-6" /><span className="min-w-0 flex-1">Account</span>
        <span className="hidden w-[110px] @[700px]:block">Aufwand</span><span className="w-[250px] text-right">Aktion</span>
      </div>
      {list.length === 0 ? (
        <EmptyState
          icon={Check} className="flex-1"
          title={rows.length === 0 ? "Nichts mehr offen" : "Nichts gefunden"}
          description={rows.length === 0 ? "Neu entdeckte Accounts erscheinen hier, sobald du eine Quelle scannst." : undefined}
          action={rows.length === 0 ? <Button onClick={() => navigate({ page: "sources" })}>Zu den Quellen</Button> : undefined}
        />
      ) : (
        <VirtualList
          items={list} rowHeight={52} getKey={(a) => a.id} active={active} onActive={setActive} onOpen={(a) => navigate({ page: "accounts", id: a.id })}
          renderRow={(a, { active: act }) => {
            const r = risk.get(a.id);
            const b = bill.get(a.id);
            const info = catalogs.data?.get(a.id);
            const diff = info?.deletion ? DIFFICULTY_LABEL[info.deletion.difficulty] : null;
            const isOpened = opened.has(a.id);
            return (
              <div className={`group flex h-full items-center gap-3 border-b border-border px-7 transition-colors ${act ? "bg-muted/70" : "hover:bg-muted/60"}`}>
                <ServiceLogo name={a.name} domain={a.domain} />
                <button className="min-w-0 flex-1 text-left" onClick={() => navigate({ page: "accounts", id: a.id })}>
                  <div className="flex items-center gap-2">
                    {r && <Dot className={riskDot(r.score)} />}
                    <span className="truncate text-base font-medium leading-[18px]" title={a.name}>{a.name}</span>
                  </div>
                  <div className="truncate text-sm leading-4 text-fg-faint">
                    {b?.active_subscription ? `Aktives Abo, ${money(b.amount_minor, b.currency)}. Erst kündigen, dann löschen.` : r ? de(r.next_step_de) : ""}
                  </div>
                </button>
                <span className="hidden w-[110px] shrink-0 items-center gap-2 text-base text-fg-muted @[700px]:flex">
                  {diff && <><Dot className={diff.dot} />{diff.label}</>}
                </span>
                <div className="flex w-[250px] shrink-0 justify-end gap-1.5">
                  {isOpened ? (
                    <>
                      <Button variant="ghost" onClick={() => setOpened((s) => { const n = new Set(s); n.delete(a.id); return n; })}>Zurück</Button>
                      <Button variant="primary" onClick={() => void setStatus([a.id], "Deleted")}><Check /> Erledigt</Button>
                    </>
                  ) : (
                    <>
                      {b?.active_subscription && <Button variant="secondary" onClick={() => startCancel(a)}>Kündigen</Button>}
                      <Button variant="secondary" onClick={() => void del(a)}>
                        {info && !info.deletion?.url ? <><FileText /> Löschantrag</> : <>Löschen <ExternalLink /></>}
                      </Button>
                      <Button variant="ghost" onClick={() => setLater((s) => new Set(s).add(a.id))} aria-label="Später">Später</Button>
                    </>
                  )}
                </div>
              </div>
            );
          }}
        />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------------------------
// Aufräumen: one account at a time
// ---------------------------------------------------------------------------------------------

function Tidy({ rows, risk, bill }: Q) {
  const setStatus = useStore((s) => s.setAccountStatus);
  const queue = useRef<Account[]>(rows).current;
  const [i, setI] = useState(0);
  const [opened, setOpened] = useState(false);
  const [done, setDone] = useState({ erledigt: 0, behalten: 0, spaeter: 0 });
  const a = queue[i];
  const r: RiskAssessment | undefined = a && risk.get(a.id);
  const b: BillingSummary | undefined = a && bill.get(a.id);
  const catalog = useAsync(() => a ? api.data.catalogInfo({ domain: a.domain, name: a.name }) : Promise.resolve(null), [a?.id]);
  const info = catalog.data?.deletion ?? null;
  const diff = info ? DIFFICULTY_LABEL[info.difficulty] : null;

  const next = (k: keyof typeof done) => { setDone((d) => ({ ...d, [k]: d[k] + 1 })); setI((n) => n + 1); setOpened(false); };
  const openLink = async () => {
    if (!a) return;
    if (info?.url) { await openUrl(info.url); setOpened(true); } else startErasure(a);
  };
  const finish = async () => { if (a) { await setStatus([a.id], "Deleted", true); next("erledigt"); } };
  const keep = async () => { if (a) { await setStatus([a.id], "Keep", true); next("behalten"); } };
  const skip = () => next("spaeter");

  const fns = useRef({ openLink, finish, keep, skip });
  fns.current = { openLink, finish, keep, skip };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isTyping(e.target) || e.ctrlKey || e.metaKey || e.altKey || document.querySelector("[role=dialog]")) return;
      const k = e.key.toLowerCase();
      if (k === "escape") navigate({ page: "cleanup" });
      else if (k === "o") void fns.current.openLink();
      else if (k === "e") void fns.current.finish();
      else if (k === "b") void fns.current.keep();
      else if (k === "l" || k === "arrowright") fns.current.skip();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const total = queue.length;
  const head = (
    <div className="flex items-center gap-3 px-7 pt-7">
      <Button variant="ghost" onClick={() => navigate({ page: "cleanup" })}><ArrowLeft /> Liste</Button>
      <div className="flex-1" />
      {total > 0 && <span className="tnum text-base text-fg-muted">{Math.min(i + 1, total)} von {total}</span>}
    </div>
  );

  if (!a) {
    return (
      <div className="flex h-full flex-col">
        {head}
        <EmptyState
          icon={Check} className="flex-1" title={total === 0 ? "Nichts aufzuräumen" : "Fertig"}
          description={total === 0 ? undefined : `${done.erledigt} erledigt, ${done.behalten} behalten, ${done.spaeter} auf später gelegt.`}
          action={<Button variant="primary" onClick={() => navigate({ page: "dashboard" })}>Zur Übersicht</Button>}
        />
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      {head}
      <div className="px-7 pt-4"><Progress value={total ? i / total : 0} /></div>
      <div className="flex flex-1 items-center justify-center overflow-y-auto px-7 py-8">
        <div key={a.id} className="anim-pop flex w-full max-w-[560px] flex-col gap-6">
          <div className="flex items-center gap-4">
            <ServiceLogo name={a.name} domain={a.domain} size={48} />
            <div className="min-w-0 flex-1">
              <h1 className="truncate text-xl font-semibold" title={a.name}>{a.name}</h1>
              <div className="truncate text-base text-fg-muted">{a.domain.startsWith("unresolved:") ? "Domain unbekannt" : a.domain}{a.identifiers[0] ? `, ${a.identifiers[0]}` : ""}</div>
            </div>
            {r && <span className="flex items-center gap-2 text-base text-fg-muted"><Dot className={riskDot(r.score)} /><span className="tnum">{r.score}</span></span>}
          </div>

          {r && r.reasons.length > 0 && (
            <ul className="flex flex-col gap-1 text-base text-fg-muted">
              {r.reasons.slice(0, 5).map((x) => <li key={x} className="flex items-center gap-2"><Dot className="bg-fg-faint" />{RISK_REASON_LABEL[x]}</li>)}
            </ul>
          )}
          {r && <p className="text-base">{de(r.next_step_de)}</p>}
          {b?.active_subscription && (
            <div className="flex items-center gap-3 rounded-md bg-muted px-3 py-2 text-base">
              <span className="flex-1">Aktives Abo, {money(b.amount_minor, b.currency)}. Erst kündigen.</span>
              <Button onClick={() => startCancel(a)}>Kündigung</Button>
            </div>
          )}
          {diff && <div className="flex items-center gap-2 text-base text-fg-muted"><Dot className={diff.dot} /> Aufwand: {diff.label}{info?.notes ? `. ${de(info.notes)}` : ""}</div>}

          <div className="flex flex-wrap items-center gap-2">
            <Button variant={opened ? "secondary" : "primary"} onClick={() => void openLink()}>
              {info?.url ? <>Löschseite öffnen <ExternalLink /></> : <><FileText /> Löschantrag</>}
              <span className="ml-1 text-xs opacity-50">O</span>
            </Button>
            <Button variant={opened ? "primary" : "secondary"} onClick={() => void finish()}><Check /> Erledigt <span className="ml-1 text-xs opacity-50">E</span></Button>
            <Button variant="secondary" onClick={() => void keep()}>Behalten <span className="ml-1 text-xs opacity-50">B</span></Button>
            <Button variant="ghost" onClick={skip}>Später <span className="ml-1 text-xs opacity-50">L</span></Button>
          </div>
          {catalog.error && <ErrorState message="Der Katalog ist gerade nicht erreichbar. Der Löschantrag geht trotzdem." className="py-2" />}
        </div>
      </div>
    </div>
  );
}
