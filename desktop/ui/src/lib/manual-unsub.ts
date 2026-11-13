// Manual unsubscribe fallback. The backend command `unsubscribe_manual_links { subscriptionId, fromMail }` returns
// `{ links: [{ url, source, label }], mailto? }`. Header links are known locally right away; the mail search is an
// explicit click and may need a rescan first (the backend only knows the Message-ID after a scan).
import { api } from "./api";
import type { ManualLink, ManualLinks, UnsubscribeFailureReason } from "./ipc";
import type { Subscription } from "./types";

export type { ManualLink, ManualLinks };

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

/** Search the newest mail of this sender for an unsubscribe link. Throws the backend error. */
export async function findManualLinks(s: Subscription): Promise<ManualLinks> {
  const known = knownManualLinks(s);
  const r = await api.unsubscribeFallback.manualLinks(s.id, true);
  return { links: dedupe([...known.links, ...r.links]), mailto: r.mailto ?? known.mailto };
}

/** The backend refuses the mail search until the mailbox was scanned again. */
export function needsRescan(e: unknown): boolean {
  const m = e && typeof e === "object" && "message" in e ? String((e as { message: unknown }).message) : String(e ?? "");
  return /erneut scannen|neu scannen/i.test(m);
}

/** What went wrong with the automatic unsubscribe, in plain words, and what to do instead. */
export const UNSUB_REASON_TEXT: Record<UnsubscribeFailureReason, string> = {
  Http: "Der Server des Absenders hat die Abmeldung mit einem Fehler beantwortet.",
  Network: "Der Server des Absenders war nicht erreichbar. Prüfe deine Verbindung und versuche es später noch einmal.",
  UnsafeTarget: "Der Abmeldelink führt auf eine Adresse, die wir aus Sicherheitsgründen nicht automatisch aufrufen.",
  NoAutomaticMethod: "Der Absender bietet keine automatische Abmeldung an.",
  Refused: "Der Absender hat die Abmeldung nicht angenommen.",
  Smtp: "Die Abmeldemail konnte nicht gesendet werden. Prüfe die Zugangsdaten deines Postfachs.",
};
