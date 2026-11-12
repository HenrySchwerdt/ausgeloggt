import { create } from "zustand";
import { toast } from "sonner";
import { api, inTauri, forceMock } from "./api";
import { navigate } from "./router";
import { errorMessage, throttle } from "./utils";
import type {
  AccountStatus, InsightsDto, JobKind, JobProgress, ReferenceData, Snapshot, SubscriptionStatus,
} from "./types";
import { ACCOUNT_STATUS_LABEL } from "./types";

export type Theme = "dark" | "light";
type Phase = "boot" | "locked" | "ready";

export interface RunningJobInfo { jobId: string; kind: JobKind; done: number; total: number; message: string }

export const JOB_LABEL: Record<JobKind, string> = {
  scan: "Postfach wird geprüft",
  import: "Import läuft",
  unsubscribe: "Abmeldung läuft",
  ai_classify: "KI ordnet ein",
  screenshot_read: "Screenshots werden gelesen",
  breach_check: "Datenlecks werden geprüft",
  catalog_refresh: "Katalog wird aktualisiert",
};

interface State {
  phase: Phase;
  vaultExists: boolean;
  vaultPath: string;
  snap: Snapshot | null;
  insights: InsightsDto | null;
  insightsError: string | null;
  loadError: string | null;
  reference: ReferenceData | null;
  theme: Theme;
  paletteOpen: boolean;
  accountsFilter: AccountStatus | "all" | "hidden";
  jobs: Record<string, RunningJobInfo>;
  dropHover: boolean;
  setAccountsFilter(f: AccountStatus | "all" | "hidden"): void;
  boot(): Promise<void>;
  unlock(pw: string): Promise<void>;
  create(pw: string): Promise<void>;
  lock(): Promise<void>;
  refresh(): Promise<void>;
  ensureReference(): Promise<ReferenceData | null>;
  setTheme(t: Theme): void;
  setPalette(open: boolean): void;
  setDropHover(b: boolean): void;
  setAccountStatus(ids: string[], status: AccountStatus, quiet?: boolean): Promise<void>;
  setNotes(id: string, notes: string): Promise<void>;
  setSubscriptionStatus(ids: string[], status: SubscriptionStatus): Promise<void>;
}

const params = new URLSearchParams(typeof location !== "undefined" ? location.search : "");

function initialTheme(): Theme {
  const q = params.get("theme");
  if (q === "light" || q === "dark") return q;
  try {
    const s = localStorage.getItem("ausgeloggt-theme");
    if (s === "light" || s === "dark") return s;
  } catch { /* storage unavailable */ }
  return "dark";
}

function applyTheme(t: Theme) {
  document.documentElement.classList.toggle("dark", t === "dark");
  document.documentElement.classList.toggle("light", t === "light");
}

let refreshTimer: ReturnType<typeof setTimeout> | null = null;
let eventsBound = false;

export const useStore = create<State>((set, get) => {
  async function loadAll() {
    const [snap, ins] = await Promise.allSettled([api.data.snapshot(), api.data.insights()]);
    if (snap.status === "rejected") {
      const e = snap.reason as { code?: string };
      if (e?.code === "VaultLocked") { set({ phase: "locked", snap: null, insights: null }); return; }
      set({ loadError: errorMessage(snap.reason) });
      return;
    }
    set({
      snap: snap.value, loadError: null,
      ...(ins.status === "fulfilled" ? { insights: ins.value, insightsError: null } : { insightsError: errorMessage(ins.reason) }),
    });
  }

  function bindEvents() {
    if (eventsBound) return;
    eventsBound = true;
    void api.events.onVaultChanged(() => {
      if (get().phase !== "ready") return;
      if (refreshTimer) clearTimeout(refreshTimer);
      refreshTimer = setTimeout(() => { void loadAll(); }, 120);
    });
    void api.events.onVaultLocked(() => {
      set({ phase: "locked", snap: null, insights: null, jobs: {} });
      navigate({ page: "dashboard" });
    });
    void api.events.onSaveFailed((m) => toast.error(`Speichern fehlgeschlagen. ${m}`));
    void api.events.onScheduledRescan(() => toast("Automatische Prüfung der Postfächer gestartet"));
    void api.events.onJobProgress((p: JobProgress) => {
      set((s) => ({ jobs: { ...s.jobs, [p.jobId]: { jobId: p.jobId, kind: p.kind, done: p.done, total: p.total, message: p.message } } }));
    });
    void api.events.onJobDone((d) => {
      set((s) => { const j = { ...s.jobs }; delete j[d.jobId]; return { jobs: j }; });
      if (d.ok) toast(d.summary);
      else if (d.summary === "Abgebrochen") toast("Abgebrochen");
      else toast.error(d.error?.message ?? d.summary);
    });
    const ping = throttle(() => { if (get().phase === "ready") void api.vault.touch().catch(() => {}); }, 15_000);
    for (const ev of ["pointerdown", "keydown", "wheel"] as const) window.addEventListener(ev, ping, { passive: true });
    window.addEventListener("pointermove", ping, { passive: true });
  }

  async function enter() {
    await loadAll();
    set({ phase: get().snap ? "ready" : get().phase });
    bindEvents();
    try {
      const running = await api.jobs.running();
      set({ jobs: Object.fromEntries(running.map((j) => [j.jobId, { ...j, message: "" }])) });
    } catch { /* ignore */ }
  }

  return {
    phase: "boot",
    vaultExists: true,
    vaultPath: "",
    snap: null,
    insights: null,
    insightsError: null,
    loadError: null,
    reference: null,
    theme: initialTheme(),
    paletteOpen: params.has("palette"),
    accountsFilter: "all",
    jobs: {},
    dropHover: false,
    setAccountsFilter(f) { set({ accountsFilter: f }); },

    async boot() {
      applyTheme(get().theme);
      let status = { exists: true, unlocked: false, path: "" };
      try { status = await api.vault.status(); } catch { /* fall through to unlock screen */ }
      set({ vaultExists: status.exists, vaultPath: status.path });
      if (status.unlocked) { await enter(); return; }
      if (api.kind === "mock" && !params.has("locked") && !params.has("create") && !(inTauri && !forceMock)) {
        await api.vault.unlock("demo");
        await enter();
        const sel = params.get("select");
        const snap = get().snap;
        if (sel && snap) {
          const q = sel.toLowerCase();
          const a = snap.accounts.find((x) => x.id === sel || x.domain === q || x.name.toLowerCase() === q);
          if (a) navigate({ page: "accounts", id: a.id });
        }
        return;
      }
      set({ phase: "locked" });
    },

    async unlock(pw) {
      await api.vault.unlock(pw);
      await enter();
    },
    async create(pw) {
      await api.vault.create(pw);
      set({ vaultExists: true });
      await enter();
    },
    async lock() {
      try { await api.vault.lock(); } catch { /* already locked */ }
      set({ phase: "locked", snap: null, insights: null, jobs: {} });
      navigate({ page: "dashboard" });
    },
    async refresh() { await loadAll(); },
    async ensureReference() {
      if (get().reference) return get().reference;
      try {
        const r = await api.data.referenceData();
        set({ reference: r });
        return r;
      } catch { return null; }
    },
    setTheme(t) {
      applyTheme(t);
      try { localStorage.setItem("ausgeloggt-theme", t); } catch { /* ignore */ }
      set({ theme: t });
    },
    setPalette(open) { set({ paletteOpen: open }); },
    setDropHover(b) { set({ dropHover: b }); },

    async setAccountStatus(ids, status, quiet) {
      try {
        await api.accounts.setStatus(ids, status);
      } catch (e) { toast.error(errorMessage(e)); return; }
      if (!quiet) toast(ids.length === 1 ? ACCOUNT_STATUS_LABEL[status] : `${ids.length} Accounts: ${ACCOUNT_STATUS_LABEL[status]}`);
    },
    async setNotes(id, notes) {
      try { await api.accounts.setNotes(id, notes); } catch (e) { toast.error(errorMessage(e)); }
    },
    async setSubscriptionStatus(ids, status) {
      try { await api.newsletters.setStatus(ids, status); } catch (e) { toast.error(errorMessage(e)); }
    },
  };
});
