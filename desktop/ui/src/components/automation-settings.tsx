// Settings: automation rules (one per mailbox) and how the app keeps running in the background.
import { useState } from "react";
import { toast } from "sonner";
import { Loader2, Play } from "lucide-react";
import { api, runJob } from "@/lib/api";
import { automationPreview, EVERY_LABEL, nameList, sendDuration } from "@/lib/automation";
import type { AutomationRule, AutomationRuleInput, AutomationRunResult } from "@/lib/ipc";
import { navigate } from "@/lib/router";
import { uiAutomation, useStore } from "@/lib/store";
import type { Mailbox, SettingsPatch } from "@/lib/types";
import { errorMessage, plural, relativeDays } from "@/lib/utils";
import { Dot } from "./common";
import { confirm } from "./confirm";
import { Row, Section } from "./settings-ui";
import { Button } from "./ui/button";
import { Checkbox } from "./ui/checkbox";
import { Progress } from "./ui/progress";
import { Select } from "./ui/select";
import { Switch } from "./ui/switch";

const DEFAULT_RULE = { every_days: 7, hour: 9, rescan: true, send_erasure_for_queue: false, send_reminders: false, prepare_complaints: false };
const hourLabel = (h: number) => `${String(h).padStart(2, "0")}:00 Uhr`;

const ACTIONS: { key: "rescan" | "send_erasure_for_queue" | "send_reminders" | "prepare_complaints"; label: string; hint: string }[] = [
  { key: "rescan", label: "Neu scannen", hint: "Sucht neue Accounts und Newsletter im Postfach." },
  { key: "send_erasure_for_queue", label: "Löschanträge für die Warteschlange senden", hint: "Sendet Anträge nur für Accounts, bei denen du Automatisch löschen lassen gewählt hast." },
  { key: "send_reminders", label: "Erinnerungen senden", hint: "Erinnert Anbieter, deren Frist abgelaufen ist und die noch nicht geantwortet haben." },
  { key: "prepare_complaints", label: "Beschwerden vorbereiten", hint: "Legt Entwürfe an. Eine Beschwerde wird nie ohne dich gesendet." },
];

function summary(r: AutomationRule | undefined): string {
  if (!r || !r.enabled) return "Aus";
  return `${EVERY_LABEL[r.every_days] ?? `Alle ${r.every_days} Tage`} um ${hourLabel(r.hour)}`;
}

export function AutomationSection() {
  const snap = useStore((s) => s.snap)!;
  const rules = snap.automations ?? [];
  return (
    <Section
      id="automation" title="Automatik"
      intro="Die Automatik arbeitet nur, solange ausgeloggt läuft, auch im Hintergrund im Infobereich. Wenn du das Programm ganz beendest, pausiert sie."
    >
      {snap.mailboxes.length === 0 ? (
        <div className="flex items-center justify-between gap-4 py-4">
          <p className="text-base text-fg-muted">Verbinde zuerst ein Postfach. Die Automatik arbeitet pro Postfach.</p>
          <Button onClick={() => navigate({ page: "sources" })}>Postfach verbinden</Button>
        </div>
      ) : snap.mailboxes.map((m) => <MailboxRule key={m.id} m={m} rule={rules.find((r) => r.mailbox_id === m.id)} />)}
    </Section>
  );
}

function MailboxRule({ m, rule }: { m: Mailbox; rule: AutomationRule | undefined }) {
  const snap = useStore((s) => s.snap)!;
  const [running, setRunning] = useState<null | { message: string; done: number; total: number; jobId: string | null }>(null);
  const [outcome, setOutcome] = useState<{ ok: boolean; text: string } | null>(null);

  const input = (patch: Partial<AutomationRuleInput> = {}): AutomationRuleInput => ({
    ...DEFAULT_RULE, ...(rule ? { id: rule.id, every_days: rule.every_days, hour: rule.hour, rescan: rule.rescan, send_erasure_for_queue: rule.send_erasure_for_queue, send_reminders: rule.send_reminders, prepare_complaints: rule.prepare_complaints } : {}),
    mailbox_id: m.id, enabled: rule?.enabled ?? false, ...patch,
  });
  const save = (patch: Partial<AutomationRuleInput>) => api.automations.upsert(input(patch)).catch((e) => toast.error(errorMessage(e)));

  const preview = automationPreview(snap, m);

  async function runNow() {
    if (!rule) return;
    const lines: string[] = [];
    let mails = 0;
    if (rule.rescan) lines.push("Das Postfach wird neu gescannt.");
    if (rule.send_erasure_for_queue) {
      const q = preview.queue;
      mails += q.length;
      lines.push(q.length ? `${plural(q.length, "Löschantrag wird", "Löschanträge werden")} gesendet an ${nameList(q.map((a) => a.name))}.` : "Es steht kein Account in der Warteschlange.");
    }
    if (rule.send_reminders) {
      const r = preview.reminders;
      mails += r.length;
      lines.push(r.length ? `${plural(r.length, "Erinnerung wird", "Erinnerungen werden")} gesendet an ${nameList(r.map((x) => x.target))}.` : "Keine Frist ist überfällig, es gehen keine Erinnerungen raus.");
    }
    if (rule.prepare_complaints && preview.complaints.length) lines.push(`${plural(preview.complaints.length, "Beschwerde wird", "Beschwerden werden")} nur als Entwurf angelegt, nichts davon wird gesendet.`);
    if (lines.length === 0) { toast("Für diese Regel ist keine Aktion gewählt."); return; }
    const ok = await confirm({
      title: `Automatik für ${m.address} jetzt ausführen?`,
      text: (
        <>
          {lines.map((l) => <span key={l} className="mb-1.5 block">{l}</span>)}
          {mails > 0 && <span className="mt-3 block text-fg">Das sind echte Mails von {m.address}. Sie dauern {sendDuration(mails, snap.settings.send_gap_seconds ?? 20)}.</span>}
        </>
      ),
      confirm: mails > 0 ? `${plural(mails, "Mail", "Mails")} senden` : "Ausführen",
    });
    if (!ok) return;
    setOutcome(null); setRunning({ message: "Wird gestartet", done: 0, total: 0, jobId: null });
    uiAutomation.running++;
    try {
      const d = await runJob<AutomationRunResult>(() => api.automations.runNow(rule.id), {
        onStarted: (id) => setRunning((r) => r && { ...r, jobId: id }),
        onProgress: (p) => setRunning((r) => r && { ...r, message: p.message, done: p.done, total: p.total }),
      });
      const text = d.summary === "Abgebrochen" ? "Abgebrochen" : d.error?.message && !d.payload ? errorMessage(d.error) : d.summary;
      setOutcome({ ok: d.ok, text });
    } catch (e) { setOutcome({ ok: false, text: errorMessage(e) }); } finally { uiAutomation.running--; setRunning(null); }
  }

  async function remove() {
    if (!rule) return;
    if (await confirm({ title: "Regel entfernen?", text: "Die Automatik für dieses Postfach wird gelöscht. Bereits gesendete Anträge bleiben.", confirm: "Entfernen", destructive: true })) {
      await api.automations.remove(rule.id).catch((e) => toast.error(errorMessage(e)));
    }
  }

  const on = !!rule?.enabled;
  return (
    <div className="border-b border-border py-4 last:border-b-0">
      <div className="flex items-center gap-3">
        <div className="min-w-0 flex-1">
          <div className="truncate text-base font-medium">{m.address}</div>
          <div className="text-sm text-fg-muted">{summary(rule)}</div>
        </div>
        <Switch checked={on} onCheckedChange={(v) => void save({ enabled: v })} aria-label={`Automatik für ${m.address}`} />
      </div>

      {rule && on && (
        <div className="anim-fade mt-4 flex flex-col gap-4">
          <div className="flex flex-wrap items-center gap-x-6 gap-y-3">
            <label className="flex items-center gap-3 text-base text-fg-muted">
              Wie oft
              <Select className="w-[170px]" value={String(rule.every_days)} onChange={(e) => void save({ every_days: Number(e.target.value) })}>
                {Object.entries(EVERY_LABEL).map(([d, l]) => <option key={d} value={d}>{l}</option>)}
              </Select>
            </label>
            <label className="flex items-center gap-3 text-base text-fg-muted">
              Uhrzeit
              <Select className="w-[130px]" value={String(rule.hour)} onChange={(e) => void save({ hour: Number(e.target.value) })}>
                {Array.from({ length: 24 }, (_, h) => <option key={h} value={h}>{hourLabel(h)}</option>)}
              </Select>
            </label>
          </div>

          <ul className="flex flex-col gap-3">
            {ACTIONS.map((a) => (
              <li key={a.key}>
                <label className="flex cursor-default items-start gap-3">
                  <Checkbox className="mt-0.5" checked={rule[a.key]} onCheckedChange={(v) => void save({ [a.key]: v === true })} />
                  <span>
                    <span className="block text-base">{a.label}</span>
                    <span className="block text-sm text-fg-muted">{a.hint}</span>
                  </span>
                </label>
              </li>
            ))}
          </ul>

          <div className="flex flex-wrap items-center gap-3">
            {running ? (
              <div className="flex min-w-[260px] flex-1 flex-col gap-1.5">
                <Progress value={running.total ? running.done / running.total : undefined} />
                <div className="flex items-center gap-3 text-sm text-fg-muted">
                  <Loader2 className="size-3 animate-spin" /><span className="flex-1 truncate">{running.message}</span>
                  <button className="text-fg underline underline-offset-2" onClick={() => running.jobId && void api.jobs.cancel(running.jobId)}>Abbrechen</button>
                </div>
              </div>
            ) : (
              <>
                <Button onClick={() => void runNow()}><Play /> Jetzt ausführen</Button>
                <span className="min-w-0 flex-1 text-sm text-fg-muted">
                  {rule.last_run ? <>Zuletzt {relativeDays(rule.last_run)}{rule.last_result ? `: ${rule.last_result}` : ""}</> : "Noch nicht gelaufen"}
                </span>
                <Button variant="ghost" onClick={() => void remove()}>Regel entfernen</Button>
              </>
            )}
          </div>
          {outcome && (
            <p className="anim-fade flex items-start gap-2 text-base">
              <Dot className={`mt-[7px] ${outcome.ok ? "bg-s-green" : "bg-s-red"}`} />
              <span>{outcome.text}</span>
            </p>
          )}
        </div>
      )}
      {rule && !on && rule.last_run && <p className="mt-1 text-sm text-fg-faint">Zuletzt {relativeDays(rule.last_run)}{rule.last_result ? `: ${rule.last_result}` : ""}</p>}
    </div>
  );
}

export function BackgroundSection() {
  const snap = useStore((s) => s.snap)!;
  const st = snap.settings;
  const anyOn = (snap.automations ?? []).some((r) => r.enabled);
  const save = (patch: SettingsPatch) => api.settings.update(patch).catch((e) => toast.error(errorMessage(e)));

  async function keepUnlocked(v: boolean) {
    if (v && !(await confirm({
      title: "Tresor entsperrt lassen?",
      text: "Solange eine Automatik aktiv ist, sperrt sich der Tresor nicht mehr von selbst. Deine Daten bleiben dann im Arbeitsspeicher lesbar, auch wenn du nicht am Computer sitzt. Schalte das nur ein, wenn dein Gerät gesperrt ist und nur du Zugriff darauf hast.",
      confirm: "Entsperrt lassen",
    }))) return;
    void save({ keep_unlocked_for_automation: v });
  }
  async function autostart(v: boolean) {
    try { await api.automations.autostartSet(v); } catch (e) { toast.error(errorMessage(e)); }
  }

  return (
    <Section id="background" title="Hintergrund und Start">
      <Row label="Beim Schließen im Infobereich bleiben" hint={anyOn ? "Eine Automatik ist aktiv, deshalb bleibt ausgeloggt beim Schließen ohnehin im Infobereich." : "Sonst beendet das Schließen des Fensters das Programm."}>
        <Switch checked={!!st.start_minimized_to_tray || anyOn} disabled={anyOn} onCheckedChange={(v) => void save({ start_minimized_to_tray: v })} />
      </Row>
      <Row label="Mit dem Computer starten" hint="Startet ausgeloggt beim Anmelden minimiert im Infobereich.">
        <Switch checked={!!st.autostart} onCheckedChange={(v) => void autostart(v)} />
      </Row>
      <Row
        label="Tresor für die Automatik entsperrt lassen"
        hint={st.keep_unlocked_for_automation
          ? "Achtung: Der Tresor sperrt sich nicht mehr automatisch, solange eine Automatik aktiv ist. Deine Daten bleiben im Arbeitsspeicher lesbar."
          : "Ist der Tresor gesperrt, pausiert die Automatik bis zum nächsten Entsperren."}
      >
        <Switch checked={!!st.keep_unlocked_for_automation} onCheckedChange={(v) => void keepUnlocked(v)} />
      </Row>
    </Section>
  );
}
