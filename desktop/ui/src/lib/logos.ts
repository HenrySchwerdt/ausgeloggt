import { useEffect, useState } from "react";
import { api } from "./api";
import { useStore } from "./store";

const cache = new Map<string, string | null>();
const waiters = new Map<string, Promise<string | null>>();
let active = 0;
const queue: (() => void)[] = [];

function pump() {
  while (active < 4 && queue.length) { active++; queue.shift()!(); }
}

function fetchLogo(domain: string): Promise<string | null> {
  const hit = waiters.get(domain);
  if (hit) return hit;
  const p = new Promise<string | null>((resolve) => {
    queue.push(() => {
      api.data.logo(domain).then(resolve, () => resolve(null)).finally(() => { active--; pump(); });
    });
    pump();
  }).then((v) => { cache.set(domain, v); return v; });
  waiters.set(domain, p);
  return p;
}

/** Favicon from the service's own domain (backend, cached, only if enabled in the settings). */
export function useLogo(domain: string, wanted: boolean): string | null {
  const enabled = useStore((s) => s.snap?.settings.fetch_logos ?? false);
  const [src, setSrc] = useState<string | null>(cache.get(domain) ?? null);
  useEffect(() => {
    if (!wanted || !enabled || domain.startsWith("unresolved:")) { setSrc(null); return; }
    if (cache.has(domain)) { setSrc(cache.get(domain)!); return; }
    let live = true;
    void fetchLogo(domain).then((v) => live && setSrc(v));
    return () => { live = false; };
  }, [domain, wanted, enabled]);
  return src;
}
