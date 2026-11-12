import {
  buildAccounts, buildRuns, buildSubscriptions, catalogFor, gdprLetterText, MAILBOXES,
  type CatalogInfo, type GdprLetter,
} from "./mock-data";
import type {
  AccountStatus, DeletionMethod, Settings, Snapshot, SubscriptionStatus,
} from "./types";

export type { CatalogInfo, GdprLetter };

/** Everything the UI needs from the backend. Phase 1-2 surface; extended in later phases. */
export interface Api {
  readonly kind: "mock" | "tauri";
  vaultExists(): Promise<boolean>;
  create(password: string): Promise<void>;
  unlock(password: string): Promise<void>;
  lock(): Promise<void>;
  getSnapshot(): Promise<Snapshot>;
  /** Bulk status change; `ids` may contain one or many accounts. */
  setAccountStatus(ids: string[], status: AccountStatus): Promise<void>;
  setNotes(id: string, notes: string): Promise<void>;
  setSubscriptionStatus(ids: string[], status: SubscriptionStatus): Promise<void>;
  /** Deletion catalog entry for a domain (link, difficulty, contact). */
  catalogFor(domain: string): Promise<CatalogInfo>;
  /** Render the GDPR Art. 17 letter for an account. */
  gdprLetter(id: string): Promise<GdprLetter>;
  /** Record that a deletion step was performed (sets status to DeletionPending, computes deadline). */
  markDeletionSent(id: string, method: DeletionMethod, note: string): Promise<void>;
  updateSettings(patch: Partial<Settings>): Promise<void>;
  openUrl(url: string): Promise<void>;
}

const delay = (ms = 120) => new Promise((r) => setTimeout(r, ms));

export function createMockApi(): Api {
  let unlocked = false;
  let exists = true;
  let snap: Snapshot = {
    schema: 1,
    accounts: buildAccounts(),
    runs: buildRuns(),
    mailboxes: MAILBOXES.map((address, i) => ({
      id: `m${i}`, address, imap_host: i ? "imap.web.de" : "imap.gmail.com", imap_port: 993, username: address,
      password: "", folders: [], progress: [], last_scan: new Date(Date.now() - 2 * 86_400_000).toISOString(),
    })),
    settings: {
      auto_lock_minutes: 10, min_confidence: 0.5, full_name: "Lena Hartmann", letter_language: "de",
      allow_catalog_updates: true, dark_mode: true, fetch_logos: true,
      ai: { provider: "Off", ollama_url: "http://127.0.0.1:11434", ollama_model: "", anthropic_api_key: "", anthropic_model: "claude-haiku-5-5" },
    },
    subscriptions: buildSubscriptions(),
  };

  const touch = (ids: string[], fn: (a: Snapshot["accounts"][number]) => void) => {
    const set = new Set(ids);
    snap = {
      ...snap,
      accounts: snap.accounts.map((a) => {
        if (!set.has(a.id)) return a;
        const c = structuredClone(a);
        fn(c);
        c.updated_at = new Date().toISOString();
        return c;
      }),
    };
  };

  return {
    kind: "mock",
    async vaultExists() { return exists; },
    async create(password) {
      await delay(500);
      if (password.length < 8) throw new Error("Das Passwort muss mindestens 8 Zeichen lang sein.");
      exists = true; unlocked = true;
    },
    async unlock(password) {
      await delay(450);
      if (!password) throw new Error("Falsches Passwort.");
      unlocked = true;
    },
    async lock() { unlocked = false; },
    async getSnapshot() {
      if (!unlocked) throw new Error("locked");
      return snap;
    },
    async setAccountStatus(ids, status) { await delay(60); touch(ids, (a) => { a.status = status; }); },
    async setNotes(id, notes) { touch([id], (a) => { a.notes = notes; }); },
    async setSubscriptionStatus(ids, status) {
      const set = new Set(ids);
      snap = { ...snap, subscriptions: snap.subscriptions.map((s) => (set.has(s.id) ? { ...s, status } : s)) };
    },
    async catalogFor(domain) { await delay(40); return catalogFor(domain); },
    async gdprLetter(id) {
      await delay(60);
      const a = snap.accounts.find((x) => x.id === id);
      if (!a) throw new Error("Account nicht gefunden");
      const c = catalogFor(a.domain);
      return gdprLetterText(a, snap.settings.full_name || "[Dein Name]", c.contact_email ?? `datenschutz@${a.domain}`, snap.settings.letter_language);
    },
    async markDeletionSent(id, method, note) {
      const at = new Date();
      const gdpr = typeof method !== "string" && "GdprEmail" in method;
      touch([id], (a) => {
        a.status = "DeletionPending";
        a.deletion.push({ at: at.toISOString(), method, note, deadline: gdpr ? new Date(at.getTime() + 30 * 86_400_000).toISOString() : null });
      });
    },
    async updateSettings(patch) { snap = { ...snap, settings: { ...snap.settings, ...patch } }; },
    async openUrl(url) { window.open(url, "_blank", "noopener"); },
  };
}

/** Real backend: IPC commands in desktop/src-tauri. Wired up in a later phase. */
export function createTauriApi(): Api {
  const call = async <T,>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(cmd, args);
  };
  return {
    kind: "tauri",
    vaultExists: () => call("vault_exists"),
    create: (password) => call("vault_create", { password }),
    unlock: (password) => call("vault_unlock", { password }),
    lock: () => call("vault_lock"),
    getSnapshot: () => call("get_snapshot"),
    setAccountStatus: (ids, status) => call("set_account_status", { ids, status }),
    setNotes: (id, notes) => call("set_notes", { id, notes }),
    setSubscriptionStatus: (ids, status) => call("set_subscription_status", { ids, status }),
    catalogFor: (domain) => call("catalog_for", { domain }),
    gdprLetter: (id) => call("gdpr_letter", { id }),
    markDeletionSent: (id, method, note) => call("mark_deletion_sent", { id, method, note }),
    updateSettings: (patch) => call("update_settings", { patch }),
    openUrl: async (url) => {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
    },
  };
}

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
const forceMock = typeof location !== "undefined" && new URLSearchParams(location.search).has("mock");

/** Flip to true once the IPC commands in desktop/src-tauri exist (phase 2). Until then the shell shows demo data. */
const BACKEND_READY = false;

export const api: Api = inTauri && BACKEND_READY && !forceMock ? createTauriApi() : createMockApi();
