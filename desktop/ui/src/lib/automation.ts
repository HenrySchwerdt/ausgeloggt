// Pure helpers for sending and automation. No backend imports, so the demo backend can use them too.
import type { Account, DataRequest, Mailbox, Snapshot } from "./types";

export const EVERY_LABEL: Record<number, string> = { 1: "Täglich", 7: "Wöchentlich", 14: "Alle 2 Wochen", 30: "Monatlich" };

const AUTOMATABLE = (a: Account) => !["Keep", "Deleted", "NotAnAccount", "Ignored"].includes(a.status);
const same = (x: string, y: string) => x.trim().toLowerCase() === y.trim().toLowerCase();

/** Same rule as the backend: an identifier is the mailbox address, or the only mailbox takes accounts without a foreign address. */
export function belongsToMailbox(snap: Snapshot, a: Account, m: Mailbox): boolean {
  if (a.identifiers.some((i) => same(i, m.address))) return true;
  return snap.mailboxes.length === 1 && !a.identifiers.some((i) => snap.mailboxes.some((x) => same(x.address, i)));
}

export const alreadyRequested = (snap: Snapshot, kind: DataRequest["kind"], accountId: string) =>
  snap.requests.some((r) => r.account_id === accountId && r.kind === kind && r.status !== "Draft");

/** What an automation run for this mailbox would send right now. */
export function automationPreview(snap: Snapshot, m: Mailbox, now = Date.now()) {
  const queue = snap.accounts.filter((a) => a.queued_for_deletion && AUTOMATABLE(a) && belongsToMailbox(snap, a, m) && !alreadyRequested(snap, "Erasure", a.id));
  const overdue = (r: DataRequest) => r.status === "Sent" && !!r.deadline && new Date(r.deadline).getTime() < now;
  const reminders = snap.requests.filter((r) =>
    r.mailbox_id === m.id && r.kind !== "Reminder" && r.kind !== "Complaint" && r.channel === "Email" && overdue(r)
    && !snap.requests.some((x) => x.kind === "Reminder" && x.parent_id === r.id));
  const complaints = snap.requests.filter((rem) => rem.kind === "Reminder" && rem.mailbox_id === m.id && overdue(rem)).map((rem) => rem.parent_id)
    .filter((pid): pid is string => !!pid && snap.requests.some((p) => p.id === pid && p.status === "Sent") && !snap.requests.some((c) => c.kind === "Complaint" && c.parent_id === pid))
    .filter((pid, i, l) => l.indexOf(pid) === i);
  return { queue, reminders, complaints };
}

/** German list of names: "A, B und C", longer lists end with "und n weitere". */
export function nameList(names: string[], max = 4): string {
  if (names.length <= max) return names.length > 1 ? `${names.slice(0, -1).join(", ")} und ${names[names.length - 1]}` : names[0] ?? "";
  return `${names.slice(0, max).join(", ")} und ${names.length - max} weitere`;
}

/** "etwa 4 Minuten" for n mails with a gap between them. */
export function sendDuration(n: number, gapSeconds: number): string {
  const s = Math.max(0, n - 1) * gapSeconds + n * 3;
  if (s < 45) return "unter einer Minute";
  const m = Math.round(s / 60);
  return m < 60 ? `etwa ${m} ${m === 1 ? "Minute" : "Minuten"}` : `etwa ${Math.round(m / 6) / 10} Stunden`.replace(".", ",");
}

export const mailsLeft = (n: number) => `Heute noch ${n.toLocaleString("de-DE")} ${n === 1 ? "Mail" : "Mails"} möglich`;
