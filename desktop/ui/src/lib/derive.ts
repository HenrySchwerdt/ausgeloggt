import type { Account, DataRequest, Evidence, EvidenceKind, Snapshot } from "./types";
import { PASSWORD_FORMAT_LABEL, sourceTag, ssoLabel } from "./types";
import { daysFromNow, de, isApplePrivateRelay } from "./utils";

/** Accounts that are hidden from the default views. */
export const isHidden = (a: Account) => a.status === "NotAnAccount" || a.status === "Ignored";
/** Accounts that still need attention. */
export const isOpen = (a: Account) => a.status === "Discovered" || a.status === "Confirmed" || a.status === "Keep" || a.status === "DeletionPending";

export const isLiveRequest = (r: DataRequest) => r.status === "Sent" || r.status === "Draft";

export function requestDays(r: DataRequest): number | null {
  return r.deadline && r.status === "Sent" ? daysFromNow(r.deadline) : null;
}
export const isOverdue = (r: DataRequest) => {
  const d = requestDays(r);
  return d !== null && d < 0;
};

/** Domain for logos / links, `unresolved:` entries have none. */
export const realDomain = (d: string) => (d.startsWith("unresolved:") ? null : d);

// ---- how sure are we ----------------------------------------------------------------

export type Tier = "high" | "mid" | "low";
export const TIER_LABEL: Record<Tier, string> = { high: "Sicher", mid: "Wahrscheinlich", low: "Unsicher" };

const WEAK_KINDS = new Set<EvidenceKind>(["Newsletter", "OtherMail"]);

/** Only newsletters or other mails: the sender knows the address, that does not prove an account. */
export const hasOnlyAddress = (a: Account) => a.evidence.length > 0 && a.evidence.every((e) => WEAK_KINDS.has(e.kind));

export function tierOf(a: Account): Tier {
  if (hasOnlyAddress(a)) return "low";
  return a.confidence >= 0.8 ? "high" : a.confidence >= 0.5 ? "mid" : "low";
}
export const tierWord = (a: Account) => TIER_LABEL[tierOf(a)];

/** Discovered but too weak to ask about by default. */
export const isWeak = (a: Account) => a.status === "Discovered" && tierOf(a) === "low";

export function discoveryStats(accounts: Account[]) {
  const s = { total: 0, high: 0, mid: 0, low: 0 };
  for (const a of accounts) {
    if (a.status !== "Discovered") continue;
    s.total++;
    s[tierOf(a)]++;
  }
  return s;
}

// ---- evidence in plain German ---------------------------------------------------------

const STRENGTH: Record<EvidenceKind, number> = {
  Manual: 10, Welcome: 10, Verification: 9, PasswordReset: 9, LoginAlert: 8, SavedPassword: 8, TwoFactor: 8,
  SsoGrant: 7, Payment: 7, Receipt: 7, DataExport: 6, Screenshot: 6, Breach: 3, Newsletter: 1, OtherMail: 0,
};

export const evidenceStrength = (e: Evidence) => STRENGTH[e.kind];

/** Strongest first, then newest. */
export function rankedEvidence(a: Account): Evidence[] {
  return [...a.evidence].sort((x, y) => STRENGTH[y.kind] - STRENGTH[x.kind] || ((y.date ?? "") > (x.date ?? "") ? 1 : -1));
}

export const topStrength = (a: Account) => Math.max(0, ...a.evidence.map(evidenceStrength));

const longDate = (iso: string) => new Date(iso).toLocaleDateString("de-DE", { day: "numeric", month: "long", year: "numeric" });
const year = (iso: string | null) => (iso ? ` ${new Date(iso).getFullYear()}` : "");
const vom = (iso: string | null) => (iso ? ` vom ${longDate(iso)}` : "");

function subjectOf(e: Evidence): string | null {
  const s = de(e.summary).trim();
  return !s || s === "(ohne Betreff)" ? null : s;
}

const isMail = (e: Evidence) => sourceTag(e.source) === "Mail";

function exportLabel(e: Evidence): string {
  const t = sourceTag(e.source);
  return t === "GoogleTakeout" ? "Google Takeout" : t === "FacebookExport" ? "Facebook Export" : t === "GenericList" ? "Liste" : "Datenexport";
}

function passwordSource(e: Evidence): string {
  const s = e.source;
  return typeof s !== "string" && "PasswordExport" in s ? PASSWORD_FORMAT_LABEL[s.PasswordExport.format] : "einem Passwortexport";
}

/** One reason, e.g. text "Willkommensmail vom 3. Mai 2021", detail "Willkommen bei Spotify". */
export function reasonOf(e: Evidence, a: Account, multiMailbox: boolean): { text: string; detail: string | null } {
  const subj = isMail(e) ? subjectOf(e) : null;
  const box = multiMailbox && isMail(e) && typeof e.source !== "string" && "Mail" in e.source ? e.source.Mail.mailbox : null;
  const withBox = (d: string | null) => [d ? `„${d}“` : null, box ? `Postfach ${box}` : null].filter(Boolean).join(", ") || null;
  switch (e.kind) {
    case "Welcome": return { text: `Willkommensmail${vom(e.date)}`, detail: withBox(subj) };
    case "Verification": return { text: `Mail zur Bestätigung deiner Adresse${vom(e.date)}`, detail: withBox(subj) };
    case "PasswordReset": return { text: `Mail zum Zurücksetzen des Passworts${vom(e.date)}`, detail: withBox(subj) };
    case "LoginAlert": return { text: `Hinweis auf eine Anmeldung${vom(e.date)}`, detail: withBox(subj) };
    case "Receipt": return { text: `Bestellung oder Rechnung${vom(e.date)}`, detail: withBox(subj) };
    case "Payment": return { text: `Zahlung${vom(e.date)}`, detail: withBox(subj) };
    case "Newsletter": return { text: `Newsletter dieses Absenders${vom(e.date)}`, detail: withBox(subj) };
    case "OtherMail": return { text: `Mail dieses Absenders${vom(e.date)}`, detail: withBox(subj) };
    case "SavedPassword": return { text: `Gespeichert in ${passwordSource(e)}`, detail: null };
    case "SsoGrant": return { text: a.sso.length ? `Mit ${a.sso.map(ssoLabel).join(" und ")} angemeldet` : "Mit einem Drittanbieter angemeldet", detail: null };
    case "DataExport": return { text: `Gefunden im ${exportLabel(e)}`, detail: null };
    case "Breach": return { text: "Taucht in einem bekannten Datenleck auf", detail: null };
    case "TwoFactor": return { text: "Zweiter Faktor eingerichtet", detail: null };
    case "Screenshot": return { text: "Auf einem Screenshot gefunden", detail: null };
    case "Manual": return { text: "Von dir hinzugefügt", detail: null };
  }
}

/** Reasons for the "Warum wir das glauben" list: strongest first, one line per kind and source. */
export function reasonsOf(a: Account, multiMailbox: boolean): { text: string; detail: string | null; kind: EvidenceKind }[] {
  const seen = new Set<string>();
  const out: { text: string; detail: string | null; kind: EvidenceKind }[] = [];
  for (const e of rankedEvidence(a)) {
    const r = reasonOf(e, a, multiMailbox);
    const key = e.kind === "SsoGrant" || e.kind === "SavedPassword" ? r.text : `${e.kind}|${r.text}|${r.detail}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push({ ...r, kind: e.kind });
  }
  // SSO known from the account but not as evidence
  if (a.sso.length && !out.some((r) => r.kind === "SsoGrant")) {
    out.push({ text: `Mit ${a.sso.map(ssoLabel).join(" und ")} angemeldet`, detail: null, kind: "SsoGrant" });
  }
  return out;
}

/** The most telling short phrases for list rows, e.g. "Willkommensmail 2021, Passwort in Chrome". */
export function headline(a: Account): string {
  if (hasOnlyAddress(a)) return "Hat nur deine Adresse";
  const parts: string[] = [];
  const kinds = new Set<EvidenceKind>();
  for (const e of rankedEvidence(a)) {
    if (kinds.has(e.kind)) continue;
    kinds.add(e.kind);
    switch (e.kind) {
      case "Welcome": parts.push(`Willkommensmail${year(e.date)}`); break;
      case "Verification": parts.push(`Bestätigungsmail${year(e.date)}`); break;
      case "PasswordReset": parts.push(`Passwortmail${year(e.date)}`); break;
      case "LoginAlert": parts.push(`Anmeldehinweis${year(e.date)}`); break;
      case "SavedPassword": parts.push(`Passwort in ${passwordSource(e)}`); break;
      case "SsoGrant": parts.push(a.sso.length ? `Mit ${a.sso.map(ssoLabel).join(" und ")} angemeldet` : "Mit Drittanbieter angemeldet"); break;
      case "Receipt": parts.push(`Bestellung${year(e.date)}`); break;
      case "Payment": parts.push(`Zahlung${year(e.date)}`); break;
      case "DataExport": parts.push(`Im ${exportLabel(e)}`); break;
      case "Breach": parts.push("Datenleck"); break;
      case "TwoFactor": parts.push("2FA eingerichtet"); break;
      case "Screenshot": parts.push("Screenshot"); break;
      case "Manual": parts.push("Von dir hinzugefügt"); break;
      default: break;
    }
    if (parts.length === 2) break;
  }
  return parts.join(", ") || "Nur Mails";
}

/** Identifiers worth showing: only if they differ from the user's own mailbox addresses, or there are several. */
export function shownIdentifiers(a: Account, snap: Pick<Snapshot, "mailboxes" | "settings">): string[] {
  const own = new Set([...snap.mailboxes.map((m) => m.address.toLowerCase()), snap.settings.identity.email.toLowerCase()].filter(Boolean));
  const other = a.identifiers.filter((i) => !own.has(i.toLowerCase()));
  if (other.length) return other;
  return a.identifiers.length > 1 ? a.identifiers : [];
}

export const identifierLabel = (id: string) => (isApplePrivateRelay(id) ? "Apple Relay Adresse" : id);

// ---- review queue ---------------------------------------------------------------------------

/** Strongest first. */
export function reviewOrder(list: Account[]): Account[] {
  return [...list].sort((x, y) =>
    y.confidence - x.confidence || topStrength(y) - topStrength(x) || x.name.localeCompare(y.name, "de"));
}

export const toReview = (accounts: Account[]) => accounts.filter((a) => a.status === "Discovered" && !isWeak(a));

// ---- first run ----------------------------------------------------------------------------

export function setupState(snap: Snapshot) {
  const hasSource = snap.mailboxes.length > 0 || snap.runs.length > 0 || snap.accounts.length > 0;
  const review = toReview(snap.accounts).length;
  const weak = snap.accounts.filter(isWeak).length;
  const confirmed = snap.accounts.filter((a) => a.status === "Confirmed").length;
  const decided = snap.accounts.some((a) => a.status !== "Discovered");
  return { hasSource, review, weak, confirmed, decided, stats: discoveryStats(snap.accounts) };
}
