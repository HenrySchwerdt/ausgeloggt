import { useRef, useState } from "react";
import { toast } from "sonner";
import { ChevronDown, Check, Eraser, HardDrive, KeyRound, ListChecks, Loader2, Mail, MailOpen } from "lucide-react";
import { api, runJob } from "@/lib/api";
import { useStore } from "@/lib/store";
import { navigate } from "@/lib/router";
import { MAIL_PROVIDERS } from "@/lib/guides";
import { discoveryStats } from "@/lib/derive";
import type { ScanDonePayload } from "@/lib/types";
import { cn, errorMessage } from "@/lib/utils";
import { Dot, LogoMark, ServiceLogo } from "@/components/common";
import { CountUp } from "@/components/count-up";
import { ImportPanel } from "@/components/import-panel";
import { MailboxForm, ProviderLogo } from "@/components/mailbox-form";
import { Titlebar } from "@/components/titlebar";
import { Button } from "@/components/ui/button";
import { Input, Textarea } from "@/components/ui/input";
import { Progress } from "@/components/ui/progress";
import { Select } from "@/components/ui/select";
import { STATES } from "./settings";

const STEPS = 6;

export function Onboarding() {
  const snap = useStore((s) => s.snap)!;
  const close = useStore((s) => s.closeOnboarding);
  const [step, setStep] = useState(0);

  // Identity form (step 3), saved when leaving the step
  const idn = snap.settings.identity;
  const [name, setName] = useState(snap.settings.full_name);
  const [street, setStreet] = useState(idn.street);
  const [zip, setZip] = useState(idn.postal_code);
  const [city, setCity] = useState(idn.city);
  const [state, setState] = useState(idn.state);
  const [birth, setBirth] = useState(idn.birth_date);
  const [prev, setPrev] = useState(idn.previous_addresses.join("\n"));
  const [saving, setSaving] = useState(false);

  // Mailboxes (step 4): each one is scanned in the background, one after the other
  const [boxes, setBoxes] = useState<Box[]>([]);
  const [addingBox, setAddingBox] = useState(true);
  const scanChain = useRef<Promise<unknown>>(Promise.resolve());

  // Import (step 5)
  const [importedFiles, setImportedFiles] = useState(0);

  const stats = discoveryStats(snap.accounts);
  const identityFilled = [name, street, zip, city, state, birth, prev].some((v) => v.trim());

  const finish = (to: "review" | "dashboard") => {
    close();
    navigate({ page: to });
  };

  async function saveIdentity(): Promise<boolean> {
    if (!identityFilled) return true;
    setSaving(true);
    try {
      await api.settings.update({
        full_name: name.trim(),
        identity: {
          street: street.trim(), postal_code: zip.trim(), city: city.trim(), state, birth_date: birth,
          previous_addresses: prev.split("\n").map((l) => l.trim()).filter(Boolean),
        },
      });
      return true;
    } catch (e) { toast.error(errorMessage(e)); return false; } finally { setSaving(false); }
  }

  function connected(id: string, address: string) {
    const upd = (p: Partial<Box>) => setBoxes((l) => l.map((x) => (x.id === id ? { ...x, ...p } : x)));
    setBoxes((l) => [...l, { id, address, state: "queued", msg: "" }]);
    setAddingBox(false);
    scanChain.current = scanChain.current.then(async () => {
      upd({ state: "scanning" });
      try {
        const d = await runJob<ScanDonePayload>(() => api.mail.scanStart([id], false), { onProgress: (p) => upd({ msg: p.message, done: p.done, total: p.total }) });
        if (d.payload) upd({ state: "done", res: d.payload });
        else upd({ state: "error", msg: d.error?.message ?? d.summary });
      } catch (e) { upd({ state: "error", msg: errorMessage(e) }); }
    });
  }

  const next = async () => {
    if (step === 2 && !(await saveIdentity())) return;
    setStep((s) => Math.min(STEPS - 1, s + 1));
  };
  const back = () => setStep((s) => Math.max(0, s - 1));
  const last = step === STEPS - 1;

  // Step 4 and 5: the action inside the step is primary until it was done
  const mailDone = boxes.length > 0;
  const importDone = importedFiles > 0;
  const nextPrimary = step === 3 ? mailDone && !addingBox : step === 4 ? importDone : true;

  return (
    <div className="flex h-full flex-col bg-bg">
      <div className="[&>header]:border-transparent"><Titlebar minimal /></div>

      <div className="flex items-center justify-between px-7 pt-2">
        <div className="flex items-center gap-2 text-base text-fg-muted"><LogoMark size={18} /> ausgeloggt</div>
        <button onClick={() => finish("dashboard")} className="text-base text-fg-faint transition-colors hover:text-fg">Einführung beenden</button>
      </div>

      <main className="flex min-h-0 flex-1 flex-col items-center overflow-y-auto px-6">
        <div key={step} className="anim-pop my-auto flex w-full max-w-[520px] flex-col gap-6 py-8">
          {step === 0 && <Welcome />}
          {step === 1 && <Privacy />}
          {step === 2 && (
            <Identity {...{ name, setName, street, setStreet, zip, setZip, city, setCity, state, setState, birth, setBirth, prev, setPrev }} />
          )}
          {step === 3 && <MailboxStep boxes={boxes} adding={addingBox} setAdding={setAddingBox} onSaved={connected} />}
          {step === 4 && <ImportStep onImported={setImportedFiles} />}
          {step === 5 && <Next stats={stats} scanning={boxes.some((b) => b.state === "queued" || b.state === "scanning")} />}
        </div>
      </main>

      <footer className="flex flex-col items-center gap-4 px-7 pb-7 pt-3">
        <div className="flex items-center gap-2" aria-label={`Schritt ${step + 1} von ${STEPS}`}>
          {Array.from({ length: STEPS }, (_, i) => (
            <span key={i} className={cn("h-1.5 rounded-full transition-all", i === step ? "w-5 bg-fg" : i < step ? "w-1.5 bg-fg-muted" : "w-1.5 bg-border-strong")} />
          ))}
        </div>
        <div className="flex w-full max-w-[520px] items-center gap-2">
          <Button variant="ghost" onClick={back} className={cn(step === 0 && "invisible")}>Zurück</Button>
          <div className="flex-1" />
          {last ? (
            <>
              {stats.total > 0 && <Button variant="ghost" onClick={() => finish("dashboard")}>Zur Übersicht</Button>}
              <Button variant="primary" onClick={() => finish(stats.total > 0 ? "review" : "dashboard")}>{stats.total > 0 ? "Funde prüfen" : "Zur Übersicht"}</Button>
            </>
          ) : (
            <>
              <Button variant="ghost" onClick={() => setStep((s) => s + 1)}>Überspringen</Button>
              <Button variant={nextPrimary ? "primary" : "secondary"} disabled={saving} onClick={() => void next()}>
                {saving && <Loader2 className="animate-spin" />} Weiter
              </Button>
            </>
          )}
        </div>
      </footer>

    </div>
  );
}

// ---------------------------------------------------------------------------------------------

function Heading({ title, children }: { title: string; children?: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <h1 className="text-xl font-semibold tracking-[-0.01em]">{title}</h1>
      {children && <p className="text-base text-fg-muted">{children}</p>}
    </div>
  );
}

/** Little picture made of the app's own pieces: a list of finds and the three answers. */
function Illustration() {
  const rows = [
    { n: "Spotify", d: "spotify.com", t: "Willkommensmail 2021", w: "Sicher" },
    { n: "Duolingo", d: "duolingo.com", t: "Mit Apple angemeldet", w: "Wahrscheinlich" },
    { n: "Amazon", d: "amazon.de", t: "Bestellung 2022, Passwort in Chrome", w: "Sicher" },
  ];
  return (
    <div aria-hidden className="pointer-events-none select-none overflow-hidden rounded-lg border border-border">
      {rows.map((r, i) => (
        <div key={r.n} className={cn("flex h-11 items-center gap-3 border-b border-border px-3", i === 0 && "bg-muted")}>
          <ServiceLogo name={r.n} domain={r.d} />
          <div className="min-w-0 flex-1">
            <div className="truncate text-base font-medium leading-[18px]">{r.n}</div>
            <div className="truncate text-sm leading-4 text-fg-faint">{r.t}</div>
          </div>
          <span className="text-sm text-fg-muted">{r.w}</span>
        </div>
      ))}
      <div className="flex items-center gap-2 px-3 py-2.5">
        <span className="flex h-6 items-center rounded-md bg-primary px-2 text-sm font-medium text-primary-fg">Ja, hab ich</span>
        <span className="flex h-6 items-center rounded-md bg-muted px-2 text-sm font-medium">Kein Account</span>
        <span className="flex h-6 items-center rounded-md bg-muted px-2 text-sm font-medium">Weiß nicht</span>
        <span className="flex-1" />
        <span className="flex items-center gap-1.5 text-sm text-fg-muted"><Dot className="bg-s-yellow" />Löschung läuft</span>
      </div>
    </div>
  );
}

function Welcome() {
  return (
    <>
      <Illustration />
      <Heading title="Willkommen bei ausgeloggt" />
      <ol className="flex flex-col gap-3 text-base">
        {[
          "ausgeloggt findet heraus, wo du überall einen Account hast.",
          "Du bestätigst jeden Fund mit einem Klick.",
          "Danach löschst du Accounts und kündigst Abos, Schritt für Schritt.",
        ].map((t, i) => (
          <li key={t} className="flex gap-3"><span className="tnum w-4 text-fg-faint">{i + 1}</span><span>{t}</span></li>
        ))}
      </ol>
    </>
  );
}

function Fact({ icon: Icon, title, text }: { icon: typeof Mail; title: string; text: string }) {
  return (
    <div className="flex gap-3">
      <Icon className="mt-0.5 size-4 shrink-0 text-fg-muted" strokeWidth={1.75} />
      <div>
        <div className="text-base font-medium">{title}</div>
        <div className="text-base text-fg-muted">{text}</div>
      </div>
    </div>
  );
}

function Privacy() {
  return (
    <>
      <Heading title="Deine Daten bleiben hier" />
      <div className="flex flex-col gap-5 border-t border-border pt-5">
        <Fact icon={MailOpen} title="Wir lesen nur die Köpfe deiner Mails" text="Das sind Absender, Betreff und Datum. Der Inhalt bleibt ungelesen." />
        <Fact icon={HardDrive} title="Nichts verlässt dieses Gerät" text="Alles liegt verschlüsselt in einer Datei auf deinem Rechner. Es gibt keinen Server und kein Konto bei uns." />
        <Fact icon={KeyRound} title="Das Hauptpasswort lässt sich nicht zurücksetzen" text="Wer es vergisst, verliert die Daten im Tresor. Bewahre es an einem sicheren Ort auf." />
      </div>
    </>
  );
}

function Labeled({ label, children, className }: { label: string; children: React.ReactNode; className?: string }) {
  return <label className={cn("flex flex-col gap-1 text-sm text-fg-muted", className)}>{label}{children}</label>;
}

function Identity(p: {
  name: string; setName(v: string): void; street: string; setStreet(v: string): void; zip: string; setZip(v: string): void;
  city: string; setCity(v: string): void; state: string; setState(v: string): void; birth: string; setBirth(v: string): void;
  prev: string; setPrev(v: string): void;
}) {
  const [more, setMore] = useState(!!(p.birth || p.prev));
  return (
    <>
      <Heading title="Deine Angaben für Briefe">
        Löschanträge sind Briefe oder Mails, und die brauchen einen Absender. Alle Angaben sind freiwillig und bleiben auf diesem Gerät.
      </Heading>
      <div className="flex flex-col gap-3">
        <Labeled label="Name"><Input autoFocus value={p.name} onChange={(e) => p.setName(e.target.value)} placeholder="Vor und Nachname" autoComplete="off" /></Labeled>
        <Labeled label="Straße und Hausnummer"><Input value={p.street} onChange={(e) => p.setStreet(e.target.value)} autoComplete="off" /></Labeled>
        <div className="grid grid-cols-[110px_1fr] gap-2">
          <Labeled label="Postleitzahl"><Input value={p.zip} onChange={(e) => p.setZip(e.target.value.replace(/\D/g, "").slice(0, 5))} inputMode="numeric" autoComplete="off" /></Labeled>
          <Labeled label="Ort"><Input value={p.city} onChange={(e) => p.setCity(e.target.value)} autoComplete="off" /></Labeled>
        </div>
        <Labeled label="Bundesland">
          <Select className="w-full" value={p.state} onChange={(e) => p.setState(e.target.value)}>
            {STATES.map(([v, l]) => <option key={v} value={v}>{l}</option>)}
          </Select>
        </Labeled>
        <div className="border-t border-border pt-3">
          <button onClick={() => setMore((m) => !m)} className="flex items-center gap-1.5 text-base text-fg-muted transition-colors hover:text-fg" aria-expanded={more}>
            Für SCHUFA und Auskunfteien <ChevronDown className={cn("size-3.5 transition-transform", more && "rotate-180")} />
          </button>
          {more && (
            <div className="anim-fade mt-3 flex flex-col gap-3">
              <p className="text-sm text-fg-faint">Auskunfteien verlangen diese Angaben zur Identifizierung. Du brauchst sie nur für eine Selbstauskunft.</p>
              <Labeled label="Geburtsdatum"><Input type="date" value={p.birth} onChange={(e) => p.setBirth(e.target.value)} /></Labeled>
              <Labeled label="Frühere Adressen, eine pro Zeile"><Textarea rows={2} value={p.prev} onChange={(e) => p.setPrev(e.target.value)} /></Labeled>
            </div>
          )}
        </div>
      </div>
    </>
  );
}

interface Box {
  id: string; address: string; state: "queued" | "scanning" | "done" | "error"; msg: string; done?: number; total?: number; res?: ScanDonePayload;
}

function providerOf(address: string) {
  const dom = address.split("@")[1]?.toLowerCase() ?? "";
  return MAIL_PROVIDERS.find((p) => p.domains.includes(dom)) ?? MAIL_PROVIDERS[MAIL_PROVIDERS.length - 1]!;
}

function BoxRow({ b }: { b: Box }) {
  const p = providerOf(b.address);
  const live = b.state === "scanning" || b.state === "queued";
  return (
    <div className="anim-fade flex flex-col gap-2.5 border-b border-border px-4 py-3.5 last:border-b-0">
      <div className="flex items-center gap-3">
        <ProviderLogo p={p} size={28} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-base font-medium">{b.address}</div>
          <div className="truncate text-sm text-fg-muted">
            {b.state === "queued" && "Wartet auf die Prüfung"}
            {b.state === "scanning" && (b.msg ? b.msg.replace(b.address + ": ", "") : "Wird geprüft")}
            {b.state === "done" && b.res && <><CountUp value={b.res.mails} /> Mails geprüft, <CountUp value={b.res.findings} /> Hinweise, <CountUp value={b.res.newAccounts} /> neue Funde</>}
            {b.state === "error" && b.msg}
          </div>
        </div>
        {live ? <Loader2 className="size-4 animate-spin text-fg-muted" /> : b.state === "done" ? <Check className="size-4 text-s-green" /> : <Dot className="bg-s-red" />}
      </div>
      {b.state === "scanning" && <Progress value={b.total ? (b.done ?? 0) / b.total : undefined} />}
    </div>
  );
}

function MailboxStep({ boxes, adding, setAdding, onSaved }: { boxes: Box[]; adding: boolean; setAdding(v: boolean): void; onSaved(id: string, address: string): void }) {
  return (
    <>
      <Heading title="Postfächer verbinden">
        Willkommensmails und Rechnungen verraten, wo du angemeldet bist. Der Zugriff ist nur lesend. Du kannst mehrere Postfächer verbinden.
      </Heading>
      {boxes.length > 0 && <div className="overflow-hidden rounded-lg border border-border">{boxes.map((b) => <BoxRow key={b.id} b={b} />)}</div>}
      {adding
        ? <MailboxForm inline quiet onSaved={onSaved} onCancel={boxes.length ? () => setAdding(false) : undefined} />
        : <div><Button variant="secondary" onClick={() => setAdding(true)}>Weiteres Postfach hinzufügen</Button></div>}
      {boxes.some((b) => b.state === "scanning" || b.state === "queued") && !adding && (
        <p className="text-sm text-fg-faint">Die Prüfung läuft im Hintergrund. Du kannst schon weitermachen.</p>
      )}
    </>
  );
}

function ImportStep({ onImported }: { onImported(n: number): void }) {
  return (
    <>
      <Heading title="Passwortexport importieren">
        Browser und Passwortmanager exportieren ihre Logins als Datei. Die Datei wird nur auf diesem Gerät gelesen. Passwörter werden kurz geprüft und nie gespeichert. Du kannst mehrere Manager nacheinander hinzufügen.
      </Heading>
      <ImportPanel onImported={onImported} />
    </>
  );
}

function Next({ stats, scanning }: { stats: ReturnType<typeof discoveryStats>; scanning: boolean }) {
  return (
    <>
      <Heading title="So geht es weiter">
        {stats.total > 0
          ? <><CountUp value={stats.total} /> {stats.total === 1 ? "Fund" : "Funde"}: {stats.high} sicher, {stats.mid} wahrscheinlich, {stats.low} unsicher.{scanning ? " Die Prüfung läuft noch, es kommen weitere dazu." : ""}</>
          : scanning ? "Dein Postfach wird noch geprüft. Die Funde erscheinen gleich." : "Sobald du eine Quelle verbunden hast, erscheinen hier deine Funde."}
      </Heading>
      <div className="flex flex-col gap-5 border-t border-border pt-5">
        <Fact icon={ListChecks} title="Prüfen" text="Du siehst jeden Fund einzeln und sagst, ob du dort einen Account hast." />
        <Fact icon={Eraser} title="Aufräumen" text="Wir führen dich Schritt für Schritt durchs Löschen und Kündigen." />
        <Fact icon={Mail} title="Newsletter" text="Melde dich von Werbemails ab, die du nicht mehr willst." />
      </div>
    </>
  );
}
