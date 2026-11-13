import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { Check, ChevronDown, ExternalLink, FileText, Inbox, Loader2, Mail, Search, Sparkles, X } from "lucide-react";
import { api, runJob } from "@/lib/api";
import { useStore } from "@/lib/store";
import { navigate } from "@/lib/router";
import { openUrl } from "@/lib/platform";
import { findManualLinks, knownManualLinks, needsRescan, UNSUB_REASON_TEXT, type ManualLinks } from "@/lib/manual-unsub";
import {
  MAIL_CATEGORY_LABEL, SUBSCRIPTION_STATUS_LABEL, type MailCategory, type Subscription, type SubscriptionStatus,
  type UnsubscribeDonePayload,
} from "@/lib/types";
import { cn, errorMessage, formatDate, nf, plural, relativeDays } from "@/lib/utils";
import { Dot, EmptyState, PageHeader, ServiceLogo, SUB_STATUS_COLOR } from "@/components/common";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { DropdownMenu, DropdownMenuContent, DropdownMenuRadioGroup, DropdownMenuRadioItem, DropdownMenuTrigger, FilterMenu } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Segmented } from "@/components/ui/segmented";
import { Tooltip } from "@/components/ui/tooltip";
import { VirtualList } from "@/components/virtual-list";
import { confirm } from "@/components/confirm";
import { openLetter } from "@/components/letter-dialog";

type Filter = "open" | "StillReceiving" | "Unsubscribed" | "Keep" | "all";
const CATS: MailCategory[] = ["Marketing", "Newsletter", "Social", "Transactional", "Spam", "Unknown"];
const autoMethod = (s: Subscription) => s.category !== "Transactional" && s.unsubscribe.some((m) => !("Link" in m));
const linkOf = (s: Subscription) => s.unsubscribe.find((m): m is { Link: { url: string } } => "Link" in m)?.Link.url;
const canAct = (s: Subscription) => s.status === "Active" || s.status === "StillReceiving" || s.status === "Failed";

function CategoryMenu({ s, onPick }: { s: Subscription; onPick(c: MailCategory): void }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button onClick={(e) => e.stopPropagation()} className="flex h-6 items-center gap-1 rounded-md px-1.5 text-base text-fg-muted transition-colors hover:bg-muted-hover hover:text-fg data-[state=open]:bg-muted-hover">
          {MAIL_CATEGORY_LABEL[s.category]}
          {s.category_source === "Ai" && <span className="text-xs text-fg-faint">KI</span>}
          <ChevronDown className="size-3 text-fg-faint" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent onClick={(e) => e.stopPropagation()}>
        <DropdownMenuRadioGroup value={s.category} onValueChange={(v) => onPick(v as MailCategory)}>
          {CATS.map((c) => <DropdownMenuRadioItem key={c} value={c}>{MAIL_CATEGORY_LABEL[c]}</DropdownMenuRadioItem>)}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export function NewsletterPage() {
  const snap = useStore((s) => s.snap)!;
  const setStatus = useStore((s) => s.setSubscriptionStatus);
  const aiOn = snap.settings.ai.provider !== "Off";
  const [filter, setFilter] = useState<Filter>("open");
  const [mailbox, setMailbox] = useState("all");
  const [category, setCategory] = useState("all");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [active, setActive] = useState(-1);
  const [openId, setOpenId] = useState<string | null>(null);
  const [busy, setBusy] = useState<null | "unsub" | "ai">(null);
  const [result, setResult] = useState<UnsubscribeDonePayload | null>(null);

  const subs = snap.subscriptions;
  const counts = useMemo(() => {
    const c = { open: 0, StillReceiving: 0, Unsubscribed: 0, Keep: 0, all: subs.length };
    for (const s of subs) {
      if (s.status === "Active" || s.status === "Failed" || s.status === "StillReceiving") c.open++;
      if (s.status === "StillReceiving") c.StillReceiving++;
      if (s.status === "Unsubscribed") c.Unsubscribed++;
      if (s.status === "Keep") c.Keep++;
    }
    return c;
  }, [subs]);

  const rows = useMemo(() => {
    const q = query.trim().toLowerCase();
    return subs
      .filter((s) => filter === "all" ? true : filter === "open" ? canAct(s) : s.status === filter)
      .filter((s) => mailbox === "all" || s.mailbox === mailbox)
      .filter((s) => category === "all" || s.category === category)
      .filter((s) => !q || s.name.toLowerCase().includes(q) || s.domain.includes(q) || s.sender.toLowerCase().includes(q))
      .sort((a, b) => b.frequency_per_month - a.frequency_per_month || a.name.localeCompare(b.name, "de"));
  }, [subs, filter, mailbox, category, query]);
  const open = subs.find((s) => s.id === openId) ?? null;

  const toggle = (id: string) => setSelected((s) => { const n = new Set(s); if (n.has(id)) n.delete(id); else n.add(id); return n; });
  const allSel = rows.length > 0 && rows.every((r) => selected.has(r.id));

  async function unsubscribe(ids: string[]) {
    if (!ids.length) return;
    setBusy("unsub"); setResult(null);
    try {
      const d = await runJob<UnsubscribeDonePayload>(() => api.newsletters.unsubscribeStart(ids));
      if (d.payload) {
        setResult(d.payload);
        useStore.getState().setUnsubReasons(Object.fromEntries(d.payload.results.map((r) => [r.id, r.ok ? null : r.reason ?? null])));
      }
      setSelected(new Set());
    } catch (e) { toast.error(errorMessage(e)); } finally { setBusy(null); }
  }

  async function unsubscribeAllAds() {
    const ids = subs.filter((s) => (s.category === "Marketing" || s.category === "Spam") && (s.status === "Active" || s.status === "Failed") && autoMethod(s)).map((s) => s.id);
    const linkOnly = subs.filter((s) => (s.category === "Marketing" || s.category === "Spam") && canAct(s) && !autoMethod(s)).length;
    if (!ids.length) { toast("Keine Werbung mit automatischer Abmeldung offen"); return; }
    const ok = await confirm({
      title: `${plural(ids.length, "Absender", "Absender")} abmelden?`,
      text: <>Alle Werbung und Spam wird abgemeldet.{linkOnly ? ` ${plural(linkOnly, "Absender hat", "Absender haben")} nur einen Link und bleibt offen.` : ""} Wichtige Mails wie Rechnungen sind nicht betroffen.</>,
      confirm: "Alle abmelden",
    });
    if (ok) await unsubscribe(ids);
  }

  async function openLink(s: Subscription) {
    const url = linkOf(s);
    if (!url) return;
    const ok = await confirm({
      title: `${s.name}: Abmeldeseite öffnen?`,
      text: "Diese Abmeldung geht nur über eine Webseite. Sie öffnet im Browser. Bestätige dort nur die Abmeldung und gib keine weiteren Daten ein. Wenn die Seite mehr verlangt, brich ab.",
      confirm: "Seite öffnen",
    });
    if (!ok) return;
    await openUrl(url);
    toast(`${s.name}: Hast du dich abgemeldet?`, { duration: 15000, action: { label: "Ja, abgemeldet", onClick: () => void setStatus([s.id], "Unsubscribed") } });
  }

  async function classify() {
    setBusy("ai");
    try {
      await runJob(() => api.newsletters.aiClassifyStart());
    } catch (e) { toast.error(errorMessage(e)); } finally { setBusy(null); }
  }

  const pickCat = (ids: string[], c: MailCategory) => void api.newsletters.setCategory(ids, c).catch((e) => toast.error(errorMessage(e)));
  const mailboxOptions = [{ value: "all", label: "Alle" }, ...snap.mailboxes.map((m) => ({ value: m.address, label: m.address }))];
  const catOptions = [{ value: "all", label: "Alle" }, ...CATS.map((c) => ({ value: c, label: MAIL_CATEGORY_LABEL[c] }))];

  const actionFor = (s: Subscription) =>
    autoMethod(s) && canAct(s) && s.status !== "Failed"
      ? <Button variant="secondary" disabled={busy !== null} onClick={(e) => { e.stopPropagation(); void unsubscribe([s.id]); }}>Abmelden</Button>
      : linkOf(s) && canAct(s)
        ? <Button variant="secondary" onClick={(e) => { e.stopPropagation(); void openLink(s); }}>Seite öffnen <ExternalLink /></Button>
        : canAct(s)
          ? <Button variant="secondary" onClick={(e) => { e.stopPropagation(); setOpenId(s.id); }}>Selbst abmelden</Button>
          : null;

  return (
    <div className="relative flex h-full min-w-0">
      <div className="relative flex min-w-0 flex-1 flex-col @container">
        <div className="flex flex-col gap-4 px-7 pb-3 pt-7">
          <PageHeader
            title="Newsletter"
            description={subs.length ? `${nf(counts.open)} Absender aktiv${counts.StillReceiving ? `, ${counts.StillReceiving} kommen trotz Abmeldung` : ""}` : undefined}
            actions={subs.length > 0 && <>
              {aiOn && <Button disabled={busy !== null} onClick={() => void classify()}>{busy === "ai" ? <Loader2 className="animate-spin" /> : <Sparkles />} Mit KI einordnen</Button>}
              <Button variant="primary" disabled={busy !== null} onClick={() => void unsubscribeAllAds()}>{busy === "unsub" && <Loader2 className="animate-spin" />} Alle Werbung abmelden</Button>
            </>}
          />
          {subs.length > 0 && (
            <div className="flex min-w-0 flex-col gap-2.5">
              <div className="max-w-full overflow-x-auto [scrollbar-width:none]">
                <Segmented<Filter> value={filter} onChange={setFilter} options={[
                  { value: "open", label: "Aktiv", count: counts.open },
                  { value: "StillReceiving", label: "Kommt trotzdem", count: counts.StillReceiving },
                  { value: "Unsubscribed", label: "Abgemeldet", count: counts.Unsubscribed },
                  { value: "Keep", label: "Behalten", count: counts.Keep },
                  { value: "all", label: "Alle", count: counts.all },
                ]} />
              </div>
              <div className="flex items-center gap-2">
                <div className="relative min-w-[140px] max-w-[280px] flex-1">
                  <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-fg-faint" />
                  <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Absender suchen" className="pl-8" />
                </div>
                {snap.mailboxes.length > 1 && <FilterMenu label="Postfach" value={mailbox} options={mailboxOptions} onChange={setMailbox} />}
                <FilterMenu label="Art" value={category} options={catOptions} onChange={setCategory} />
              </div>
            </div>
          )}
          {result && <ResultStrip r={result} subs={subs} onClose={() => setResult(null)} />}
        </div>

        {subs.length === 0 ? (
          <EmptyState icon={Mail} title="Noch keine Newsletter" description="Beim Scan eines Postfachs erkennt ausgeloggt Absender und ihre Abmeldelinks." className="flex-1"
            action={<Button variant="primary" onClick={() => navigate({ page: "sources" })}>Postfach verbinden</Button>} />
        ) : (
          <>
            <div className="flex h-8 shrink-0 items-center gap-3 border-y border-border px-7 text-sm text-fg-faint">
              <Checkbox aria-label="Alle auswählen" checked={allSel} onCheckedChange={() => setSelected(allSel ? new Set() : new Set(rows.map((r) => r.id)))} />
              <span className="w-6" />
              <span className="min-w-0 flex-1">Absender</span>
              <span className="hidden w-[90px] text-right @[640px]:block">Pro Monat</span>
              <span className="hidden w-[130px] @[820px]:block">Art</span>
              <span className="w-[120px]">Status</span>
              <span className="w-[130px]" />
            </div>
            {rows.length === 0 ? (
              <EmptyState icon={Inbox} title="Nichts gefunden" description="Passe Filter oder Suche an." className="flex-1" />
            ) : (
              <VirtualList
                items={rows} rowHeight={52} getKey={(s) => s.id} active={active} onActive={setActive} bottomPad={80} onOpen={(s) => setOpenId(s.id)}
                renderRow={(s, { active: act, index }) => (
                  <div
                    role="row" onClick={() => { setActive(index); setOpenId(s.id); }}
                    className={cn("group flex h-full cursor-default items-center gap-3 border-b border-border px-7 transition-colors", s.id === openId || selected.has(s.id) ? "bg-muted" : act ? "bg-muted/70" : "hover:bg-muted/60")}
                  >
                    <Checkbox aria-label={`${s.name} auswählen`} checked={selected.has(s.id)} onClick={(e) => e.stopPropagation()} onCheckedChange={() => toggle(s.id)}
                      className={cn(!selected.has(s.id) && selected.size === 0 && "opacity-0 group-hover:opacity-100")} />
                    <ServiceLogo name={s.name} domain={s.domain} />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-base font-medium leading-[18px]" title={s.name}>{s.name}</div>
                      <div className="truncate text-sm leading-4 text-fg-faint">{s.sender}</div>
                    </div>
                    <span className="tnum hidden w-[90px] text-right text-base text-fg-muted @[640px]:block">{s.frequency_per_month < 1 ? "selten" : Math.round(s.frequency_per_month)}</span>
                    <span className="hidden w-[130px] @[820px]:block"><CategoryMenu s={s} onPick={(c) => pickCat([s.id], c)} /></span>
                    <span className="flex w-[120px] items-center gap-2 text-base text-fg-muted"><Dot className={SUB_STATUS_COLOR[s.status]} /><span className="truncate">{SUBSCRIPTION_STATUS_LABEL[s.status]}</span></span>
                    <span className="flex w-[130px] justify-end">{actionFor(s)}</span>
                  </div>
                )}
              />
            )}
          </>
        )}

        {selected.size > 0 && (
          <div className="anim-slide-up absolute bottom-5 left-1/2 z-20 flex items-center gap-1 rounded-lg border border-border bg-surface p-1.5 shadow-popover">
            <span className="tnum whitespace-nowrap px-2.5 text-base">{selected.size} ausgewählt</span>
            <span className="mx-1 h-4 w-px bg-border" />
            <Button variant="ghost" disabled={busy !== null} onClick={() => void unsubscribe([...selected])}>Abmelden</Button>
            <Button variant="ghost" onClick={() => { void setStatus([...selected], "Keep"); setSelected(new Set()); }}><Check /> Behalten</Button>
            <Button variant="ghost" onClick={() => { pickCat([...selected], "Marketing"); setSelected(new Set()); }}>Als Werbung</Button>
            <Tooltip content="Auswahl aufheben"><Button variant="ghost" size="icon" aria-label="Auswahl aufheben" onClick={() => setSelected(new Set())}><X /></Button></Tooltip>
          </div>
        )}
      </div>

      {open && (
        <aside className="anim-slide-right h-full w-[420px] shrink-0 border-l border-border bg-bg max-[1100px]:absolute max-[1100px]:inset-y-0 max-[1100px]:right-0 max-[1100px]:z-30 max-[1100px]:max-w-full max-[1100px]:shadow-popover">
          <SubDetail s={open} onClose={() => setOpenId(null)} busy={busy !== null} onUnsub={() => void unsubscribe([open.id])} onCat={(c) => pickCat([open.id], c)} />
        </aside>
      )}
    </div>
  );
}

function ResultStrip({ r, subs, onClose }: { r: UnsubscribeDonePayload; subs: Subscription[]; onClose(): void }) {
  const name = (id: string) => subs.find((s) => s.id === id)?.name ?? id;
  const bad = r.results.filter((x) => !x.ok && !x.skipped);
  const skipped = r.results.filter((x) => x.skipped);
  return (
    <div className="rounded-md bg-muted px-3 py-2.5 text-base">
      <div className="flex items-center gap-2">
        <span className="flex-1">{r.ok} abgemeldet{r.failed ? `, ${r.failed} fehlgeschlagen` : ""}{r.skipped ? `, ${r.skipped} nur per Link` : ""}</span>
        <Button variant="ghost" size="icon-sm" aria-label="Schließen" onClick={onClose}><X /></Button>
      </div>
      {(bad.length > 0 || skipped.length > 0) && (
        <ul className="mt-1.5 flex max-h-28 flex-col gap-0.5 overflow-y-auto text-sm text-fg-muted">
          {bad.map((x) => <li key={x.id}><Dot className="mr-2 bg-s-red" />{name(x.id)}: {x.message}</li>)}
          {skipped.slice(0, 8).map((x) => <li key={x.id}><Dot className="mr-2 bg-s-gray" />{name(x.id)}: Seite selbst öffnen</li>)}
        </ul>
      )}
    </div>
  );
}

function SubDetail({ s, onClose, busy, onUnsub, onCat }: {
  s: Subscription; onClose(): void; busy: boolean; onUnsub(): void; onCat(c: MailCategory): void;
}) {
  const setStatus = useStore((x) => x.setSubscriptionStatus);
  const proofs = s.post_unsubscribe_mails;
  const methods = s.unsubscribe.map((m) => "OneClick" in m ? "Ein Klick" : "Mailto" in m ? "Mail" : "Link");
  const st = (status: SubscriptionStatus) => void setStatus([s.id], status);
  return (
    <div className="selectable flex h-full flex-col overflow-y-auto">
      <div className="flex items-start gap-3 px-5 pb-4 pt-5">
        <ServiceLogo name={s.name} domain={s.domain} size={36} />
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-xl font-semibold leading-6" title={s.name}>{s.name}</h2>
          <div className="mt-0.5 truncate text-base text-fg-muted">{s.sender}</div>
        </div>
        <Button variant="ghost" size="icon-sm" aria-label="Schließen" onClick={onClose}><X /></Button>
      </div>
      <div className="flex flex-wrap items-center gap-2 px-5 pb-4">
        <span className="flex items-center gap-2 text-base text-fg-muted"><Dot className={SUB_STATUS_COLOR[s.status]} />{SUBSCRIPTION_STATUS_LABEL[s.status]}</span>
        <span className="flex-1" />
        {canAct(s) && autoMethod(s) && s.status !== "Failed" && <Button variant="primary" disabled={busy} onClick={onUnsub}>Abmelden</Button>}
        {s.status !== "Keep" && <Button onClick={() => st("Keep")}>Behalten</Button>}
        {s.status === "Keep" && <Button onClick={() => st("Active")}>Nicht mehr behalten</Button>}
        {s.status !== "Unsubscribed" && <Button variant="ghost" onClick={() => st("Unsubscribed")}>Bereits abgemeldet</Button>}
      </div>
      {canAct(s) && (s.status === "Failed" || !autoMethod(s)) && <ManualFallback s={s} failed={s.status === "Failed"} />}
      <dl className="grid grid-cols-3 gap-4 border-t border-border px-5 py-4">
        <div><dt className="text-sm text-fg-faint">Mails</dt><dd className="tnum text-base">{nf(s.message_count)}</dd></div>
        <div><dt className="text-sm text-fg-faint">Pro Monat</dt><dd className="tnum text-base">{s.frequency_per_month < 1 ? "selten" : Math.round(s.frequency_per_month)}</dd></div>
        <div><dt className="text-sm text-fg-faint">Zuletzt</dt><dd className="text-base">{relativeDays(s.last_seen)}</dd></div>
        <div className="col-span-3"><dt className="text-sm text-fg-faint">Postfach</dt><dd className="truncate text-base">{s.mailbox}</dd></div>
        <div className="col-span-3 flex items-center gap-2">
          <dt className="text-sm text-fg-faint">Art</dt><dd><CategoryMenu s={s} onPick={onCat} /></dd>
        </div>
      </dl>
      <section className="border-t border-border px-5 py-4">
        <h3 className="mb-2 text-sm font-medium text-fg-faint">Abmeldung</h3>
        <p className="text-base text-fg-muted">{methods.length ? `Möglich per ${[...new Set(methods)].join(", ")}.` : "Der Absender bietet keine Abmeldung an."}{s.category === "Transactional" ? " Wichtige Mails werden nie automatisch abgemeldet." : ""}</p>
        {s.history.length > 0 && (
          <ul className="mt-3 flex flex-col gap-2">
            {s.history.map((h, i) => (
              <li key={i} className="flex items-start gap-2.5 text-base">
                <Dot className={`mt-[7px] ${h.ok ? "bg-s-green" : "bg-s-red"}`} />
                <div className="min-w-0 flex-1"><div>{h.message}</div><div className="text-sm text-fg-faint">{formatDate(h.at)}</div></div>
              </li>
            ))}
          </ul>
        )}
      </section>
      {(s.status === "StillReceiving" || proofs.length > 0) && (
        <section className="border-t border-border px-5 py-4">
          <h3 className="mb-2 text-sm font-medium text-fg-faint">Kommt trotzdem</h3>
          <p className="mb-3 text-base text-fg-muted">Diese Mails kamen nach deiner Abmeldung. Das kannst du als Beleg verwenden.</p>
          <ul className="flex flex-col gap-2">
            {proofs.map((m, i) => (
              <li key={i} className="flex items-start gap-2.5 text-base">
                <Dot className="mt-[7px] bg-s-red" />
                <div className="min-w-0 flex-1"><div className="truncate" title={m.subject}>{m.subject}</div><div className="text-sm text-fg-faint">{formatDate(m.date)}, {m.from}</div></div>
              </li>
            ))}
          </ul>
          <Button className="mt-3" onClick={() => openLetter({ args: { kind: "CeaseAndDesist", subscriptionId: s.id }, target: s.name, domain: s.domain })}>
            <FileText /> Unterlassung
          </Button>
        </section>
      )}
      {s.sample_subjects.length > 0 && (
        <section className="border-t border-border px-5 py-4">
          <h3 className="mb-2 text-sm font-medium text-fg-faint">Beispiele</h3>
          <ul className="flex flex-col gap-1.5 text-base text-fg-muted">
            {s.sample_subjects.slice(0, 5).map((t, i) => <li key={i} className="truncate" title={t}>{t}</li>)}
          </ul>
        </section>
      )}
      <div className="h-6 shrink-0" />
    </div>
  );
}

/** Shown when the automatic unsubscribe failed or there is none: the user does it by hand. */
function ManualFallback({ s, failed }: { s: Subscription; failed: boolean }) {
  const setStatus = useStore((x) => x.setSubscriptionStatus);
  const [found, setFound] = useState<ManualLinks>(() => knownManualLinks(s));
  const [searching, setSearching] = useState(false);
  const [searched, setSearched] = useState(false);
  const [opened, setOpened] = useState(false);
  const [rescan, setRescan] = useState(false);
  const [scanning, setScanning] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const reason = useStore((x) => x.unsubReasons[s.id]);
  const mailbox = useStore((x) => x.snap?.mailboxes.find((m) => m.address === s.mailbox));
  useEffect(() => { setFound(knownManualLinks(s)); setSearched(false); setOpened(false); setRescan(false); setSearchError(null); }, [s.id]);

  const search = async () => {
    setSearching(true); setSearchError(null);
    try { setFound(await findManualLinks(s)); setSearched(true); setRescan(false); }
    catch (e) {
      if (needsRescan(e)) setRescan(true);
      else setSearchError(errorMessage(e));
    } finally { setSearching(false); }
  };
  const scanMailbox = async () => {
    if (!mailbox) { toast.error("Das Postfach dieser Liste ist nicht mehr eingerichtet."); return; }
    setScanning(true);
    try {
      const d = await runJob(() => api.mail.scanStart([mailbox.id], false));
      if (d.ok) { setRescan(false); await search(); }
    } catch (e) { toast.error(errorMessage(e)); } finally { setScanning(false); }
  };
  const open = async (url: string) => {
    const ok = await confirm({
      title: s.name + ": Abmeldeseite öffnen?",
      text: "Sie öffnet im Browser. Bestätige dort nur die Abmeldung und gib keine weiteren Daten ein. Wenn die Seite mehr verlangt, brich ab.",
      confirm: "Seite öffnen",
    });
    if (!ok) return;
    await openUrl(url);
    setOpened(true);
  };
  const host = (u: string) => { try { return new URL(u).host; } catch { return u; } };

  return (
    <section className="border-t border-border px-5 py-4">
      <h3 className="mb-2 text-sm font-medium text-fg-faint">Selbst abmelden</h3>
      <p className="text-base">{failed ? "Automatisch ging nicht." : "Hier gibt es keine automatische Abmeldung."}</p>
      {failed && reason && <p className="mt-1 text-base text-fg-muted">{UNSUB_REASON_TEXT[reason]}</p>}
      <p className="mt-1 text-base text-fg-muted">So meldest du dich selbst ab:</p>
      <ul className="mt-3 flex flex-col">
        {found.links.map((l) => (
          <li key={l.url} className="flex min-h-11 items-center gap-3 border-b border-border py-1.5 last:border-b-0">
            <div className="min-w-0 flex-1">
              <div className="truncate text-base">{l.label}</div>
              <div className="truncate text-sm text-fg-faint" title={l.url}>{host(l.url)}{l.source === "mail" ? ", aus der Mail" : ""}</div>
            </div>
            <Button variant="secondary" onClick={() => void open(l.url)}>Seite öffnen <ExternalLink /></Button>
          </li>
        ))}
        {found.mailto && (
          <li className="flex min-h-11 items-center gap-3 py-1.5">
            <div className="min-w-0 flex-1">
              <div className="truncate text-base">Per Mail abmelden</div>
              <div className="truncate text-sm text-fg-faint">{found.mailto}</div>
            </div>
            <Button variant="secondary" onClick={() => { void openUrl("mailto:" + found.mailto + "?subject=" + encodeURIComponent("Abmeldung")); setOpened(true); }}>Mail schreiben</Button>
          </li>
        )}
        {found.links.length === 0 && !found.mailto && <li className="text-base text-fg-muted">Der Absender nennt keine Abmeldung.{searched ? " Auch in der letzten Mail haben wir nichts gefunden." : ""}</li>}
      </ul>
      {rescan ? (
        <div className="anim-fade mt-3 flex flex-col items-start gap-2 rounded-md bg-muted px-3 py-2.5">
          <p className="text-base">Für die Suche brauchen wir die letzte Mail dieses Absenders. Dafür muss das Postfach noch einmal geprüft werden.</p>
          <Button variant="secondary" disabled={scanning} onClick={() => void scanMailbox()}>
            {scanning && <Loader2 className="animate-spin" />} Postfach neu scannen
          </Button>
        </div>
      ) : (
        <Button className="mt-3" variant="ghost" disabled={searching} onClick={() => void search()}>
          {searching && <Loader2 className="animate-spin" />} Link aus der letzten Mail suchen
        </Button>
      )}
      {searchError && <p className="mt-2 flex items-start gap-2 text-sm text-fg-muted"><Dot className="mt-1.5 bg-s-red" />{searchError}</p>}
      {opened && (
        <div className="anim-fade mt-4 flex flex-col gap-2.5 border-t border-border pt-4">
          <p className="text-base font-medium">Hast du dich abgemeldet?</p>
          <div className="flex flex-wrap gap-2">
            <Button variant="primary" onClick={() => void setStatus([s.id], "Unsubscribed")}><Check /> Erledigt, ich habe mich abgemeldet</Button>
            <Button variant="ghost" onClick={() => { setOpened(false); void setStatus([s.id], "Failed"); }}>Hat nicht geklappt</Button>
          </div>
        </div>
      )}
    </section>
  );
}
