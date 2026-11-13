import { toast } from "sonner";
import { api } from "./api";
import { openUrl } from "./platform";
import type { Account } from "./types";
import { errorMessage } from "./utils";

/** Open the service's deletion page. Only after the user says they filed the request does the account become "Löschung läuft". */
export async function openDeletionPage(a: Account, url: string): Promise<void> {
  const prev = a.status;
  try {
    await openUrl(url);
    toast(`${a.name}: Löschseite geöffnet`, {
      description: "Hast du die Löschung beim Anbieter beantragt?", duration: 30000,
      action: { label: "Ja, beantragt", onClick: () => void api.accounts.addDeletionStep(a.id, { DirectLink: { url } }, "Löschseite geöffnet").then(() => toast(`${a.name}: Löschung läuft`, { duration: 6000, action: { label: "Rückgängig", onClick: () => void api.accounts.setStatus([a.id], prev) } })) },
    });
  } catch (e) { toast.error(errorMessage(e)); }
}
