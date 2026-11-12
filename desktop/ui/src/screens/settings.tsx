import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { ExternalLink, Loader2 } from "lucide-react";
import { api, runJob } from "@/lib/api";
import { useStore } from "@/lib/store";
import { openUrl, pickSavePath, requestNotifications } from "@/lib/platform";
import { useAsync } from "@/lib/hooks";
import type { AiProvider, SettingsPatch } from "@/lib/types";
import { cn, errorMessage } from "@/lib/utils";
import { Dot, PageHeader } from "@/components/common";
import { PageShell } from "@/components/page";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input, Textarea } from "@/components/ui/input";
import { Segmented } from "@/components/ui/segmented";
import { Select } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { confirm } from "@/components/confirm";
import { passwordStrength } from "./unlock";

const STATES: [string, string][] = [
  ["", "Aus der Postleitzahl ableiten"], ["DE-BW", "Baden-Württemberg"], ["DE-BY", "Bayern"], ["DE-BE", "Berlin"], ["DE-BB", "Brandenburg"],
  ["DE-HB", "Bremen"], ["DE-HH", "Hamburg"], ["DE-HE", "Hessen"], ["DE-MV", "Mecklenburg-Vorpommern"], ["DE-NI", "Niedersachsen"],
  ["DE-NW", "Nordrhein-Westfalen"], ["DE-RP", "Rheinland-Pfalz"], ["DE-SL", "Saarland"], ["DE-SN", "Sachsen"], ["DE-ST", "Sachsen-Anhalt"],
  ["DE-SH", "Schleswig-Holstein"], ["DE-TH", "Thüringen"],
];

function Section({ id, title, children }: { id: string; title: string; children: React.ReactNode }) {
  return (
    <section id={`s-${id}`} className="scroll-mt-6">
      <h2 className="border-b border-border pb-2 text-lg font-semibold">{title}</h2>
      <div className="flex flex-col">{children}</div>
    </section>
  );
}

function Row({ label, hint, children, stack }: { label: string; hint?: React.ReactNode; children: React.ReactNode; stack?: boolean }) {
  return (
    <div className={cn("flex gap-6 border-b border-border py-3 last:border-b-0", stack ? "flex-col gap-2" : "items-center justify-between max-md:flex-col max-md:items-stretch max-md:gap-2")}>
      <div className="min-w-0 max-w-[460px]">
        <div className="text-base">{label}</div>
        {hint && <div className="mt-0.5 text-sm text-fg-muted">{hint}</div>}
      </div>
      <div className={cn("shrink-0", stack ? "w-full" : "w-[300px] max-md:w-full")}>{children}</div>
    </div>
  );
}

/** Text field that saves on blur. */
function Field({ value, onSave, type = "text", placeholder, multiline }: { value: string; onSave(v: string): void; type?: string; placeholder?: string; multiline?: boolean }) {
  const [v, setV] = useState(value);
  useEffect(() => setV(value), [value]);
  const commit = () => { if (v !== value) onSave(v); };
  return multiline
    ? <Textarea rows={2} value={v} placeholder={placeholder} onChange={(e) => setV(e.target.value)} onBlur={commit} />
    : <Input type={type} value={v} placeholder={placeholder} onChange={(e) => setV(e.target.value)} onBlur={commit} onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()} />;
}

export function SettingsPage({ section }: { section?: string }) {
  const snap = useStore((s) => s.snap)!;
  const vaultPath = useStore((s) => s.vaultPath);
  const st = snap.settings;
  const ai = st.ai;
  const save = (patch: SettingsPatch) => api.settings.update(patch).catch((e) => toast.error(errorMessage(e)));
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!section) return;
    document.getElementById(`s-${section}`)?.scrollIntoView({ block: "start" });
  }, [section]);

  const [hibp, setHibp] = useState("");
  const [anth, setAnth] = useState("");
  const [testing, setTesting] = useState(false);
  const [aiResult, setAiResult] = useState<{ ok: boolean; text: string } | null>(null);
  const [pwOpen, setPwOpen] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const models = useAsync(() => api.settings.aiModels(), [ai.provider, ai.ollama_url, ai.has_anthropic_key]);

  const secret = async (kind: "hibp" | "anthropic", v: string) => {
    try { await api.settings.setSecret(kind, v); toast(v ? "Schlüssel gespeichert" : "Schlüssel entfernt"); kind === "hibp" ? setHibp("") : setAnth(""); } catch (e) { toast.error(errorMessage(e)); }
  };
  const test = async () => {
    setTesting(true); setAiResult(null);
    try { setAiResult({ ok: true, text: await api.settings.aiTest() }); } catch (e) { setAiResult({ ok: false, text: errorMessage(e) }); } finally { setTesting(false); }
  };
  const exportJson = async () => {
    if (!(await confirm({ title: "Datenexport erstellen?", text: "Die JSON Datei ist nicht verschlüsselt und enthält alles aus deinem Tresor. Lege sie nur an einem sicheren Ort ab.", confirm: "Weiter" }))) return;
    try {
      const p = await pickSavePath("ausgeloggt-export.json", "json", "JSON");
      if (p) { await api.vault.exportJson(p); toast("Export gespeichert"); }
    } catch (e) { toast.error(errorMessage(e)); }
  };
  const kit = async (kind: "legacy" | "report") => {
    try {
      const p = await pickSavePath(kind === "legacy" ? "ausgeloggt-notfallmappe.pdf" : "ausgeloggt-bericht.pdf", "pdf", "PDF");
      if (p) { await api.settings.generatePdfKit(kind, p); toast("PDF gespeichert"); }
    } catch (e) { toast.error(errorMessage(e)); }
  };
  const refreshCatalog = async () => {
    setRefreshing(true);
    try { await runJob(() => api.settings.catalogRefreshStart()); } catch (e) { toast.error(errorMessage(e)); } finally { setRefreshing(false); }
  };
  const notif = async (on: boolean) => {
    if (on && !(await requestNotifications().catch(() => false))) { toast.error("Das System erlaubt keine Benachrichtigungen."); return; }
    void save({ notifications: on });
  };

  const modelList = (kind: "text") => models.data?.[kind] ?? [];
  const idn = st.identity;
  const setId = (k: keyof typeof idn, v: string) => void save({ identity: { [k]: v } });

  return (
    <PageShell className="max-w-[760px]">
      <div ref={ref}><PageHeader title="Einstellungen" /></div>

      <Section id="identity" title="Absender für Briefe">
        <Row label="Name"><Field value={st.full_name} onSave={(v) => void save({ full_name: v })} placeholder="Vor und Nachname" /></Row>
        <Row label="Straße und Hausnummer"><Field value={idn.street} onSave={(v) => setId("street", v)} /></Row>
        <Row label="Postleitzahl"><Field value={idn.postal_code} onSave={(v) => setId("postal_code", v)} /></Row>
        <Row label="Ort"><Field value={idn.city} onSave={(v) => setId("city", v)} /></Row>
        <Row label="Land"><Field value={idn.country} onSave={(v) => setId("country", v)} /></Row>
        <Row label="Bundesland" hint="Bestimmt die Aufsichtsbehörde für Beschwerden."><Select className="w-full" value={idn.state} onChange={(e) => setId("state", e.target.value)}>{STATES.map(([v, l]) => <option key={v} value={v}>{l}</option>)}</Select></Row>
        <Row label="Geburtsdatum" hint="Auskunfteien verlangen es zur Identifizierung."><Field type="date" value={idn.birth_date} onSave={(v) => setId("birth_date", v)} /></Row>
        <Row label="Frühere Adressen" hint="Eine pro Zeile." stack>
          <Field multiline value={idn.previous_addresses.join("\n")} onSave={(v) => void save({ identity: { previous_addresses: v.split("\n").map((l) => l.trim()).filter(Boolean) } })} />
        </Row>
        <Row label="Sprache der Briefe">
          <Select className="w-full" value={st.letter_language} onChange={(e) => void save({ letter_language: e.target.value as "de" | "en" })}><option value="de">Deutsch</option><option value="en">Englisch</option></Select>
        </Row>
      </Section>

      <Section id="general" title="Sperre und Benachrichtigungen">
        <Row label="Automatisch sperren nach">
          <Select className="w-full" value={String(st.auto_lock_minutes)} onChange={(e) => void save({ auto_lock_minutes: Number(e.target.value) })}>
            {[1, 5, 10, 15, 30, 60].map((m) => <option key={m} value={m}>{m === 60 ? "1 Stunde" : `${m} ${m === 1 ? "Minute" : "Minuten"}`}</option>)}
            {![1, 5, 10, 15, 30, 60].includes(st.auto_lock_minutes) && <option value={st.auto_lock_minutes}>{st.auto_lock_minutes} Minuten</option>}
          </Select>
        </Row>
        <Row label="Benachrichtigungen" hint="Überfällige Fristen, neue Accounts, Datenlecks und bevorstehende Abbuchungen."><Switch checked={st.notifications} onCheckedChange={(v) => void notif(v)} /></Row>
        <Row label="Postfächer automatisch prüfen">
          <Select className="w-full" value={String(st.auto_rescan_days)} onChange={(e) => void save({ auto_rescan_days: Number(e.target.value) })}>
            <option value="0">Nie</option>
            {[1, 3, 7, 14, 30].map((d) => <option key={d} value={d}>{d === 1 ? "Täglich" : `Alle ${d} Tage`}</option>)}
          </Select>
        </Row>
        <Row label="Rechnungstexte lesen" hint="Erkennt Beträge und Zahlarten für Abos. Der Mailtext bleibt auf diesem Gerät."><Switch checked={st.read_receipt_bodies} onCheckedChange={(v) => void save({ read_receipt_bodies: v })} /></Row>
        <Row label="Logos laden" hint="Holt das Symbol von der Webseite des Dienstes. Dabei sieht der Dienst eine Anfrage."><Switch checked={st.fetch_logos} onCheckedChange={(v) => void save({ fetch_logos: v })} /></Row>
        <Row label="Mindestsicherheit für Treffer" hint="Accounts darunter werden grau dargestellt.">
          <Select className="w-full" value={String(st.min_confidence)} onChange={(e) => void save({ min_confidence: Number(e.target.value) })}>
            {[0.3, 0.5, 0.7].map((v) => <option key={v} value={v}>{Math.round(v * 100)} Prozent</option>)}
          </Select>
        </Row>
      </Section>

      <Section id="security" title="Datenlecks">
        <Row label="Have I Been Pwned Schlüssel" hint={<>{st.has_hibp_key ? <span className="inline-flex items-center gap-1.5"><Dot className="bg-s-green" />Schlüssel hinterlegt. </span> : "Prüft zusätzlich deine Mailadressen. "}<button className="text-fg underline underline-offset-2" onClick={() => void openUrl("https://haveibeenpwned.com/API/Key")}>Schlüssel besorgen</button></>} stack>
          <div className="flex gap-2">
            <Input type="password" value={hibp} onChange={(e) => setHibp(e.target.value)} placeholder={st.has_hibp_key ? "Neuen Schlüssel eingeben" : "Schlüssel einfügen"} autoComplete="off" />
            <Button disabled={!hibp.trim()} onClick={() => void secret("hibp", hibp.trim())}>Speichern</Button>
            {st.has_hibp_key && <Button variant="ghost" onClick={() => void secret("hibp", "")}>Entfernen</Button>}
          </div>
        </Row>
        <Row label="Passwörter beim Import prüfen" hint="Nur ein kurzer Teil des Hashwerts verlässt das Gerät."><Switch checked={st.check_pwned_passwords} onCheckedChange={(v) => void save({ check_pwned_passwords: v })} /></Row>
      </Section>

      <Section id="ai" title="KI">
        <Row label="Anbieter" hint="Optional. Hilft beim Einordnen von Newslettern und beim Lesen von Screenshots.">
          <Segmented<AiProvider> value={ai.provider} onChange={(v) => { setAiResult(null); void save({ ai: { provider: v } }); }} options={[{ value: "Off", label: "Aus" }, { value: "Ollama", label: "Ollama" }, { value: "Anthropic", label: "Anthropic" }]} />
        </Row>
        {ai.provider === "Ollama" && (
          <>
            <Row label="Adresse von Ollama" hint="Läuft lokal. Nichts verlässt dein Gerät."><Field value={ai.ollama_url} onSave={(v) => void save({ ai: { ollama_url: v } })} /></Row>
            <Row label="Modell" hint={models.error ? "Ollama ist nicht erreichbar." : undefined}>
              <Select className="w-full" value={ai.ollama_model} onChange={(e) => void save({ ai: { ollama_model: e.target.value } })}>
                <option value="">Bitte wählen</option>
                {[...new Set([...(ai.ollama_model ? [ai.ollama_model] : []), ...modelList("text")])].map((m) => <option key={m} value={m}>{m}</option>)}
              </Select>
            </Row>
          </>
        )}
        {ai.provider === "Anthropic" && (
          <>
            <Row label="Schlüssel" hint={<>{ai.has_anthropic_key ? "Schlüssel hinterlegt. " : ""}Die Cloud wird genutzt. Texte aus Mails verlassen dein Gerät.</>} stack>
              <div className="flex gap-2">
                <Input type="password" value={anth} onChange={(e) => setAnth(e.target.value)} placeholder={ai.has_anthropic_key ? "Neuen Schlüssel eingeben" : "Schlüssel einfügen"} autoComplete="off" />
                <Button disabled={!anth.trim()} onClick={() => void secret("anthropic", anth.trim())}>Speichern</Button>
                {ai.has_anthropic_key && <Button variant="ghost" onClick={() => void secret("anthropic", "")}>Entfernen</Button>}
              </div>
            </Row>
            <Row label="Modell">
              <Select className="w-full" value={ai.anthropic_model} onChange={(e) => void save({ ai: { anthropic_model: e.target.value } })}>
                {[...new Set([ai.anthropic_model, ...modelList("text")])].map((m) => <option key={m} value={m}>{m}</option>)}
              </Select>
            </Row>
          </>
        )}
        {ai.provider !== "Off" && (
          <Row label="Verbindung" hint={aiResult ? <span className="inline-flex items-center gap-1.5"><Dot className={aiResult.ok ? "bg-s-green" : "bg-s-red"} />{aiResult.text}</span> : undefined}>
            <Button disabled={testing} onClick={() => void test()}>{testing && <Loader2 className="animate-spin" />} Testen</Button>
          </Row>
        )}
      </Section>

      <Section id="catalog" title="Katalog">
        <Row label="Katalog aktualisieren" hint="Löschseiten und 2FA Daten. Lädt nur öffentliche Listen."><Button disabled={refreshing} onClick={() => void refreshCatalog()}>{refreshing && <Loader2 className="animate-spin" />} Jetzt aktualisieren</Button></Row>
        <Row label="Automatische Updates"><Switch checked={st.allow_catalog_updates} onCheckedChange={(v) => void save({ allow_catalog_updates: v })} /></Row>
      </Section>

      <Section id="vault" title="Tresor und Daten">
        <Row label="Speicherort" hint={<span className="selectable break-all font-mono text-xs">{vaultPath || "Standardordner"}</span>}><span /></Row>
        <Row label="Hauptpasswort ändern" hint="Das Passwort lässt sich nicht wiederherstellen."><Button onClick={() => setPwOpen(true)}>Ändern</Button></Row>
        <Row label="Daten exportieren" hint="Alle Daten als JSON Datei."><Button onClick={() => void exportJson()}>Als JSON speichern</Button></Row>
        <Row label="Notfallmappe" hint="PDF mit Accounts, Abos und offenen Fristen für den Notfall."><Button onClick={() => void kit("legacy")}>Als PDF speichern</Button></Row>
        <Row label="Bericht" hint="PDF mit Risiken und Empfehlungen."><Button onClick={() => void kit("report")}>Als PDF speichern</Button></Row>
      </Section>

      <Section id="about" title="Über ausgeloggt">
        <Row label="Version"><span className="text-base text-fg-muted">0.1.0</span></Row>
        <Row label="Lizenz" hint="Freie Software unter MIT Lizenz. Alle Daten bleiben lokal."><Button variant="secondary" onClick={() => void openUrl("https://github.com/HenrySchwerdt/ausgeloggt")}>Quellcode und Lizenzen <ExternalLink /></Button></Row>
      </Section>

      <PasswordDialog open={pwOpen} onClose={() => setPwOpen(false)} />
    </PageShell>
  );
}

function PasswordDialog({ open, onClose }: { open: boolean; onClose(): void }) {
  const [cur, setCur] = useState(""); const [next, setNext] = useState(""); const [rep, setRep] = useState("");
  const [busy, setBusy] = useState(false); const [err, setErr] = useState<string | null>(null);
  useEffect(() => { if (open) { setCur(""); setNext(""); setRep(""); setErr(null); } }, [open]);
  const strength = passwordStrength(next);
  async function go() {
    setErr(null);
    if (next !== rep) return setErr("Die neuen Passwörter stimmen nicht überein.");
    setBusy(true);
    try { await api.vault.changePassword(cur, next); toast("Hauptpasswort geändert"); onClose(); } catch (e) { setErr(errorMessage(e)); } finally { setBusy(false); }
  }
  return (
    <Dialog open={open} onOpenChange={(o) => !o && !busy && onClose()}>
      <DialogContent className="max-w-[420px]">
        <DialogHeader><DialogTitle>Hauptpasswort ändern</DialogTitle></DialogHeader>
        <form className="flex flex-col gap-2 px-5 pb-4" onSubmit={(e) => { e.preventDefault(); void go(); }}>
          <Input type="password" autoFocus placeholder="Aktuelles Passwort" value={cur} onChange={(e) => setCur(e.target.value)} />
          <Input type="password" placeholder="Neues Passwort, mindestens 10 Zeichen" value={next} onChange={(e) => setNext(e.target.value)} />
          <Input type="password" placeholder="Neues Passwort wiederholen" value={rep} onChange={(e) => setRep(e.target.value)} />
          {next && <p className="flex items-center gap-2 text-sm text-fg-muted"><Dot className={strength.dot} />{strength.label}</p>}
          {err && <p className="flex items-start gap-2 text-sm"><Dot className="mt-1.5 bg-s-red" />{err}</p>}
          <button type="submit" hidden />
        </form>
        <DialogFooter>
          <Button variant="ghost" disabled={busy} onClick={onClose}>Abbrechen</Button>
          <Button variant="primary" disabled={busy || !cur || next.length < 10} onClick={() => void go()}>{busy && <Loader2 className="animate-spin" />} Ändern</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
