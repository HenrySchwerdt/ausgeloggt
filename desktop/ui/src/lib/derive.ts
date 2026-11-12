import type { Account, DataRequest } from "./types";
import { daysFromNow } from "./utils";

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
