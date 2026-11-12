import { useEffect } from "react";
import { Loader2 } from "lucide-react";
import { useStore } from "@/lib/store";
import { useRoute } from "@/lib/router";
import { Sidebar } from "@/components/sidebar";
import { Titlebar } from "@/components/titlebar";
import { CommandPalette } from "@/components/palette";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { UnlockScreen } from "@/screens/unlock";
import { Dashboard } from "@/screens/dashboard";
import { AccountsPage } from "@/screens/accounts";
import { DeletionsPage, NewsletterPage, SettingsPage, SourcesPage } from "@/screens/placeholders";

function Page() {
  const route = useRoute();
  switch (route.page) {
    case "dashboard": return <div className="h-full overflow-y-auto"><Dashboard /></div>;
    case "accounts": return <AccountsPage />;
    case "newsletter": return <NewsletterPage />;
    case "deletions": return <DeletionsPage />;
    case "sources": return <SourcesPage />;
    case "settings": return <SettingsPage />;
  }
}

export default function App() {
  const phase = useStore((s) => s.phase);
  const boot = useStore((s) => s.boot);

  useEffect(() => { void boot(); }, [boot]);

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
        </div>
      )}
      <Toaster />
    </TooltipProvider>
  );
}
