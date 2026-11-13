// In memory demo backend: implements the whole Backend surface (see ../api.ts) with fake data.
// Query switches: ?empty (empty vault), ?big=5000 (huge lists), ?noidentity (no sender data),
// ?ai=ollama (AI enabled), ?fail=insights (one command fails, for error states).
import { IpcError } from "../ipc";
import type {
  AutomationRule, BulkSendResult, ManualLinks, MailboxDto, SendCheck, SettingsDto, UnsubscribeFailureReason,
} from "../ipc";
import type { Backend } from "../api";
import type {
  Account, DataRequest, JobDone, JobId, JobKind, JobProgress, LetterArgs, RequestKind, Snapshot,
  Subscription,
} from "../types";
import { automationPreview } from "../automation";
import { buildAccounts, buildRuns, buildSubscriptions, MAILBOXES } from "./data";
import {
  buildRequests, catalogInfo, computeInsights, enrichAccounts, makeLetter, PRESETS, REFERENCE, withPostUnsubscribeMails,
} from "./extras";

const DAY = 86_400_000;
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const ago = (d: number) => new Date(Date.now() - d * DAY).toISOString();
const inDays = (d: number) => new Date(Date.now() + d * DAY).toISOString();
const newId = () => crypto.randomUUID();

type Listener = (payload: never) => void;

function bigAccounts(base: Account[], n: number): Account[] {
  const out = [...base];
  const statuses: Account["status"][] = ["Discovered", "Discovered", "Confirmed", "Keep", "DeletionPending", "Deleted"];
  for (let i = 0; out.length < n; i++) {
    const b = base[i % base.length]!;
    const k = out.length;
    out.push({
      ...b, id: `x${k.toString(16).padStart(7, "0")}-0000-4000-8000-${(k * 31).toString(16).padStart(12, "0")}`,
      name: `${b.name} ${Math.floor(k / base.length) + 1}`, domain: `s${k}.${b.domain}`, status: statuses[k % statuses.length]!,
      confidence: 0.4 + ((k * 37) % 60) / 100,
    });
  }
  return out;
}
function bigSubs(base: Subscription[], n: number): Subscription[] {
  const out = [...base];
  for (let i = 0; out.length < n; i++) {
    const b = base[i % base.length]!;
    const k = out.length;
    out.push({ ...b, id: `y${k.toString(16).padStart(7, "0")}-0000-4000-8000-${(k * 17).toString(16).padStart(12, "0")}`, name: `${b.name} ${k}`, domain: `s${k}.${b.domain}`, frequency_per_month: 1 + ((k * 13) % 30) });
  }
  return out;
}

export function createMockBackend(): Backend {
  const q = new URLSearchParams(location.search);
  const empty = q.has("empty");
  const big = Number(q.get("big") ?? 0);
  const fail = q.get("fail");
  let exists = !q.has("create");
  let unlocked = false;
  let revision = 0;
  let password = "demo";
  const hasHibp = { v: false };

  const settings: SettingsDto = {
    auto_lock_minutes: 10, min_confidence: 0.5, full_name: empty ? "" : "Lena Hartmann", letter_language: "de",
    allow_catalog_updates: true, dark_mode: true, fetch_logos: false,
    ai: { provider: q.get("ai") === "ollama" ? "Ollama" : "Off", ollama_url: "http://127.0.0.1:11434", ollama_model: q.get("ai") === "ollama" ? "llama3.1:8b" : "", claude_model: "", codex_model: "" },
    identity: q.has("noidentity") || empty
      ? { street: "", postal_code: "", city: "", country: "Deutschland", state: "", birth_date: "", previous_addresses: [], phone: "", email: "" }
      : { street: "Lindenallee 12", postal_code: "80331", city: "München", country: "Deutschland", state: "DE-BY", birth_date: "1989-04-17", previous_addresses: ["Gartenstraße 3, 10115 Berlin"], phone: "", email: "lena.hartmann@gmail.com" },
    check_pwned_passwords: true, read_receipt_bodies: false, auto_rescan_days: 7, notifications: true, has_hibp_key: false,
    send_gap_seconds: 20, daily_send_cap: 0, start_minimized_to_tray: false, autostart: false, keep_unlocked_for_automation: false,
    onboarding_done: false,
  };

  const discover = q.has("discover");
  const baseAccounts = enrichAccounts(buildAccounts()).map((a) => (discover ? { ...a, status: "Discovered" as const } : a));
  const baseSubs = withPostUnsubscribeMails(buildSubscriptions()).map((x, i) => (i % 2 === 0 ? { ...x, last_message_id: `<last-${i}@${x.domain}>` } : x));
  const mailboxes: MailboxDto[] = empty ? [] : MAILBOXES.map((address, i) => ({
    id: `m${i}`, address, imap_host: i ? "imap.web.de" : "imap.gmail.com", imap_port: 993, username: address,
    folders: ["INBOX"], progress: [{ folder: "INBOX", uid_validity: 1, last_uid: 18420 }], last_scan: ago(2), has_password: true,
  }));
  const accounts = empty ? [] : big > 0 ? bigAccounts(baseAccounts, big) : baseAccounts;
  const subs = empty ? [] : big > 0 ? bigSubs(baseSubs, big) : baseSubs;
  const automationsInit: AutomationRule[] = empty || q.has("noauto") ? [] : [{
    id: "auto0", mailbox_id: "m0", enabled: true, every_days: 7, hour: 9, rescan: true, send_erasure_for_queue: true,
    send_reminders: true, prepare_complaints: true, last_run: ago(3), last_result: "Postfach geprüft, 1 Löschantrag gesendet, 2 Erinnerungen gesendet",
  }];
  let snap: Snapshot = {
    schema: 1, accounts, runs: empty ? [] : buildRuns(), mailboxes, settings, subscriptions: subs,
    requests: empty ? [] : buildRequests(accounts, subs).map((r) => (r.channel === "Email" && r.status === "Sent" ? { ...r, mailbox_id: "m0" } : r)), automations: automationsInit,
  };
  // Mails sent today per mailbox (rate limit state) and the time of the last one (gap).
  const sentToday: Record<string, number> = { m0: 3, m1: 0 };
  let lastSentAt = 0;
  const capOf = (id: string) => {
    const cap = snap.settings.daily_send_cap ?? 0;
    if (cap > 0) return cap;
    return snap.mailboxes.find((m) => m.id === id)?.address.endsWith("@gmail.com") ? 100 : 40;
  };
  const remaining = (id: string) => Math.max(0, capOf(id) - (sentToday[id] ?? 0));
  const gap = () => snap.settings.send_gap_seconds ?? 20;
  const LETTER_REQUEST: Record<string, RequestKind> = {
    Erasure: "Erasure", Access: "Access", ObjectionAndAccess: "Objection", Cancellation: "Cancellation", Reminder: "Reminder", Complaint: "Complaint",
    CeaseAndDesist: "CeaseAndDesist", CreditBureauAccess: "CreditBureauAccess", AddressBrokerObjection: "AdvertisingObjection",
  };
  const BULK_KINDS = ["Erasure", "Access", "ObjectionAndAccess", "Cancellation"];

  /** Why a letter cannot be sent from this mailbox, or the letter. */
  function prepareSend(a: LetterArgs, mailboxId: string): { ok: true; target: string; to: string; subject: string; body: string; deadline: string | null } | { ok: false; reason: string } {
    const mb = snap.mailboxes.find((m) => m.id === mailboxId);
    if (!mb) return { ok: false, reason: "Postfach nicht gefunden" };
    if (!mb.has_password) return { ok: false, reason: "Für dieses Postfach ist kein Passwort gespeichert" };
    const p = makeLetter(snap, a);
    if (p.missing_sender_fields.length) return { ok: false, reason: `Deine Angaben fehlen: ${p.missing_sender_fields.join(", ")}` };
    const to = p.recipient_email?.trim();
    if (!to) return { ok: false, reason: "Für diesen Empfänger ist keine Mailadresse bekannt. Bitte den Brief als PDF speichern und per Post senden" };
    if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(to)) return { ok: false, reason: "Die Mailadresse des Empfängers ist ungültig" };
    const acc = a.accountId ? snap.accounts.find((x) => x.id === a.accountId) : undefined;
    return { ok: true, target: acc?.name ?? p.recipient.name, to, subject: p.subject, body: p.text, deadline: p.deadline };
  }

  // ---- events ----
  const listeners: Record<string, Set<Listener>> = {};
  const on = <T,>(ev: string, cb: (p: T) => void) => {
    (listeners[ev] ??= new Set()).add(cb as Listener);
    return Promise.resolve(() => { listeners[ev]!.delete(cb as Listener); });
  };
  const emit = (ev: string, payload: unknown) => {
    for (const l of [...(listeners[ev] ?? [])]) (l as (p: unknown) => void)(payload);
  };

  const needUnlocked = () => { if (!unlocked) throw new IpcError("VaultLocked", "Der Tresor ist gesperrt."); };
  const lat = async (ms = 70) => { await sleep(ms); needUnlocked(); };
  const commit = (fn: (s: Snapshot) => Snapshot) => {
    snap = fn(snap);
    revision++;
    setTimeout(() => emit("vault://changed", { revision }), 0);
  };
  const patchAccounts = (ids: string[], fn: (a: Account) => Account) => {
    const set = new Set(ids);
    commit((s) => ({ ...s, accounts: s.accounts.map((a) => (set.has(a.id) ? { ...fn(a), updated_at: new Date().toISOString() } : a)) }));
  };

  // ---- jobs ----
  interface Ctx { progress(done: number, total: number, message: string, item?: JobProgress["item"]): void; cancelled(): boolean }
  const running = new Map<JobId, { kind: JobKind; done: number; total: number; cancel: boolean }>();
  function startJob(kind: JobKind, work: (c: Ctx) => Promise<Omit<JobDone, "jobId" | "kind">>): JobId {
    needUnlocked();
    const group = (k: JobKind) => (k === "send" || k === "automation" ? "send" : k);
    if ([...running.values()].some((j) => group(j.kind) === group(kind))) throw new IpcError("Busy", "Dieser Vorgang läuft bereits.");
    const jobId = newId();
    const job = { kind, done: 0, total: 0, cancel: false };
    running.set(jobId, job);
    const ctx: Ctx = {
      progress(done, total, message, item) {
        job.done = done; job.total = total;
        emit("job://progress", { jobId, kind, done, total, message, item } satisfies JobProgress);
      },
      cancelled: () => job.cancel,
    };
    setTimeout(async () => {
      let res: Omit<JobDone, "jobId" | "kind">;
      try { res = await work(ctx); } catch (e) {
        const err = IpcError.from(e);
        res = { ok: false, summary: err.message, error: { code: err.code, message: err.message } };
      }
      if (job.cancel) res = { ok: false, summary: "Abgebrochen" };
      running.delete(jobId);
      emit("job://done", { jobId, kind, ...res } satisfies JobDone);
    }, 40);
    return jobId;
  }
  const steps = async (c: Ctx, n: number, ms: number, msg: (i: number) => string) => {
    for (let i = 1; i <= n; i++) {
      if (c.cancelled()) return false;
      c.progress(i, n, msg(i));
      await sleep(ms);
    }
    return !c.cancelled();
  };

  const pickRelay = () => `${Math.random().toString(36).slice(2, 12)}@privaterelay.appleid.com`;
  function addAccount(domain: string, name: string, identifier: string, kind: Account["evidence"][number]["kind"], source: Account["evidence"][number]["source"]): Account {
    const now = new Date().toISOString();
    return {
      id: newId(), domain, name, identifiers: identifier ? [identifier] : [], sso: [], status: "Discovered", confidence: 0.8,
      evidence: [{ kind, source, summary: `${name} gefunden`, date: now, source_ref: null }], first_seen: now, last_seen: now,
      tags: [], notes: "", deletion: [], created_at: now, updated_at: now, payments: [],
      security: { password_reuse_count: null, password_pwned: null, has_2fa: null, phone_login: false, breaches: [], checked_at: null },
    };
  }

  /** Dry run for a bulk send: the same checks as the backend, in the same order. */
  function planCheck(items: { kind: LetterArgs["kind"]; accountId: string; recipient?: LetterArgs["recipient"] }[], mailboxId: string): SendCheck {
    const ok: SendCheck["ok"] = [];
    const problems: SendCheck["problems"] = [];
    const seen = new Set<string>();
    for (const it of items) {
      const kind = LETTER_REQUEST[it.kind]!;
      let reason: string | null = null;
      if (!BULK_KINDS.includes(it.kind)) reason = "Diese Art von Brief ist für den Sammelversand nicht vorgesehen";
      else if (seen.has(`${it.accountId}:${kind}`)) reason = "Dieser Account ist in der Auswahl doppelt enthalten";
      else if (snap.requests.some((r) => r.account_id === it.accountId && r.kind === kind && r.status !== "Draft")) reason = "Für diesen Account gibt es bereits einen gesendeten Antrag";
      seen.add(`${it.accountId}:${kind}`);
      if (!reason) {
        const p = prepareSend({ kind: it.kind, accountId: it.accountId, recipient: it.recipient }, mailboxId);
        if (p.ok) ok.push({ accountId: it.accountId, name: p.target, recipientEmail: p.to, subject: p.subject });
        else reason = p.reason;
      }
      if (reason) problems.push({ accountId: it.accountId, reason });
    }
    return { ok, problems, remainingToday: remaining(mailboxId), dailyCap: capOf(mailboxId), gapSeconds: gap() };
  }

  /** Send one letter: gap, SMTP, counters, request record. Throws the German reason on failure. */
  async function sendOne(c: Ctx, args: LetterArgs, mailboxId: string, attachPdf: boolean, index: number, total: number, label = "") {
    if (remaining(mailboxId) <= 0) throw new IpcError("Invalid", `Das Tageslimit von ${capOf(mailboxId)} Mails für dieses Postfach ist erreicht. Morgen geht es weiter`);
    const p = prepareSend(args, mailboxId);
    if (!p.ok) throw new IpcError("Invalid", p.reason);
    const wait = lastSentAt ? Math.max(0, gap() * 1000 - (Date.now() - lastSentAt)) : 0;
    if (wait > 0) {
      c.progress(index, total, `Wartet ${Math.ceil(wait / 1000)} Sekunden wegen des Sendelimits`);
      await sleep(Math.min(wait, 500));
    }
    if (c.cancelled()) throw new IpcError("Internal", "Abgebrochen");
    c.progress(index, total, label ? `Mail an ${label} wird gesendet` : "Mail wird gesendet");
    await sleep(label ? 450 : 800);
    lastSentAt = Date.now();
    sentToday[mailboxId] = (sentToday[mailboxId] ?? 0) + 1;
    const kind = LETTER_REQUEST[args.kind]!;
    const month: RequestKind[] = ["Erasure", "Access", "Objection", "CreditBureauAccess", "AdvertisingObjection"];
    const short: RequestKind[] = ["CeaseAndDesist", "Reminder", "Cancellation"];
    const acc = args.accountId ? snap.accounts.find((x) => x.id === args.accountId) : undefined;
    const parent = args.requestId ? snap.requests.find((x) => x.id === args.requestId) : undefined;
    const req: DataRequest = {
      id: newId(), kind, target: p.target, domain: acc?.domain ?? parent?.domain ?? null, account_id: args.accountId ?? parent?.account_id ?? null,
      subscription_id: args.subscriptionId ?? null, parent_id: kind === "Reminder" || kind === "Complaint" ? args.requestId ?? null : null,
      channel: "Email", recipient: p.to, subject: p.subject, body: p.body, created_at: new Date().toISOString(), sent_at: new Date().toISOString(),
      deadline: month.includes(kind) ? inDays(30) : short.includes(kind) ? inDays(14) : null, status: "Sent", notes: "",
      message_id: `<${newId()}@ausgeloggt.local>`, mailbox_id: mailboxId, attachment: attachPdf ? `${args.kind}_${p.target}.pdf` : null,
    };
    commit((s) => ({
      ...s, requests: [req, ...s.requests],
      accounts: s.accounts.map((a) => (a.id === args.accountId && kind === "Erasure" ? { ...a, queued_for_deletion: false } : a)),
    }));
    return { requestId: req.id, messageId: req.message_id!, sentAt: req.sent_at! };
  }

  const api: Backend = {
    kind: "mock",
    vault: {
      async status() {
        await sleep(30);
        return { exists, unlocked, path: "C:\\Users\\Lena\\AppData\\Roaming\\ausgeloggt\\tresor.ausgeloggt" };
      },
      async create(pw) {
        await sleep(600);
        if (pw.length < 10) throw new IpcError("Invalid", "Das Passwort braucht mindestens 10 Zeichen.");
        password = pw; exists = true; unlocked = true;
        snap = { ...snap, accounts: [], runs: [], mailboxes: [], subscriptions: [], requests: [] };
      },
      async unlock(pw) {
        await sleep(500);
        if (!pw || (password !== "demo" && pw !== password) || pw === "falsch") throw new IpcError("WrongPassword", "Das Passwort stimmt nicht.");
        unlocked = true;
      },
      async lock() {
        unlocked = false;
        for (const j of running.values()) j.cancel = true;
        setTimeout(() => emit("vault://locked", {}), 0);
      },
      async touch() { /* activity ping */ },
      async changePassword(cur, next) {
        await sleep(500);
        needUnlocked();
        if (cur !== password && !(password === "demo")) throw new IpcError("WrongPassword", "Das aktuelle Passwort stimmt nicht.");
        if (next.length < 10) throw new IpcError("Invalid", "Das neue Passwort braucht mindestens 10 Zeichen.");
        password = next;
      },
      async exportJson() { await lat(300); },
    },
    data: {
      async snapshot() { await lat(); return snap; },
      async insights() {
        await lat();
        if (fail === "insights") throw new IpcError("Internal", "Die Auswertung ist fehlgeschlagen.");
        return computeInsights(snap);
      },
      async catalogInfo(qq) { await lat(40); return catalogInfo(qq.domain, qq.name); },
      async referenceData() { await lat(40); return REFERENCE; },
      async logo() { return null; },
    },
    accounts: {
      async setStatus(ids, status) { await lat(40); patchAccounts(ids, (a) => ({ ...a, status })); },
      async setNotes(id, notes) { await lat(20); patchAccounts([id], (a) => ({ ...a, notes })); },
      async add(domain, name, identifier) {
        await lat(40);
        const a = addAccount(domain, name || domain, identifier ?? "", "Manual", "Manual");
        commit((s) => ({ ...s, accounts: [a, ...s.accounts] }));
        return a.id;
      },
      async remove(ids) { await lat(40); const set = new Set(ids); commit((s) => ({ ...s, accounts: s.accounts.filter((a) => !set.has(a.id)) })); },
      async addDeletionStep(id, method, note) {
        await lat(40);
        const gdpr = typeof method !== "string" && "GdprEmail" in method;
        patchAccounts([id], (a) => ({
          ...a, status: method === "Confirmed" ? "Deleted" : "DeletionPending",
          deletion: [...a.deletion, { at: new Date().toISOString(), method, note, deadline: gdpr ? inDays(30) : null }],
        }));
      },
    },
    mail: {
      async presets() { await lat(20); return PRESETS; },
      async add(m) {
        await lat(60);
        const id = newId();
        commit((s) => ({ ...s, mailboxes: [...s.mailboxes, { id, address: m.address, imap_host: m.imapHost, imap_port: m.imapPort, username: m.username, folders: m.folders, progress: [], last_scan: null, has_password: !!m.password }] }));
        return id;
      },
      async update(id, patch) {
        await lat(40);
        commit((s) => ({
          ...s, mailboxes: s.mailboxes.map((m) => m.id !== id ? m : {
            ...m, address: patch.address ?? m.address, imap_host: patch.imapHost ?? m.imap_host, imap_port: patch.imapPort ?? m.imap_port,
            username: patch.username ?? m.username, folders: patch.folders ?? m.folders, has_password: m.has_password || !!patch.password,
          }),
        }));
      },
      async remove(id) { await lat(40); commit((s) => ({ ...s, mailboxes: s.mailboxes.filter((m) => m.id !== id) })); },
      async testSaved() { await lat(700); return ["INBOX", "Archiv", "Gesendet", "Spam"]; },
      async testDraft(d) {
        await lat(900);
        if (/invalid|example\./.test(d.imapHost) || !d.imapHost.includes(".")) throw new IpcError("Network", `Der Server ${d.imapHost || "(leer)"} ist nicht erreichbar. Prüfe den Servernamen und deine Internetverbindung.`);
        if (!d.password || d.password === "falsch") throw new IpcError("Auth", "Anmeldung abgelehnt. Prüfe Benutzername und Passwort. Bei Gmail und iCloud wird ein App Passwort verlangt.");
        return ["INBOX", "Archiv", "Gesendet", "Spam"];
      },
      async scanStart(ids, full) {
        return startJob("scan", async (c) => {
          const targets = snap.mailboxes.filter((m) => ids.includes(m.id));
          const total = 10 * targets.length;
          let done = 0;
          for (const m of targets) {
            for (let i = 1; i <= 10; i++) {
              if (c.cancelled()) return { ok: false, summary: "Abgebrochen" };
              done++;
              c.progress(done, total, `${m.address}: INBOX, ${(i * (full ? 2100 : 640)).toLocaleString("de-DE")} Mails geprüft`);
              await sleep(380);
            }
          }
          const ids2 = new Set(targets.map((m) => m.id));
          const newAcc = addAccount("wolt.com", "Wolt", targets[0]?.address ?? "", "Receipt", { Mail: { mailbox: targets[0]?.address ?? "" } });
          commit((s) => ({
            ...s,
            accounts: s.accounts.some((a) => a.domain === "wolt.com") ? s.accounts : [newAcc, ...s.accounts],
            mailboxes: s.mailboxes.map((m) => (ids2.has(m.id) ? { ...m, last_scan: new Date().toISOString() } : m)),
            subscriptions: s.subscriptions.map((x) => (targets.some((m) => m.address === x.mailbox) && !x.last_message_id ? { ...x, last_message_id: `<last-${x.id}@${x.domain}>` } : x)),
            runs: [...targets.map((m) => ({ id: newId(), source: { Mail: { mailbox: m.address } }, started_at: ago(0.01), finished_at: new Date().toISOString(), items_scanned: 3120, findings: 61, new_accounts: 1, error: null })), ...s.runs],
          }));
          return {
            ok: true, summary: `${targets.length} ${targets.length === 1 ? "Postfach" : "Postfächer"} geprüft, 1 neuer Account`,
            payload: { mailboxes: targets.length, failed: 0, mails: 3120 * targets.length, findings: 61, newAccounts: 1, newSubscriptions: 2, payments: 4 },
          };
        });
      },
    },
    imports: {
      async detect(path) {
        await lat(80);
        const p = path.toLowerCase();
        if (p.includes("takeout") || p.endsWith(".zip")) return { kind: "google_takeout", label_de: "Google Takeout" };
        if (p.includes("facebook")) return { kind: "facebook_export", label_de: "Facebook Export" };
        if (p.includes("chrome") || p.includes("passw")) return { kind: "passwords", label_de: "Passwortexport von Chrome", format: "Chrome" };
        if (p.includes("bitwarden")) return { kind: "passwords", label_de: "Passwortexport von Bitwarden", format: "Bitwarden" };
        return { kind: "generic_list", label_de: "Liste mit Webseiten" };
      },
      async start(paths, checkPwned) {
        return startJob("import", async (c) => {
          if (!(await steps(c, 6, 260, (i) => `${paths.length > 1 ? `Datei ${Math.min(i, paths.length)} von ${paths.length}, ` : ""}${i * 31} Einträge gelesen`))) return { ok: false, summary: "Abgebrochen" };
          const fresh = [["kleinanzeigen.de", "Kleinanzeigen"], ["check24.de", "Check24"], ["bitpanda.com", "Bitpanda"]] as const;
          const list = fresh.filter(([d]) => !snap.accounts.some((a) => a.domain === d)).map(([d, n], i) => {
            const a = addAccount(d, n, "lena.hartmann@gmail.com", "SavedPassword", { PasswordExport: { format: "Chrome" } });
            if (checkPwned) a.security = { ...a.security, password_pwned: i === 2, password_reuse_count: 0, checked_at: new Date().toISOString() };
            return a;
          });
          commit((s) => ({ ...s, accounts: [...list, ...s.accounts], runs: [{ id: newId(), source: { PasswordExport: { format: "Chrome" } }, started_at: ago(0.01), finished_at: new Date().toISOString(), items_scanned: 186, findings: 186, new_accounts: list.length, error: null }, ...s.runs] }));
          return {
            ok: true, summary: `${list.length} neue Accounts aus ${paths.length} ${paths.length === 1 ? "Datei" : "Dateien"}`,
            payload: { files: paths.length, imported: 186, findings: 186, newAccounts: list.length, warnings: paths.length > 2 ? ["Eine Zeile ohne Adresse übersprungen."] : [] },
          };
        });
      },
      async screenshotRead() {
        return startJob("screenshot_read", async (c) => {
          if (!(await steps(c, 4, 450, (i) => `Bild ${Math.min(i, 2)} wird gelesen`))) return { ok: false, summary: "Abgebrochen" };
          return {
            ok: true, summary: "5 Einträge erkannt",
            payload: { provider: "Apple", entries: [
              { name: "Letterboxd", relay_address: pickRelay() }, { name: "Komoot", domain: "komoot.de" }, { name: "Too Good To Go", domain: null },
              { name: "Flink", domain: "goflink.com" }, { name: "Vivino", relay_address: pickRelay() },
            ] },
          };
        });
      },
      async screenshotCommit(provider, entries) {
        await lat(200);
        const list = entries.map((e) => {
          const a = addAccount(e.domain ?? `unresolved:${e.name.toLowerCase().replace(/\s+/g, "")}`, e.name, e.relay_address ?? "", "Screenshot", { Screenshot: { provider } });
          if (provider) a.sso = [provider];
          return a;
        });
        commit((s) => ({ ...s, accounts: [...list, ...s.accounts] }));
        return { new_accounts: list.length, updated: 0 };
      },
    },
    newsletters: {
      async setStatus(ids, status) {
        await lat(30);
        const set = new Set(ids);
        commit((s) => ({ ...s, subscriptions: s.subscriptions.map((x) => (set.has(x.id) ? { ...x, status, history: status === "Unsubscribed" ? [...x.history, { at: new Date().toISOString(), method: x.unsubscribe[0] ?? { Link: { url: `https://${x.domain}` } }, ok: true, message: "Manuell abgemeldet" }] : x.history } : x)) }));
      },
      async setCategory(ids, category) {
        await lat(30);
        const set = new Set(ids);
        commit((s) => ({ ...s, subscriptions: s.subscriptions.map((x) => (set.has(x.id) ? { ...x, category, category_source: "User" } : x)) }));
      },
      async unsubscribeStart(ids) {
        return startJob("unsubscribe", async (c) => {
          const items = snap.subscriptions.filter((s) => ids.includes(s.id));
          const results: { id: string; ok: boolean; skipped: boolean; message: string; reason: UnsubscribeFailureReason | null }[] = [];
          let n = 0;
          for (const s of items) {
            if (c.cancelled()) break;
            n++;
            const auto = s.unsubscribe.some((m) => !("Link" in m)) && s.category !== "Transactional";
            const failed = auto && n % 7 === 0;
            const why: UnsubscribeFailureReason = (["Http", "Network", "Refused"] as const)[Math.floor(n / 7) % 3]!;
            const r = {
              id: s.id, ok: auto && !failed, skipped: !auto,
              message: !auto ? "Nur Link, bitte selbst öffnen" : failed ? (why === "Http" ? "Server antwortet mit Fehler 503" : why === "Network" ? "Server nicht erreichbar" : "Abmeldung abgelehnt") : "Abgemeldet",
              reason: !auto ? ("NoAutomaticMethod" as const) : failed ? why : null,
            };
            results.push(r);
            c.progress(n, items.length, s.name, { id: s.id, ok: r.ok, message: r.message, reason: r.reason } as JobProgress["item"]);
            await sleep(180);
          }
          const okIds = new Set(results.filter((r) => r.ok).map((r) => r.id));
          const failIds = new Set(results.filter((r) => !r.ok && !r.skipped).map((r) => r.id));
          commit((st) => ({ ...st, subscriptions: st.subscriptions.map((x) => (okIds.has(x.id) ? { ...x, status: "Unsubscribed" as const } : failIds.has(x.id) ? { ...x, status: "Failed" as const } : x)) }));
          const ok = results.filter((r) => r.ok).length, failedN = results.filter((r) => !r.ok && !r.skipped).length, skipped = results.filter((r) => r.skipped).length;
          return { ok: failedN === 0, summary: `${ok} abgemeldet${failedN ? `, ${failedN} fehlgeschlagen` : ""}${skipped ? `, ${skipped} übersprungen` : ""}`, payload: { ok, failed: failedN, skipped, results } };
        });
      },
      async aiClassifyStart() {
        if (snap.settings.ai.provider === "Off") throw new IpcError("Invalid", "Die KI ist ausgeschaltet. Aktiviere sie in den Einstellungen.");
        return startJob("ai_classify", async (c) => {
          if (!(await steps(c, 5, 400, (i) => `${i * 6} von 30 Absendern eingeordnet`))) return { ok: false, summary: "Abgebrochen" };
          return { ok: true, summary: "30 geprüft, 4 geändert", payload: { checked: 30, changed: 4 } };
        });
      },
    },
    security: {
      async breachCheckStart() {
        return startJob("breach_check", async (c) => {
          if (!(await steps(c, 6, 380, (i) => i < 4 ? "Öffentliche Leckliste wird abgeglichen" : `Adresse ${i - 3} von 3 wird geprüft`))) return { ok: false, summary: "Abgebrochen" };
          const d = new Date().toISOString();
          commit((s) => ({
            ...s,
            accounts: s.accounts.map((a) => a.domain === "zoom.us" && !a.security.breaches.length
              ? { ...a, security: { ...a.security, checked_at: d, breaches: [{ name: "Zoom", title: "Zoom", breach_date: "2020-04-01", data_classes: ["Mailadressen", "Passwörter"], confirmed_for_user: true }] } }
              : a),
          }));
          return { ok: true, summary: "1 neues Datenleck gefunden", payload: { catalogue: 612, newBreaches: 1, addressesChecked: 3, confirmedNew: 1, hibpKey: hasHibp.v } };
        });
      },
    },
    letters: {
      async preview(a) { await lat(80); return makeLetter(snap, a); },
      async savePdf(a, path) {
        await lat(200);
        const p = makeLetter(snap, a);
        if (p.missing_sender_fields.length || p.missing_recipient) throw new IpcError("Invalid", "Absender oder Empfänger sind unvollständig.");
        if (!path) throw new IpcError("Invalid", "Kein Speicherort gewählt.");
      },
      async mailtoUrl(subject, body, to) { return `mailto:${to ?? ""}?subject=${encodeURIComponent(subject)}&body=${encodeURIComponent(body)}`; },
    },
    requests: {
      async create(r) {
        await lat(60);
        const month: RequestKind[] = ["Erasure", "Access", "Objection", "CreditBureauAccess", "AdvertisingObjection"];
        const short: RequestKind[] = ["CeaseAndDesist", "Reminder", "Cancellation"];
        const req: DataRequest = {
          id: newId(), kind: r.kind, target: r.target, domain: r.domain ?? null, account_id: r.accountId ?? null,
          subscription_id: r.subscriptionId ?? null, parent_id: r.parentId ?? null, channel: r.channel, recipient: r.recipient,
          subject: r.subject, body: r.body, created_at: new Date().toISOString(), sent_at: r.sent ? new Date().toISOString() : null,
          deadline: r.sent ? (month.includes(r.kind) ? inDays(30) : short.includes(r.kind) ? inDays(14) : null) : null,
          status: r.sent ? "Sent" : "Draft", notes: "",
        };
        commit((s) => ({ ...s, requests: [req, ...s.requests] }));
        return req.id;
      },
      async update(u) {
        await lat(40);
        commit((s) => ({ ...s, requests: s.requests.map((r) => (r.id !== u.id ? r : { ...r, status: u.status ?? r.status, notes: u.notes ?? r.notes, sent_at: u.sentAt ?? r.sent_at, deadline: u.sentAt ? inDays(30) : r.deadline })) }));
      },
      async remove(id) { await lat(40); commit((s) => ({ ...s, requests: s.requests.filter((r) => r.id !== id) })); },
    },
    settings: {
      async update(patch) {
        await lat(30);
        commit((s) => ({
          ...s,
          settings: {
            ...s.settings, ...patch,
            ai: { ...s.settings.ai, ...(patch.ai ?? {}) }, identity: { ...s.settings.identity, ...(patch.identity ?? {}) },
          } as SettingsDto,
        }));
      },
      async setSecret(kind, value) {
        await lat(30);
        if (kind === "hibp") hasHibp.v = !!value;
        commit((s) => ({ ...s, settings: { ...s.settings, has_hibp_key: hasHibp.v } }));
      },
      async aiTest() {
        await lat(900);
        if (snap.settings.ai.provider === "Off") throw new IpcError("Invalid", "Die KI ist ausgeschaltet.");
        const p = snap.settings.ai.provider;
        // ?cli=nologin simulates a Claude Code / Codex that is not signed in.
        if ((p === "ClaudeCode" || p === "Codex") && q.get("cli") === "nologin") {
          throw new IpcError("Network", p === "ClaudeCode"
            ? "Claude Code ist nicht angemeldet. Öffne ein Terminal, starte claude und melde dich mit deinem Claude Konto an."
            : "Codex ist nicht angemeldet. Öffne ein Terminal und führe codex login aus.");
        }
        if (p === "ClaudeCode") return "Claude Code ist mit deinem Claude Konto angemeldet";
        if (p === "Codex") return "Codex ist mit deinem ChatGPT Konto angemeldet";
        return `Verbunden, ${snap.settings.ai.ollama_model || "Modell"} antwortet.`;
      },
      async aiModels() {
        await lat(300);
        const p = snap.settings.ai.provider;
        if (p === "Ollama") return { text: ["llama3.1:8b", "qwen2.5:7b", "mistral:7b"], vision: ["llava:13b", "qwen2.5vl:7b"] };
        if (p === "ClaudeCode") return { text: ["haiku", "sonnet", "opus"], vision: ["haiku", "sonnet", "opus"] };
        return { text: [], vision: [] };
      },
      async catalogRefreshStart() {
        return startJob("catalog_refresh", async (c) => {
          if (!(await steps(c, 4, 350, () => "Katalog wird geladen"))) return { ok: false, summary: "Abgebrochen" };
          return { ok: true, summary: "4.821 Einträge aktualisiert", payload: { entries: 4821 } };
        });
      },
      async generatePdfKit() { await lat(500); },
    },
    sending: {
      async letterSend(a) {
        needUnlocked();
        return startJob("send", async (c) => {
          c.progress(0, 1, "Brief wird vorbereitet");
          await sleep(350);
          const args: LetterArgs = { kind: a.kind, accountId: a.accountId, subscriptionId: a.subscriptionId, requestId: a.requestId, orgId: a.orgId, authorityState: a.authorityState, recipient: a.recipient };
          const out = await sendOne(c, args, a.mailboxId, a.attachPdf, 0, 1);
          const gmail = snap.mailboxes.find((m) => m.id === a.mailboxId)?.address.endsWith("@gmail.com");
          return {
            ok: true, summary: "Mail gesendet. Eine Kopie liegt im Ordner Gesendet.",
            payload: { requestId: out.requestId, messageId: out.messageId, sentAt: out.sentAt, appendedTo: gmail ? null : "Gesendet", appendError: null },
          };
        });
      },
      async lettersSendCheck(items, mailboxId) {
        await lat(260);
        return planCheck(items, mailboxId);
      },
      async lettersSendBulk(items, mailboxId, attachPdf) {
        needUnlocked();
        return startJob("send", async (c) => {
          const plan = planCheck(items, mailboxId);
          const bad = new Map(plan.problems.map((p) => [p.accountId, p.reason]));
          const results: BulkSendResult["results"] = [];
          let sent = 0, failed = 0, skipped = 0, capReached = false;
          for (let i = 0; i < items.length; i++) {
            const it = items[i]!;
            const name = snap.accounts.find((x) => x.id === it.accountId)?.name ?? it.accountId;
            if (c.cancelled()) break;
            const why = bad.get(it.accountId);
            if (why) {
              skipped++;
              results.push({ id: it.accountId, ok: false, skipped: true, message: why, reason: "Skipped" });
              c.progress(i + 1, items.length, name, { id: it.accountId, ok: false, message: why });
              continue;
            }
            if (capReached || remaining(mailboxId) <= 0) {
              const first = !capReached;
              capReached = true; skipped++;
              results.push({ id: it.accountId, ok: false, skipped: true, message: "Tageslimit erreicht", reason: first ? "CapReached" : "Blocked" });
              continue;
            }
            try {
              await sendOne(c, { kind: it.kind, accountId: it.accountId, recipient: it.recipient }, mailboxId, attachPdf, i, items.length, name);
              sent++;
              results.push({ id: it.accountId, ok: true, skipped: false, message: "Gesendet", reason: null });
              c.progress(i + 1, items.length, name, { id: it.accountId, ok: true, message: "Gesendet" });
            } catch (e) {
              failed++;
              const m = IpcError.from(e).message;
              results.push({ id: it.accountId, ok: false, skipped: false, message: m, reason: "SendFailed" });
              c.progress(i + 1, items.length, name, { id: it.accountId, ok: false, message: m });
            }
          }
          const payload: BulkSendResult = { sent, failed, skipped, capReached, results };
          return { ok: failed === 0 && !capReached, summary: `${sent} gesendet${failed ? `, ${failed} fehlgeschlagen` : ""}${skipped ? `, ${skipped} übersprungen` : ""}`, payload };
        });
      },
      async accountSetQueued(ids, queued) { await lat(30); patchAccounts(ids, (a) => ({ ...a, queued_for_deletion: queued })); },
    },
    automations: {
      async list() { await lat(30); return snap.automations ?? []; },
      async upsert(rule) {
        await lat(60);
        const list = snap.automations ?? [];
        if (![1, 7, 14, 30].includes(rule.every_days)) throw new IpcError("Invalid", "Ungültiges Intervall");
        const other = list.find((r) => r.mailbox_id === rule.mailbox_id && r.id !== rule.id);
        if (other) throw new IpcError("Invalid", "Für dieses Postfach gibt es schon eine Regel");
        const id = rule.id ?? newId();
        const prev = list.find((r) => r.id === id);
        const next: AutomationRule = { ...rule, id, last_run: prev?.last_run ?? null, last_result: prev?.last_result ?? "" };
        commit((s) => ({ ...s, automations: prev ? (s.automations ?? []).map((r) => (r.id === id ? next : r)) : [...(s.automations ?? []), next] }));
        return id;
      },
      async remove(id) { await lat(40); commit((s) => ({ ...s, automations: (s.automations ?? []).filter((r) => r.id !== id) })); },
      async runNow(id) {
        needUnlocked();
        return startJob("automation", async (c) => {
          const rule = (snap.automations ?? []).find((r) => r.id === id);
          if (!rule) throw new IpcError("NotFound", "Regel nicht gefunden");
          const mb = snap.mailboxes.find((m) => m.id === rule.mailbox_id);
          if (!mb) throw new IpcError("NotFound", "Postfach nicht gefunden");
          const parts: string[] = [];
          let failed = 0;
          if (rule.rescan) {
            for (let i = 1; i <= 5; i++) { if (c.cancelled()) break; c.progress(i, 10, `${mb.address}: Postfach wird geprüft`); await sleep(300); }
            parts.push("Postfach geprüft");
          }
          if (rule.send_erasure_for_queue) {
            const mine = automationPreview(snap, mb).queue;
            let n = 0;
            for (const a of mine) {
              if (c.cancelled() || remaining(mb.id) <= 0) break;
              c.progress(5 + n, 10, `Löschantrag an ${a.name}`);
              try { await sendOne(c, { kind: "Erasure", accountId: a.id }, mb.id, true, n, mine.length, a.name); n++; } catch { failed++; }
            }
            if (n || failed) parts.push(`${n} ${n === 1 ? "Löschantrag" : "Löschanträge"} gesendet`);
          }
          if (rule.send_reminders) {
            const due = automationPreview(snap, mb).reminders;
            let n = 0;
            for (const r of due) {
              if (c.cancelled() || remaining(mb.id) <= 0) break;
              c.progress(8, 10, `Erinnerung an ${r.target}`);
              try { await sendOne(c, { kind: "Reminder", requestId: r.id }, mb.id, false, n, due.length, r.target); n++; } catch { failed++; }
            }
            if (n) parts.push(`${n} ${n === 1 ? "Erinnerung" : "Erinnerungen"} gesendet`);
          }
          const summary = parts.length ? parts.join(", ") : "Nichts zu tun";
          const at = new Date().toISOString();
          commit((st) => ({ ...st, automations: (st.automations ?? []).map((r) => (r.id === id ? { ...r, last_run: at, last_result: summary } : r)) }));
          return { ok: failed === 0, summary, payload: { runs: [{ ruleId: id, summary, failed }] } };
        });
      },
      async autostartSet(enabled) { await lat(60); commit((s) => ({ ...s, settings: { ...s.settings, autostart: enabled } })); },
    },
    unsubscribeFallback: {
      async manualLinks(subscriptionId, fromMail = false) {
        await lat(fromMail ? 1100 : 60);
        const sub = snap.subscriptions.find((x) => x.id === subscriptionId);
        if (!sub) throw new IpcError("NotFound", "Newsletter nicht gefunden");
        const links: ManualLinks["links"] = [];
        let mailto: string | undefined;
        for (const m of sub.unsubscribe) {
          if ("Link" in m) links.push({ url: m.Link.url, source: "header", label: "Abmeldelink des Absenders" });
          else if ("OneClick" in m) links.push({ url: m.OneClick.url, source: "header", label: "Abmeldeadresse des Absenders" });
          else mailto = m.Mailto.address;
        }
        if (fromMail) {
          if (!sub.last_message_id) throw new IpcError("Invalid", "Von dieser Liste ist keine Mail gespeichert. Bitte das Postfach erneut scannen");
          links.push(
            { url: `https://${sub.domain}/newsletter/abmelden`, source: "mail", label: "Link am Ende der letzten Mail" },
            { url: `https://${sub.domain}/konto/einstellungen/benachrichtigungen`, source: "mail", label: "Maileinstellungen im Kundenkonto" },
          );
        }
        return { links, mailto };
      },
    },
    jobs: {
      async cancel(jobId) { const j = running.get(jobId); if (j) j.cancel = true; },
      async running() { return [...running.entries()].map(([jobId, j]) => ({ jobId, kind: j.kind, done: j.done, total: j.total })); },
    },
    events: {
      onJobProgress: (cb) => on("job://progress", cb),
      onJobDone: (cb) => on("job://done", cb),
      onVaultChanged: (cb) => on<{ revision: number }>("vault://changed", (p) => cb(p.revision)),
      onVaultLocked: (cb) => on("vault://locked", cb),
      onSaveFailed: (cb) => on<{ message: string }>("vault://save-failed", (p) => cb(p.message)),
      onScheduledRescan: (cb) => on<{ mailboxIds: string[] }>("schedule://rescan", (p) => cb(p.mailboxIds)),
    },
  };
  return api;
}
