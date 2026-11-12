import { toast } from "sonner";
import { api } from "./api";
import { openUrl } from "./platform";
import type { Account } from "./types";
import { errorMessage } from "./utils";

/** Open the service's deletion page and record it: the account becomes "Löschung läuft". Undo restores the old status. */
export async function openDeletionPage(a: Account, url: string): Promise<void> {
  const prev = a.status;
  try {
    await openUrl(url);
    await api.accounts.addDeletionStep(a.id, { DirectLink: { url } }, "Löschseite geöffnet");
    toast(`${a.name}: Löschung läuft`, { duration: 6000, action: { label: "Rückgängig", onClick: () => void api.accounts.setStatus([a.id], prev) } });
  } catch (e) { toast.error(errorMessage(e)); }
}
