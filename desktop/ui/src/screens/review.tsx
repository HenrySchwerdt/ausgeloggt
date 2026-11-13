import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, Check, ListChecks, Undo2 } from "lucide-react";
import { queueChoice, useStore } from "@/lib/store";
import { navigate } from "@/lib/router";
import { isTyping } from "@/lib/hooks";
import { identifierLabel, isWeak, reasonsOf, reviewOrder, shownIdentifiers, tierWord, toReview } from "@/lib/derive";
import { plural } from "@/lib/utils";
import { EmptyState, SectionLabel, ServiceLogo } from "@/components/common";
import { Button } from "@/components/ui/button";
import { Kbd } from "@/components/ui/kbd";
import { Checkbox } from "@/components/ui/checkbox";
import { Progress } from "@/components/ui/progress";
import { Tooltip } from "@/components/ui/tooltip";

type Choice = "yes" | "no" | "skip";
interface Step { index: number; id: string; choice: Choice }

const CHOICE_TEXT: Record<Choice, string> = { yes: "Ja, hab ich", no: "Kein Account", skip: "Weiß nicht" };
const MAX_REASONS = 5;

export function ReviewPage() {
  const snap = useStore((s) => s.snap)!;
  const setStatus = useStore((s) => s.setAccountStatus);
  const setQueued = useStore((s) => s.setQueued);
  const byId = useMemo(() => new Map(snap.accounts.map((a) => [a.id, a])), [snap.accounts]);
  const multi = snap.mailboxes.length > 1;
  const sender = snap.mailboxes.find((m) => m.has_password);
  const [ask, setAsk] = useState<string | null>(null);
  const [remember, setRemember] = useState(true);
  const [qc, setQc] = useState(queueChoice.value);
  const resetQueueChoice = () => { queueChoice.value = null; setQc(null); };

  // The queue is fixed when the page opens, so decisions do not reshuffle it. Strongest first.
  const [queue, setQueue] = useState<string[]>(() => reviewOrder(toReview(snap.accounts)).map((a) => a.id));
  const [i, setI] = useState(0);
  const [history, setHistory] = useState<Step[]>([]);
  const [withWeak, setWithWeak] = useState(false);
  const chain = useRef<Promise<unknown>>(Promise.resolve());

  const weakIds = useMemo(() => snap.accounts.filter(isWeak).map((a) => a.id), [snap.accounts]);
  const weakLeft = withWeak ? 0 : weakIds.length;

  const id = queue[i];
  const a = id ? byId.get(id) : undefined;
  const total = queue.length;

  const decide = (choice: Choice) => {
    if (!a) return;
    const status = choice === "yes" ? "Confirmed" : choice === "no" ? "NotAnAccount" : null;
    if (status) chain.current = chain.current.then(() => setStatus([a.id], status, true));
    setHistory((h) => [...h, { index: i, id: a.id, choice }]);
    if (choice === "yes" && sender) {
      if (queueChoice.value === null) { setAsk(a.id); return; }
      if (queueChoice.value === "yes") chain.current = chain.current.then(() => setQueued([a.id], true, true));
    }
    setI(i + 1);
  };

  /** Second step after "Ja, hab ich": queue the erasure request for the automation, or not. */
  const answerQueue = (queue: boolean) => {
    if (!ask) return;
    const id = ask;
    if (remember) { queueChoice.value = queue ? "yes" : "no"; setQc(queueChoice.value); }
    if (queue) chain.current = chain.current.then(() => setQueued([id], true, true));
    setAsk(null);
    setI((n) => n + 1);
  };

  const undo = () => {
    const last = history[history.length - 1];
    if (!last) return;
    setAsk(null);
    if (last.choice !== "skip") chain.current = chain.current.then(() => setStatus([last.id], "Discovered", true));
    setHistory((h) => h.slice(0, -1));
    setI(last.index);
  };

  const addWeak = () => {
    setQueue((q) => [...q, ...reviewOrder(weakIds.map((w) => byId.get(w)!).filter(Boolean)).map((x) => x.id).filter((x) => !q.includes(x))]);
    setWithWeak(true);
  };
  const again = () => {
    const skipped = history.filter((h) => h.choice === "skip").map((h) => h.id);
    setHistory((h) => h.filter((x) => x.choice !== "skip"));
    setQueue((q) => [...q, ...skipped]);
  };

  const fns = useRef({ decide, undo, answerQueue, ask });
  fns.current = { decide, undo, answerQueue, ask };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isTyping(e.target) || e.altKey || e.metaKey || document.querySelector("[role=dialog],[role=menu],[cmdk-root]")) return;
      const k = e.key.toLowerCase();
      if (e.ctrlKey) { if (k === "z") { e.preventDefault(); fns.current.undo(); } return; }
      if (e.repeat) return;
      if (fns.current.ask) {
        if (k === "1" || k === "j") fns.current.answerQueue(true);
        else if (k === "2" || k === "n" || k === "escape" || k === " ") { e.preventDefault(); fns.current.answerQueue(false); }
        else if (k === "z" || k === "u" || k === "backspace") fns.current.undo();
        return;
      }
      if (k === "escape") navigate({ page: "dashboard" });
      else if (k === "1" || k === "j") fns.current.decide("yes");
      else if (k === "2" || k === "n") fns.current.decide("no");
      else if (k === "3" || k === " ") { e.preventDefault(); fns.current.decide("skip"); }
      else if (k === "z" || k === "u" || k === "backspace") fns.current.undo();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const last = history[history.length - 1];
  const lastAccount = last ? byId.get(last.id) : undefined;
  const count = (c: Choice) => history.filter((h) => h.choice === c).length;
  const confirmedNow = snap.accounts.filter((x) => x.status === "Confirmed").length;

  const head = (
    <div className="flex items-center gap-3 px-7 pt-7">
      <Button variant="ghost" onClick={() => navigate({ page: "dashboard" })}><ArrowLeft /> Übersicht</Button>
      <div className="flex-1" />
      {a && total > 0 && <span className="tnum text-base text-fg-muted">{Math.min(i + 1, total)} von {total}</span>}
      <Tooltip content="Letzte Entscheidung zurücknehmen" shortcut="Z">
        <Button variant="ghost" disabled={!last} onClick={undo}><Undo2 /> Rückgängig</Button>
      </Tooltip>
    </div>
  );

  if (total === 0) {
    return (
      <div className="flex h-full flex-col">
        {head}
        <EmptyState
          icon={ListChecks} className="flex-1" title="Nichts zu prüfen"
          description={snap.accounts.length === 0
            ? "Verbinde ein Postfach oder importiere einen Passwortexport. Danach zeigen wir dir hier jeden Fund einzeln."
            : weakIds.length ? `Es gibt noch ${plural(weakIds.length, "unsicheren Fund", "unsichere Funde")}, bei denen wir kaum Hinweise haben.` : "Neue Funde erscheinen hier nach der nächsten Prüfung deines Postfachs."}
          action={weakIds.length && snap.accounts.length > 0
            ? <Button variant="primary" onClick={addWeak}>Unsichere prüfen ({weakIds.length})</Button>
            : <Button variant="primary" onClick={() => navigate({ page: "sources" })}>{snap.accounts.length === 0 ? "Quelle verbinden" : "Zu den Quellen"}</Button>}
        />
      </div>
    );
  }

  if (!a) {
    const skipped = count("skip");
    return (
      <div className="flex h-full flex-col">
        {head}
        <div className="flex flex-1 flex-col items-center justify-center gap-5 px-7 text-center">
          <Check className="size-8 text-fg-faint" strokeWidth={1.5} />
          <div className="flex max-w-[420px] flex-col gap-1.5">
            <p className="text-lg font-medium">{skipped === 0 && weakLeft === 0 ? "Alles geprüft" : "Das war der Durchgang"}</p>
            <p className="text-base text-fg-muted">
              {count("yes")} als Account bestätigt, {count("no")} ausgeblendet{skipped ? `, ${skipped} übersprungen` : ""}.
              {confirmedNow > 0 ? " Als Nächstes räumst du die bestätigten Accounts auf." : ""}
            </p>
          </div>
          <div className="flex flex-wrap items-center justify-center gap-2">
            {confirmedNow > 0
              ? <Button variant="primary" onClick={() => navigate({ page: "cleanup", tidy: true })}>Aufräumen ({confirmedNow})</Button>
              : <Button variant="primary" onClick={() => navigate({ page: "dashboard" })}>Zur Übersicht</Button>}
            {weakLeft > 0 && (
              <Tooltip content="Bei diesen Funden gibt es kaum Hinweise, zum Beispiel nur Newsletter."><Button onClick={addWeak}>Unsichere prüfen ({weakLeft})</Button></Tooltip>
            )}
            {skipped > 0 && <Button onClick={again}>Übersprungene noch einmal ({skipped})</Button>}
          </div>
        </div>
      </div>
    );
  }

  const upcoming = queue.slice(i + 1, i + 4).map((x) => byId.get(x)).filter((x): x is NonNullable<typeof x> => !!x);
  const reasons = reasonsOf(a, multi);
  const shown = reasons.slice(0, MAX_REASONS);
  const ids = shownIdentifiers(a, snap);
  const unresolved = a.domain.startsWith("unresolved:");

  return (
    <div className="flex h-full flex-col">
      {head}
      <div className="px-7 pt-4"><Progress value={total ? i / total : 0} /></div>
      <div className="flex flex-1 justify-center overflow-y-auto px-7 pb-10 pt-[clamp(24px,7vh,72px)]">
        <div key={a.id} className="anim-pop flex h-fit w-full max-w-[640px] flex-col gap-7">
          <div className="flex items-center gap-4">
            <ServiceLogo name={a.name} domain={a.domain} size={48} />
            <div className="min-w-0 flex-1">
              <h1 className="truncate text-xl font-semibold" title={a.name}>{a.name}</h1>
              <div className="truncate text-base text-fg-muted">
                {unresolved ? "Domain unbekannt" : a.domain}
                {ids.length > 0 && <span className="text-fg-faint">, {identifierLabel(ids[0]!)}{ids.length > 1 ? ` und ${ids.length - 1} weitere` : ""}</span>}
              </div>
            </div>
            <Tooltip content="Wie sicher wir sind, dass du hier einen Account hast. Eindeutig: mehrere klare Hinweise. Unklar: kaum Hinweise.">
              <span className="text-base text-fg-muted">{tierWord(a)}</span>
            </Tooltip>
          </div>

          <section>
            <SectionLabel>Warum wir das glauben</SectionLabel>
            <ul className="mt-2 flex flex-col gap-3.5 border-t border-border pt-4">
              {shown.map((r) => (
                <li key={r.text + r.detail} className="flex items-start gap-3">
                  <span className="mt-[9px] size-[5px] shrink-0 rounded-full bg-fg-faint" />
                  <div className="min-w-0">
                    <div className="text-base">{r.text}</div>
                    {r.detail && <div className="truncate text-sm text-fg-faint" title={r.detail}>{r.detail}</div>}
                  </div>
                </li>
              ))}
              {reasons.length > shown.length && <li className="pl-5 text-sm text-fg-faint">und {plural(reasons.length - shown.length, "weiterer Hinweis", "weitere Hinweise")}</li>}
              {reasons.length === 0 && <li className="text-base text-fg-muted">Wir haben nur den Namen gefunden, ohne weiteren Hinweis.</li>}
            </ul>
          </section>

          {ask === a.id ? (
            <div className="anim-fade flex flex-col gap-3 border-t border-border pt-6">
              <p className="text-lg font-medium">Löschantrag für {a.name} automatisch senden?</p>
              <p className="max-w-[520px] text-base text-fg-muted">
                Wir merken den Account für die Automatik vor und senden den Antrag per Mail von {sender?.address}. Das lässt sich in Aufräumen jederzeit ändern.
              </p>
              <div className="grid grid-cols-2 gap-3 max-[640px]:grid-cols-1">
                <Button variant="primary" className="h-11 gap-2 text-lg" onClick={() => answerQueue(true)}>Ja, vormerken <Kbd className="border-primary-fg/30 bg-transparent text-primary-fg/70">1</Kbd></Button>
                <Button className="h-11 gap-2 text-lg" onClick={() => answerQueue(false)}>Nein, später <Kbd>2</Kbd></Button>
              </div>
              <label className="flex w-fit cursor-default items-center gap-2 text-base text-fg-muted">
                <Checkbox checked={remember} onCheckedChange={(v) => setRemember(v === true)} /> Für diese Sitzung merken
              </label>
            </div>
          ) : (
          <div className="flex flex-col gap-3 border-t border-border pt-6">
            <p className="text-lg font-medium">Hast du bei {a.name} einen Account?</p>
            <div className="grid grid-cols-3 gap-3 max-[640px]:grid-cols-1">
              <Choice label={CHOICE_TEXT.yes} caption="Bestätigt den Fund. Löscht nichts." keyHint="1" primary onClick={() => decide("yes")} tip="Du hast oder hattest hier einen Account. Du kannst ihn später löschen. Taste 1 oder J." />
              <Choice label={CHOICE_TEXT.no} caption="Wird ausgeblendet" keyHint="2" onClick={() => decide("no")} tip="Du hast hier keinen Account. Der Fund verschwindet aus deiner Liste. Taste 2 oder N." />
              <Choice label={CHOICE_TEXT.skip} caption="Bleibt zu prüfen" keyHint="3" onClick={() => decide("skip")} tip="Du entscheidest später. Der Fund bleibt in der Liste. Taste 3 oder Leertaste." />
            </div>
          </div>
          )}

          <div className="flex h-8 items-center justify-between text-sm text-fg-faint">
            {last && lastAccount ? (
              <span className="truncate">Zuletzt: {lastAccount.name}, {CHOICE_TEXT[last.choice]}</span>
            ) : <span>Tipp: Mit den Tasten 1, 2 und 3 geht es schneller.</span>}
            {qc && sender && ask !== a.id && (
              <span className="ml-auto flex items-center gap-1 whitespace-nowrap">
                {qc === "yes" ? "Bestätigte Accounts werden vorgemerkt" : "Löschanträge entscheidest du in Aufräumen"}
                <Button variant="ghost" size="sm" onClick={resetQueueChoice}>Wieder fragen</Button>
              </span>
            )}
            <Button variant="ghost" size="sm" onClick={() => navigate({ page: "accounts", id: a.id })}>Alle Details</Button>
          </div>

          {upcoming.length > 0 && (
            <section className="border-t border-border pt-4">
              <SectionLabel right={<span className="tnum text-sm text-fg-faint">{plural(total - i - 1, "weiterer Fund", "weitere Funde")}</span>}>Als Nächstes</SectionLabel>
              <ul className="mt-1 flex flex-col">
                {upcoming.map((u) => (
                  <li key={u.id} className="flex h-9 items-center gap-3 text-base text-fg-muted">
                    <ServiceLogo name={u.name} domain={u.domain} size={20} />
                    <span className="flex-1 truncate">{u.name}</span>
                    <span className="text-sm text-fg-faint">{tierWord(u)}</span>
                  </li>
                ))}
              </ul>
            </section>
          )}
        </div>
      </div>
    </div>
  );
}

function Choice({ label, caption, keyHint, primary, onClick, tip }: {
  label: string; caption: string; keyHint: string; primary?: boolean; onClick(): void; tip: string;
}) {
  return (
    <div className="flex flex-col items-stretch gap-1.5">
      <Tooltip content={tip} side="top">
        <Button variant={primary ? "primary" : "secondary"} className="h-11 gap-2 text-lg" onClick={onClick}>
          {label} <Kbd className={primary ? "border-primary-fg/30 bg-transparent text-primary-fg/70" : ""}>{keyHint}</Kbd>
        </Button>
      </Tooltip>
      <span className="text-center text-sm text-fg-faint">{caption}</span>
    </div>
  );
}
