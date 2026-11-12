import { useCallback, useEffect, useRef, useState } from "react";
import { errorMessage } from "./utils";
import { useStore } from "./store";

export interface Async<T> { data: T | null; error: string | null; loading: boolean; reload(): void }

/** Load something async; reloads when deps change. Errors are German strings, `reload` retries. */
export function useAsync<T>(fn: () => Promise<T>, deps: unknown[]): Async<T> {
  const [state, setState] = useState<{ data: T | null; error: string | null; loading: boolean }>({ data: null, error: null, loading: true });
  const [tick, setTick] = useState(0);
  const fnRef = useRef(fn);
  fnRef.current = fn;
  useEffect(() => {
    let live = true;
    setState((s) => ({ ...s, loading: true, error: null }));
    fnRef.current().then(
      (data) => live && setState({ data, error: null, loading: false }),
      (e) => live && setState((s) => ({ data: s.data, error: errorMessage(e), loading: false })),
    );
    return () => { live = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, tick]);
  const reload = useCallback(() => setTick((t) => t + 1), []);
  return { ...state, reload };
}

export function isTyping(t: EventTarget | null): boolean {
  const el = t as HTMLElement | null;
  if (!el?.tagName) return false;
  return el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable;
}

/** j/k and arrow keys move an active index, Enter opens. Disabled while typing or a dialog is open. */
export function useListKeys(o: { count: number; index: number; setIndex(i: number): void; onEnter?(i: number): void; enabled?: boolean }) {
  const ref = useRef(o);
  ref.current = o;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const c = ref.current;
      if (c.enabled === false || e.metaKey || e.ctrlKey || e.altKey || isTyping(e.target)) return;
      if (document.querySelector("[role=dialog],[role=menu],[cmdk-root]")) return;
      if (e.key === "j" || e.key === "ArrowDown") { e.preventDefault(); c.setIndex(Math.min(c.count - 1, c.index + 1)); }
      else if (e.key === "k" || e.key === "ArrowUp") { e.preventDefault(); c.setIndex(Math.max(0, c.index - 1)); }
      else if (e.key === "Home") { c.setIndex(0); }
      else if (e.key === "End") { c.setIndex(c.count - 1); }
      else if (e.key === "Enter" && c.index >= 0 && c.index < c.count) { c.onEnter?.(c.index); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}

/** Re-run `fn` whenever the vault changed (after the store refreshed). */
export function useSnapshotKey(): unknown {
  return useStore((s) => s.snap);
}
