// Data layer facade. In Tauri it is the typed IPC client (ipc.ts); in the browser (?mock or no Tauri)
// it is an in memory demo backend with realistic fake data (lib/mock/backend.ts).
import * as ipc from "./ipc";
import type { JobDone, JobId, JobProgress, Uuid } from "./ipc";

export { IpcError } from "./ipc";

type Unlisten = () => void;

export interface Backend {
  readonly kind: "mock" | "tauri";
  vault: typeof ipc.vault;
  data: typeof ipc.data;
  accounts: typeof ipc.accounts;
  mail: typeof ipc.mail;
  imports: typeof ipc.imports;
  newsletters: typeof ipc.newsletters;
  security: typeof ipc.security;
  letters: typeof ipc.letters;
  requests: typeof ipc.requests;
  settings: typeof ipc.settings;
  jobs: typeof ipc.jobs;
  sending: typeof ipc.sending;
  automations: typeof ipc.automations;
  unsubscribeFallback: typeof ipc.unsubscribeFallback;
  events: {
    onJobProgress(cb: (p: JobProgress) => void): Promise<Unlisten>;
    onJobDone(cb: (d: JobDone) => void): Promise<Unlisten>;
    onVaultChanged(cb: (revision: number) => void): Promise<Unlisten>;
    onVaultLocked(cb: () => void): Promise<Unlisten>;
    onSaveFailed(cb: (message: string) => void): Promise<Unlisten>;
    onScheduledRescan(cb: (mailboxIds: Uuid[]) => void): Promise<Unlisten>;
  };
}

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
const params = new URLSearchParams(typeof location !== "undefined" ? location.search : "");
export const forceMock = params.has("mock");

const real: Backend = {
  kind: "tauri",
  vault: ipc.vault, data: ipc.data, accounts: ipc.accounts, mail: ipc.mail, imports: ipc.imports,
  newsletters: ipc.newsletters, security: ipc.security, letters: ipc.letters, requests: ipc.requests,
  settings: ipc.settings, jobs: ipc.jobs,
  sending: ipc.sending, automations: ipc.automations, unsubscribeFallback: ipc.unsubscribeFallback,
  events: {
    onJobProgress: ipc.onJobProgress, onJobDone: ipc.onJobDone, onVaultChanged: ipc.onVaultChanged,
    onVaultLocked: ipc.onVaultLocked, onSaveFailed: ipc.onSaveFailed, onScheduledRescan: ipc.onScheduledRescan,
  },
};

export const api: Backend = inTauri && !forceMock
  ? real
  : (await import("./mock/backend")).createMockBackend();

/** Start a job and wait for it (listeners are attached before the command is sent). */
export async function runJob<P = unknown>(
  start: () => Promise<JobId>,
  opts: { onProgress?: (p: JobProgress) => void; onStarted?: (id: JobId) => void } = {},
): Promise<JobDone<P>> {
  let jobId: JobId | null = null;
  const early: JobProgress[] = [];
  const earlyDone: JobDone<P>[] = [];
  let resolve!: (d: JobDone<P>) => void;
  const finished = new Promise<JobDone<P>>((r) => (resolve = r));
  const unProgress = await api.events.onJobProgress((p) => {
    if (jobId === null) early.push(p);
    else if (p.jobId === jobId) opts.onProgress?.(p);
  });
  const unDone = await api.events.onJobDone((d) => {
    if (jobId === null) earlyDone.push(d as JobDone<P>);
    else if (d.jobId === jobId) resolve(d as JobDone<P>);
  });
  try {
    jobId = await start();
    opts.onStarted?.(jobId);
    for (const p of early) if (p.jobId === jobId) opts.onProgress?.(p);
    const already = earlyDone.find((d) => d.jobId === jobId);
    if (already) resolve(already);
    return await finished;
  } finally {
    unProgress();
    unDone();
  }
}
