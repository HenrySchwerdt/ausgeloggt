// Manual unsubscribe fallback. The backend command `unsubscribe_manual_links { subscriptionId }` returns
// `{ links: [{ url, source, label }], mailto? }`. Until it exists the adapter falls back to what the vault already knows.
import { inTauri, forceMock } from "./api";
import type { Subscription } from "./types";

export interface ManualLink { url: string; source: "header" | "mail"; label: string }
export interface ManualLinks { links: ManualLink[]; mailto?: string }

const dedupe = (l: ManualLink[]) => l.filter((x, i) => l.findIndex((y) => y.url === x.url) === i);

/** Links and addresses already known from the list headers. */
export function knownManualLinks(s: Subscription): ManualLinks {
  const links: ManualLink[] = [];
  let mailto: string | undefined;
  for (const m of s.unsubscribe) {
    if ("Link" in m) links.push({ url: m.Link.url, source: "header", label: "Abmeldelink des Absenders" });
    else if ("OneClick" in m) links.push({ url: m.OneClick.url, source: "header", label: "Abmeldeadresse des Absenders" });
    else mailto = m.Mailto.address;
  }
  return { links, mailto };
}

/** Search the latest mail of this sender for an unsubscribe link. */
export async function findManualLinks(s: Subscription): Promise<ManualLinks> {
  const known = knownManualLinks(s);
  if (inTauri && !forceMock) {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const r = await invoke<ManualLinks>("unsubscribe_manual_links", { subscriptionId: s.id });
      return { links: dedupe([...r.links, ...known.links]), mailto: r.mailto ?? known.mailto };
    } catch { return known; }
  }
  // demo data
  await new Promise((r) => setTimeout(r, 700));
  return {
    links: dedupe([
      ...known.links,
      { url: `https://${s.domain}/newsletter/abmelden`, source: "mail", label: "Link am Ende der letzten Mail" },
      { url: `https://${s.domain}/konto/einstellungen/benachrichtigungen`, source: "mail", label: "Maileinstellungen im Kundenkonto" },
    ]),
    mailto: known.mailto ?? `abmelden@${s.domain}`,
  };
}
