import { useEffect, useMemo, useState } from "react";
import { Check, EyeOff, ListChecks, Search, ShieldCheck, UserX, Users, X } from "lucide-react";
import { useStore } from "@/lib/store";
import { navigate, useRoute } from "@/lib/router";
import { headline, identifierLabel, isHidden, isWeak, shownIdentifiers, tierWord, toReview } from "@/lib/derive";
import {
  ACCOUNT_STATUS_LABEL, sourceGroup, ssoKey, ssoLabel, type Account, type AccountStatus,
} from "@/lib/types";
import { cn, nf } from "@/lib/utils";
import { Dot, EmptyState, PageHeader, ServiceLogo, STATUS_COLOR } from "@/components/common";
import { AccountDetail } from "@/components/account-detail";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { FilterMenu } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Segmented } from "@/components/ui/segmented";
import { Tooltip } from "@/components/ui/tooltip";
import { VirtualList } from "@/components/virtual-list";

type Filter = AccountStatus | "all" | "hidden";

const PRIORITY: Record<AccountStatus, number> = {
  Discovered: 0, Confirmed: 1, DeletionPending: 2, Keep: 3, Deleted: 4, NotAnAccount: 5, Ignored: 6,
};

function matches(a: Account, f: Filter) {
  if (f === "all") return !isHidden(a);
  if (f === "hidden") return isHidden(a);
  return a.status === f;
}

export function AccountsPage() {
  const route = useRoute();
  const snap = useStore((s) => s.snap)!;
  const filter = useStore((s) => s.accountsFilter);
  const setFilter = useStore((s) => s.setAccountsFilter);
  const setStatus = useStore((s) => s.setAccountStatus);
  const [query, setQuery] = useState("");
  const [source, setSource] = useState("all");
  const [sso, setSso] = useState("all");
  const [showWeak, setShowWeak] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [active, setActive] = useState(-1);

  const openId = route.page === "accounts" ? route.id : undefined;
  const open = snap.accounts.find((a) => a.id === openId);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !document.querySelector("[role=dialog],[role=menu]")) {
        if (selected.size) setSelected(new Set());
        else if (openId) navigate({ page: "accounts" });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selected.size, openId]);

  // Weak finds (little evidence) stay out of the default list, with a toggle to show them.
  const weakCount = useMemo(() => snap.accounts.filter(isWeak).length, [snap.accounts]);
  const review = useMemo(() => toReview(snap.accounts).length, [snap.accounts]);

  const counts = useMemo(() => {
    const c: Record<string, number> = { all: 0, hidden: 0 };
    for (const a of snap.accounts) {
      if (isWeak(a) && !showWeak) continue;
      c[a.status] = (c[a.status] ?? 0) + 1;
      if (isHidden(a)) c.hidden++;
      else c.all++;
    }
    return c;
  }, [snap.accounts, showWeak]);

  const sourceOptions = useMemo(() => {
    const set = new Set<string>();
    snap.accounts.forEach((a) => a.evidence.forEach((e) => set.add(sourceGroup(e.source))));
    return [{ value: "all", label: "Alle" }, ...[...set].sort().map((s) => ({ value: s, label: s }))];
  }, [snap.accounts]);

  const ssoOptions = useMemo(() => {
    const m = new Map<string, string>();
    snap.accounts.forEach((a) => a.sso.forEach((p) => m.set(ssoKey(p), ssoLabel(p))));
    const hasRelay = snap.accounts.some((a) => a.identifiers.some((i) => i.endsWith("@privaterelay.appleid.com")));
    return [
      { value: "all", label: "Alle" },
      { value: "none", label: "Ohne Drittanbieter" },
      ...(hasRelay ? [{ value: "relay", label: "Apple Relay" }] : []),
      ...[...m.entries()].sort((a, b) => a[1].localeCompare(b[1])).map(([value, label]) => ({ value, label })),
    ];
  }, [snap.accounts]);

  const rows = useMemo(() => {
    const q = query.trim().toLowerCase();
    return snap.accounts
      .filter((a) => showWeak || !isWeak(a))
      .filter((a) => matches(a, filter))
      .filter((a) => source === "all" || a.evidence.some((e) => sourceGroup(e.source) === source))
      .filter((a) => {
        if (sso === "all") return true;
        if (sso === "none") return a.sso.length === 0;
        if (sso === "relay") return a.identifiers.some((i) => i.endsWith("@privaterelay.appleid.com"));
        return a.sso.some((p) => ssoKey(p) === sso);
      })
      .filter((a) => !q || a.name.toLowerCase().includes(q) || a.domain.includes(q) || a.identifiers.some((i) => i.toLowerCase().includes(q)))
      .sort((a, b) => PRIORITY[a.status] - PRIORITY[b.status] || b.confidence - a.confidence || a.name.localeCompare(b.name, "de"));
  }, [snap.accounts, filter, source, sso, query, showWeak]);

  useEffect(() => {
    if (new URLSearchParams(location.search).has("bulk")) setSelected(new Set(rows.slice(0, 3).map((r) => r.id)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const allSelected = rows.length > 0 && rows.every((r) => selected.has(r.id));
  const someSelected = selected.size > 0 && !allSelected;
  const toggle = (id: string) =>
    setSelected((s) => { const n = new Set(s); if (n.has(id)) n.delete(id); else n.add(id); return n; });

  const bulk = async (status: AccountStatus) => {
    const ids = [...selected];
    setSelected(new Set());
    await setStatus(ids, status);
  };

  const filtering = source !== "all" || sso !== "all" || query;

  return (
    <div className="relative flex h-full min-w-0">
      <div className="relative flex min-w-0 flex-1 flex-col @container">
        <div className="flex flex-col gap-4 px-7 pb-3 pt-7">
          <PageHeader
            title="Accounts"
            description={`${nf(counts.all)} Accounts${review ? `, ${nf(review)} zu prüfen` : ""}`}
            actions={review > 0 ? (
              <Tooltip content="Du siehst jeden Fund einzeln und sagst, ob du dort einen Account hast.">
                <Button variant="primary" onClick={() => navigate({ page: "review" })}><ListChecks /> Prüfen ({nf(review)})</Button>
              </Tooltip>
            ) : undefined}
          />
          <div className="flex min-w-0 flex-col gap-2.5">
            <div className="-mb-px max-w-full overflow-x-auto [scrollbar-width:none]">
              <Segmented<Filter>
                value={filter}
                onChange={setFilter}
                options={[
                  { value: "all", label: "Alle", count: counts.all },
                  { value: "Discovered", label: ACCOUNT_STATUS_LABEL.Discovered, count: counts.Discovered ?? 0 },
                  { value: "Confirmed", label: ACCOUNT_STATUS_LABEL.Confirmed, count: counts.Confirmed ?? 0 },
                  { value: "Keep", label: ACCOUNT_STATUS_LABEL.Keep, count: counts.Keep ?? 0 },
                  { value: "DeletionPending", label: ACCOUNT_STATUS_LABEL.DeletionPending, count: counts.DeletionPending ?? 0 },
                  { value: "Deleted", label: ACCOUNT_STATUS_LABEL.Deleted, count: counts.Deleted ?? 0 },
                  { value: "hidden", label: ACCOUNT_STATUS_LABEL.Ignored, count: counts.hidden },
                ]}
              />
            </div>
            <div className="flex items-center gap-2">
              <div className="relative min-w-[140px] max-w-[280px] flex-1">
                <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-fg-faint" />
                <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Name, Domain oder Adresse" className="pl-8" />
              </div>
              <FilterMenu label="Quelle" value={source} options={sourceOptions} onChange={setSource} />
              <FilterMenu label="Anmeldung" value={sso} options={ssoOptions} onChange={setSso} />
              {filtering && <Button variant="ghost" onClick={() => { setSource("all"); setSso("all"); setQuery(""); }}>Zurücksetzen</Button>}
              {weakCount > 0 && (
                <Tooltip content="Funde mit kaum Hinweisen, zum Beispiel nur Newsletter. Sie sind meist keine Accounts.">
                  <label className="ml-auto flex cursor-default items-center gap-2 text-base text-fg-muted">
                    <Checkbox checked={showWeak} onCheckedChange={(v) => setShowWeak(v === true)} />
                    Unsichere zeigen ({nf(weakCount)})
                  </label>
                </Tooltip>
              )}
            </div>
          </div>
        </div>

        {/* column header */}
        <div className="flex h-8 shrink-0 items-center gap-3 border-y border-border px-7 text-sm text-fg-faint">
          <Checkbox
            aria-label="Alle auswählen"
            checked={allSelected ? true : someSelected ? "indeterminate" : false}
            onCheckedChange={() => setSelected(allSelected ? new Set() : new Set(rows.map((r) => r.id)))}
          />
          <span className="w-6" />
          <span className="min-w-0 flex-[1] basis-0">Account</span>
          <span className="hidden min-w-0 flex-[1.2] basis-0 @[640px]:block">Warum wir es glauben</span>
          <span className="w-[116px]">Status</span>
          <span className="hidden w-[100px] text-right @[520px]:block">Einschätzung</span>
        </div>

        {rows.length === 0 ? (
          snap.accounts.length === 0
            ? <EmptyState icon={Users} title="Noch keine Accounts" description="Hier erscheinen die Accounts, die wir in deinem Postfach oder in einem Passwortexport finden." className="flex-1" action={<Button variant="primary" onClick={() => navigate({ page: "sources" })}>Quelle verbinden</Button>} />
            : filter === "Discovered" && !filtering
              ? <EmptyState icon={Check} title="Nichts zu prüfen" description={weakCount ? `Es gibt noch ${nf(weakCount)} unsichere Funde. Blende sie mit „Unsichere zeigen“ ein.` : "Neue Funde erscheinen nach der nächsten Prüfung."} className="flex-1" />
              : <EmptyState icon={Users} title="Keine Accounts gefunden" description="Passe Filter oder Suche an." className="flex-1" />
        ) : (
          <VirtualList
            items={rows} getKey={(a) => a.id} active={active} onActive={setActive} bottomPad={80}
            onOpen={(a) => navigate({ page: "accounts", id: a.id })}
            renderRow={(a, { active: act }) => {
              const isSel = selected.has(a.id);
              const isOpen = a.id === openId;
              const ids = shownIdentifiers(a, snap);
              return (
                <div
                  role="row" onClick={() => { setActive(rows.indexOf(a)); navigate({ page: "accounts", id: a.id }); }}
                  className={cn("group flex h-full cursor-default items-center gap-3 border-b border-border px-7 transition-colors", isOpen || isSel ? "bg-muted" : act ? "bg-muted/70" : "hover:bg-muted/60")}
                >
                  <Checkbox
                    aria-label={`${a.name} auswählen`} checked={isSel} onClick={(e) => e.stopPropagation()} onCheckedChange={() => toggle(a.id)}
                    className={cn(!isSel && selected.size === 0 && "opacity-0 group-hover:opacity-100")}
                  />
                  <ServiceLogo name={a.name} domain={a.domain} />
                  <div className="min-w-0 flex-[1] basis-0">
                    <div className="truncate text-base font-medium leading-[18px]" title={a.name}>{a.name}</div>
                    <div className="truncate text-sm leading-4 text-fg-faint">{a.domain.startsWith("unresolved:") ? "Domain unbekannt" : a.domain}</div>
                  </div>
                  <div className="hidden min-w-0 flex-[1.2] basis-0 @[640px]:block">
                    <div className="truncate text-base text-fg-muted" title={headline(a)}>{headline(a)}</div>
                    {ids.length > 0 && <div className="truncate text-sm leading-4 text-fg-faint">{identifierLabel(ids[0]!)}{ids.length > 1 ? ` und ${ids.length - 1} weitere` : ""}</div>}
                  </div>
                  <div className="flex w-[116px] shrink-0 items-center gap-2 text-base text-fg-muted">
                    <Dot className={STATUS_COLOR[a.status]} />
                    <span className="truncate">{ACCOUNT_STATUS_LABEL[a.status]}</span>
                  </div>
                  <div className={cn("hidden w-[100px] text-right text-base @[520px]:block", tierWord(a) === "Unsicher" ? "text-fg-faint" : "text-fg-muted")}>{tierWord(a)}</div>
                </div>
              );
            }}
          />
        )}

        {selected.size > 0 && (
          <div className="anim-slide-up absolute bottom-5 left-1/2 z-20 flex items-center gap-1 rounded-lg border border-border bg-surface p-1.5 shadow-popover">
            <span className="tnum whitespace-nowrap px-2.5 text-base text-fg">{selected.size} ausgewählt</span>
            <span className="mx-1 h-4 w-px bg-border" />
            <Tooltip content="Du hast dort einen Account. Er kommt in deine Liste zum Aufräumen."><Button variant="ghost" onClick={() => void bulk("Confirmed")}><Check /> Ja, hab ich</Button></Tooltip>
            <Tooltip content="Dort ist kein Account. Der Fund wird ausgeblendet."><Button variant="ghost" onClick={() => void bulk("NotAnAccount")}><UserX /> Kein Account</Button></Tooltip>
            <Tooltip content="Der Account bleibt bestehen und wird nicht mehr als offen gezählt."><Button variant="ghost" onClick={() => void bulk("Keep")}><ShieldCheck /> Behalten</Button></Tooltip>
            <Tooltip content="Der Eintrag verschwindet aus der Liste, ohne dass wir etwas dazu sagen."><Button variant="ghost" onClick={() => void bulk("Ignored")}><EyeOff /> Ausblenden</Button></Tooltip>
            <span className="mx-1 h-4 w-px bg-border" />
            <Button variant="ghost" size="icon" aria-label="Auswahl aufheben" onClick={() => setSelected(new Set())}><X /></Button>
          </div>
        )}
      </div>

      {open && (
        <aside className="anim-slide-right h-full w-[420px] shrink-0 border-l border-border bg-bg max-[1100px]:absolute max-[1100px]:inset-y-0 max-[1100px]:right-0 max-[1100px]:z-30 max-[1100px]:max-w-full max-[1100px]:shadow-popover">
          <AccountDetail account={open} />
        </aside>
      )}
    </div>
  );
}
