import { Inbox, Mail, Settings, Trash2 } from "lucide-react";
import { EmptyState, PageHeader } from "@/components/common";
import { Button } from "@/components/ui/button";
import { navigate } from "@/lib/router";

function Placeholder({ title, description, icon, text }: { title: string; description: string; icon: typeof Mail; text: string }) {
  return (
    <div className="mx-auto flex h-full w-full max-w-[1120px] flex-col px-7 py-7">
      <PageHeader title={title} description={description} />
      <EmptyState
        icon={icon}
        title="Kommt bald"
        description={text}
        className="flex-1"
        action={<Button variant="secondary" onClick={() => navigate({ page: "accounts" })}>Zu den Accounts</Button>}
      />
    </div>
  );
}

export const NewsletterPage = () => (
  <Placeholder icon={Mail} title="Newsletter" description="Absender finden und mit einem Klick abmelden." text="Hier verwaltest du alle Newsletter und Mailinglisten deiner Postfächer." />
);
export const DeletionsPage = () => (
  <Placeholder icon={Trash2} title="Löschungen" description="Laufende Löschungen, Fristen und Anträge." text="Hier behältst du Löschlinks, DSGVO-Anträge und Fristen im Blick." />
);
export const SourcesPage = () => (
  <Placeholder icon={Inbox} title="Quellen" description="Postfächer, Passwort-Exporte und Datenexporte." text="Hier verbindest du Postfächer und importierst Passwort-Exporte, um Accounts zu finden." />
);
export const SettingsPage = () => (
  <Placeholder icon={Settings} title="Einstellungen" description="Tresor, Briefe, KI und Darstellung." text="Hier stellst du Auto-Sperre, Briefvorlagen und KI-Einordnung ein." />
);
