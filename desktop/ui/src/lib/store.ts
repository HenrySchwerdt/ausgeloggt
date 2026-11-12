import { create } from "zustand";
import { toast } from "sonner";
import { api, inTauri } from "./api";
import { navigate } from "./router";
import type { AccountStatus, DeletionMethod, Snapshot, SubscriptionStatus } from "./types";
import { ACCOUNT_STATUS_LABEL } from "./types";

export type Theme = "dark" | "light";
type Phase = "boot" | "locked" | "ready";

interface State {
  phase: Phase;
  vaultExists: boolean;
  snap: Snapshot | null;
  theme: Theme;
  paletteOpen: boolean;
  accountsFilter: AccountStatus | "all" | "hidden";
  setAccountsFilter(f: AccountStatus | "all" | "hidden"): void;
  boot(): Promise<void>;
  unlock(pw: string): Promise<void>;
  create(pw: string): Promise<void>;
  lock(): Promise<void>;
  refresh(): Promise<void>;
  setTheme(t: Theme): void;
  setPalette(open: boolean): void;
  setAccountStatus(ids: string[], status: AccountStatus): Promise<void>;
  setNotes(id: string, notes: string): Promise<void>;
  setSubscriptionStatus(ids: string[], status: SubscriptionStatus): Promise<void>;
  markDeletionSent(id: string, method: DeletionMethod, note: string): Promise<void>;
}

const params = new URLSearchParams(typeof location !== "undefined" ? location.search : "");

function initialTheme(): Theme {
  const q = params.get("theme");
  if (q === "light" || q === "dark") return q;
  try {
    const s = localStorage.getItem("spurlos-theme");
    if (s === "light" || s === "dark") return s;
  } catch { /* storage unavailable */ }
  return "dark";
}

function applyTheme(t: Theme) {
  document.documentElement.classList.toggle("dark", t === "dark");
  document.documentElement.classList.toggle("light", t === "light");
}

export const useStore = create<State>((set, get) => ({
  phase: "boot",
  vaultExists: true,
  snap: null,
  theme: initialTheme(),
  paletteOpen: params.has("palette"),
  accountsFilter: "all",
  setAccountsFilter(f) { set({ accountsFilter: f }); },

  async boot() {
    applyTheme(get().theme);
    const exists = await api.vaultExists();
    set({ vaultExists: exists });
    if (api.kind === "mock" && !params.has("locked") && !params.has("unlock") && !inTauri) {
      await api.unlock("demo");
      await get().refresh();
      set({ phase: "ready" });
      const sel = params.get("select");
      if (sel && get().snap) {
        const q = sel.toLowerCase();
        const a = get().snap!.accounts.find((x) => x.id === sel || x.domain === q || x.name.toLowerCase() === q);
        if (a) navigate({ page: "accounts", id: a.id });
      }
      return;
    }
    set({ phase: "locked", vaultExists: params.has("create") ? false : exists });
  },

  async unlock(pw) {
    await api.unlock(pw);
    await get().refresh();
    set({ phase: "ready" });
  },
  async create(pw) {
    await api.create(pw);
    await get().refresh();
    set({ phase: "ready", vaultExists: true });
  },
  async lock() {
    await api.lock();
    set({ phase: "locked", snap: null });
    navigate({ page: "dashboard" });
  },
  async refresh() {
    set({ snap: await api.getSnapshot() });
  },
  setTheme(t) {
    applyTheme(t);
    try { localStorage.setItem("spurlos-theme", t); } catch { /* ignore */ }
    set({ theme: t });
  },
  setPalette(open) { set({ paletteOpen: open }); },

  async setAccountStatus(ids, status) {
    await api.setAccountStatus(ids, status);
    await get().refresh();
    toast(ids.length === 1 ? `Status: ${ACCOUNT_STATUS_LABEL[status]}` : `${ids.length} Accounts: ${ACCOUNT_STATUS_LABEL[status]}`, {
      description: undefined,
    });
  },
  async setNotes(id, notes) {
    await api.setNotes(id, notes);
    await get().refresh();
  },
  async setSubscriptionStatus(ids, status) {
    await api.setSubscriptionStatus(ids, status);
    await get().refresh();
  },
  async markDeletionSent(id, method, note) {
    await api.markDeletionSent(id, method, note);
    await get().refresh();
  },
}));
