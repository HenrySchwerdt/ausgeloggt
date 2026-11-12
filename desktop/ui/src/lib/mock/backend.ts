// In memory demo backend: implements the whole Backend surface (see ../api.ts) with fake data.
// Query switches: ?empty (empty vault), ?big=5000 (huge lists), ?noidentity (no sender data),
// ?ai=ollama (AI enabled), ?fail=insights (one command fails, for error states).
import { IpcError } from "../ipc";
import type { MailboxDto, SettingsDto } from "../ipc";
import type { Backend } from "../api";
import type {
  Account, DataRequest, JobDone, JobId, JobKind, JobProgress, RequestKind, Snapshot,
  Subscription,
} from "../types";
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
  const hasAnthropic = { v: false };

  const settings: SettingsDto = {
    auto_lock_minutes: 10, min_confidence: 0.5, full_name: "Lena Hartmann", letter_language: "de",
    allow_catalog_updates: true, dark_mode: true, fetch_logos: false,
    ai: { provider: q.get("ai") === "ollama" ? "Ollama" : "Off", ollama_url: "http://127.0.0.1:11434", ollama_model: q.get("ai") === "ollama" ? "llama3.1:8b" : "", anthropic_model: "claude-haiku-5-5", has_anthropic_key: false },
    identity: q.has("noidentity")
      ? { street: "", postal_code: "", city: "", country: "Deutschland", state: "", birth_date: "", previous_addresses: [], phone: "", email: "" }
      : { street: "Lindenallee 12", postal_code: "80331", city: "München", country: "Deutschland", state: "DE-BY", birth_date: "1989-04-17", previous_addresses: ["Gartenstraße 3, 10115 Berlin"], phone: "", email: "lena.hartmann@gmail.com" },
    check_pwned_passwords: true, read_receipt_bodies: false, auto_rescan_days: 7, notifications: true, has_hibp_key: false,
  };

  const baseAccounts = enrichAccounts(buildAccounts());
  const baseSubs = withPostUnsubscribeMails(buildSubscriptions());
  const mailboxes: MailboxDto[] = empty ? [] : MAILBOXES.map((address, i) => ({
    id: `m${i}`, address, imap_host: i ? "imap.web.de" : "imap.gmail.com", imap_port: 993, username: address,
    folders: ["INBOX"], progress: [{ folder: "INBOX", uid_validity: 1, last_uid: 18420 }], last_scan: ago(2), has_password: true,
  }));
  const accounts = empty ? [] : big > 0 ? bigAccounts(baseAccounts, big) : baseAccounts;
  const subs = empty ? [] : big > 0 ? bigSubs(baseSubs, big) : baseSubs;
  let snap: Snapshot = {
    schema: 1, accounts, runs: empty ? [] : buildRuns(), mailboxes, settings, subscriptions: subs,
    requests: empty ? [] : buildRequests(accounts, subs),
  };

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
    if ([...running.values()].some((j) => j.kind === kind)) throw new IpcError("Busy", "Dieser Vorgang läuft bereits.");
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
          const results: { id: string; ok: boolean; skipped: boolean; message: string }[] = [];
          let n = 0;
          for (const s of items) {
            if (c.cancelled()) break;
            n++;
            const auto = s.unsubscribe.some((m) => !("Link" in m)) && s.category !== "Transactional";
            const failed = auto && n % 7 === 0;
            const r = { id: s.id, ok: auto && !failed, skipped: !auto, message: !auto ? "Nur Link, bitte selbst öffnen" : failed ? "Server antwortet mit Fehler 503" : "Abgemeldet" };
            results.push(r);
            c.progress(n, items.length, s.name, { id: s.id, ok: r.ok, message: r.message });
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
        if (kind === "hibp") hasHibp.v = !!value; else hasAnthropic.v = !!value;
        commit((s) => ({ ...s, settings: { ...s.settings, has_hibp_key: hasHibp.v, ai: { ...s.settings.ai, has_anthropic_key: hasAnthropic.v } } }));
      },
      async aiTest() {
        await lat(900);
        if (snap.settings.ai.provider === "Off") throw new IpcError("Invalid", "Die KI ist ausgeschaltet.");
        if (snap.settings.ai.provider === "Anthropic" && !hasAnthropic.v) throw new IpcError("Auth", "Es ist kein Schlüssel hinterlegt.");
        return snap.settings.ai.provider === "Ollama" ? `Verbunden, ${snap.settings.ai.ollama_model || "Modell"} antwortet.` : "Verbunden, Claude antwortet.";
      },
      async aiModels() {
        await lat(300);
        const p = snap.settings.ai.provider;
        if (p === "Ollama") return { text: ["llama3.1:8b", "qwen2.5:7b", "mistral:7b"], vision: ["llava:13b", "qwen2.5vl:7b"] };
        if (p === "Anthropic") return { text: ["claude-haiku-5-5", "claude-sonnet-5-5"], vision: ["claude-sonnet-5-5"] };
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
