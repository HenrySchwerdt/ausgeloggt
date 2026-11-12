import type { Account, Snapshot } from "./types";
import { daysFromNow } from "./utils";

/** Accounts that are hidden from the default views. */
export const isHidden = (a: Account) => a.status === "NotAnAccount" || a.status === "Ignored";

export interface Deadline {
  account: Account;
  at: string;
  days: number;
  kind: "gdpr" | "link";
}

export function deadlines(snap: Snapshot): Deadline[] {
  const out: Deadline[] = [];
  for (const a of snap.accounts) {
    if (a.status !== "DeletionPending") continue;
    const open = a.deletion.filter((d) => d.deadline);
    const last = open[open.length - 1];
    if (last?.deadline) {
      out.push({
        account: a, at: last.deadline, days: daysFromNow(last.deadline),
        kind: typeof last.method !== "string" && "GdprEmail" in last.method ? "gdpr" : "link",
      });
    }
  }
  return out.sort((x, y) => x.days - y.days);
}

export function stats(snap: Snapshot) {
  const by = (s: Account["status"]) => snap.accounts.filter((a) => a.status === s).length;
  const visible = snap.accounts.filter((a) => !isHidden(a));
  const total = visible.length;
  const deleted = by("Deleted");
  const kept = by("Keep");
  const pending = by("DeletionPending");
  const open = by("Discovered") + by("Confirmed");
  const overdue = deadlines(snap).filter((d) => d.days < 0).length;
  const sso = visible.filter((a) => a.sso.length > 0 && a.status !== "Deleted").length;
  const subsActive = snap.subscriptions.filter((s) => s.status === "Active" || s.status === "StillReceiving").length;
  return { total, deleted, kept, pending, open, overdue, sso, discovered: by("Discovered"), subsActive };
}
