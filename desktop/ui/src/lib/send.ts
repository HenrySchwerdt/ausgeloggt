// Shared logic for sending letters from a mailbox: which mailbox, quota, and what to do after a send.
import { useEffect, useState } from "react";
import { api } from "./api";
import { useAsync } from "./hooks";
import { useStore } from "./store";
import type { Mailbox, Snapshot } from "./types";

/** Mailboxes that can send (a password is stored). */
export function sendingMailboxes(snap: Snapshot): Mailbox[] {
  return snap.mailboxes.filter((m) => m.has_password);
}

/** German names of the sender fields that are still empty (the letter needs them). */
export function missingSenderFields(snap: Snapshot): string[] {
  const id = snap.settings.identity;
  const out: string[] = [];
  if (!snap.settings.full_name.trim()) out.push("Name");
  if (!id.street.trim()) out.push("Straße");
  if (!id.postal_code.trim()) out.push("Postleitzahl");
  if (!id.city.trim()) out.push("Ort");
  return out;
}

let preferred: string | null = null;

/** The mailbox to send from. The last choice is remembered for this session. */
export function useSendMailbox(): { mailboxes: Mailbox[]; mailbox: Mailbox | null; setMailboxId(id: string): void } {
  const snap = useStore((s) => s.snap)!;
  const mailboxes = sendingMailboxes(snap);
  const [id, setId] = useState<string | null>(preferred);
  const mailbox = mailboxes.find((m) => m.id === id) ?? mailboxes[0] ?? null;
  useEffect(() => { if (mailbox) preferred = mailbox.id; }, [mailbox]);
  return { mailboxes, mailbox, setMailboxId: (x) => { preferred = x; setId(x); } };
}

export interface Quota { remaining: number; cap: number; gapSeconds: number }

/** Remaining mails today for a mailbox (a dry run with no items). Reloads after requests change. */
export function useQuota(mailboxId: string | null) {
  const requests = useStore((s) => s.snap?.requests.length ?? 0);
  return useAsync<Quota | null>(async () => {
    if (!mailboxId) return null;
    const c = await api.sending.lettersSendCheck([], mailboxId);
    return { remaining: c.remainingToday, cap: c.dailyCap, gapSeconds: c.gapSeconds };
  }, [mailboxId, requests]);
}

/** After a sent erasure the account counts as "Löschung läuft" and carries the deadline. */
export async function markErasureSent(accountId: string, to: string): Promise<void> {
  await api.accounts.addDeletionStep(accountId, { GdprEmail: { to } }, "Löschantrag gesendet.").catch(() => {});
}
