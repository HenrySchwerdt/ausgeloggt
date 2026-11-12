import { useEffect } from "react";
import { Loader2, Upload } from "lucide-react";
import { useStore } from "@/lib/store";
import { navigate, useRoute } from "@/lib/router";
import { onFileDrop } from "@/lib/platform";
import { useDrop } from "@/lib/drop";
import { forceMock } from "@/lib/api";
import { Sidebar } from "@/components/sidebar";
import { Titlebar } from "@/components/titlebar";
import { CommandPalette } from "@/components/palette";
import { ConfirmHost } from "@/components/confirm";
import { LetterHost, openLetter } from "@/components/letter-dialog";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { UnlockScreen } from "@/screens/unlock";
import { Dashboard } from "@/screens/dashboard";
import { AccountsPage } from "@/screens/accounts";
import { CleanupPage } from "@/screens/cleanup";
import { NewsletterPage } from "@/screens/newsletter";
import { BillingPage } from "@/screens/billing";
import { SecurityPage } from "@/screens/security";
import { RequestsPage } from "@/screens/requests";
import { SourcesPage } from "@/screens/sources";
import { SettingsPage } from "@/screens/settings";

function Page() {
  const route = useRoute();
  switch (route.page) {
    case "dashboard": return <Dashboard />;
    case "accounts": return <AccountsPage />;
    case "cleanup": return <CleanupPage tidy={!!route.tidy} />;
    case "newsletter": return <NewsletterPage />;
    case "billing": return <BillingPage />;
    case "security": return <SecurityPage />;
    case "requests": return <RequestsPage />;
    case "sources": return <SourcesPage />;
    case "settings": return <SettingsPage section={route.section} />;
  }
}

function DropLayer() {
  const hover = useStore((s) => s.dropHover);
  if (!hover) return null;
  return (
    <div className="anim-fade pointer-events-none fixed inset-0 z-[60] flex items-center justify-center bg-[var(--overlay)]">
      <div className="flex flex-col items-center gap-3 rounded-lg border border-dashed border-border-strong bg-surface px-12 py-10 text-base text-fg">
        <Upload className="size-6 text-fg-muted" strokeWidth={1.5} />
        Dateien zum Importieren hier ablegen
      </div>
    </div>
  );
}

export default function App() {
  const phase = useStore((s) => s.phase);
  const boot = useStore((s) => s.boot);
  const setDropHover = useStore((s) => s.setDropHover);

  useEffect(() => { void boot(); }, [boot]);

  useEffect(() => {
    if (phase !== "ready") return;
    let un: (() => void) | undefined;
    let dead = false;
    void onFileDrop((paths) => { useDrop.getState().push(paths); navigate({ page: "sources" }); }, setDropHover)
      .then((u) => { if (dead) u(); else un = u; });
    return () => { dead = true; un?.(); };
  }, [phase, setDropHover]);

  // Demo hook for screenshots: ?mock&letter=Erasure&select=adobe
  useEffect(() => {
    if (phase !== "ready" || !forceMock) return;
    const q = new URLSearchParams(location.search);
    const kind = q.get("letter");
    const sel = q.get("select")?.toLowerCase();
    const acc = useStore.getState().snap?.accounts.find((x) => x.domain === sel || x.name.toLowerCase() === sel);
    if (kind) openLetter({ args: { kind: kind as never, accountId: acc?.id }, target: acc?.name ?? "Test", domain: acc?.domain });
  }, [phase]);

  return (
    <TooltipProvider>
      {phase === "boot" && (
        <div className="flex h-full items-center justify-center bg-bg text-fg-faint"><Loader2 className="size-4 animate-spin" /></div>
      )}
      {phase === "locked" && <UnlockScreen />}
      {phase === "ready" && (
        <div className="flex h-full bg-bg">
          <Sidebar />
          <div className="flex min-w-0 flex-1 flex-col">
            <Titlebar />
            <main className="relative min-h-0 flex-1"><Page /></main>
          </div>
          <CommandPalette />
          <LetterHost />
          <DropLayer />
        </div>
      )}
      <ConfirmHost />
      <Toaster />
    </TooltipProvider>
  );
}
