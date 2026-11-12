import {
  CreditCard, Eraser, FileText, Gauge, Inbox, ListChecks, Mail, Settings, ShieldAlert, Users, type LucideIcon,
} from "lucide-react";
import type { Page } from "@/lib/router";

export const PAGE_TITLE: Record<Page, string> = {
  dashboard: "Übersicht",
  review: "Prüfen",
  accounts: "Accounts",
  cleanup: "Aufräumen",
  newsletter: "Newsletter",
  billing: "Abos und Zahlungen",
  security: "Sicherheit",
  requests: "Anträge",
  sources: "Quellen",
  settings: "Einstellungen",
};

export interface NavItem { page: Page; label: string; icon: LucideIcon; tip?: string }

/** Main path first (Übersicht, Prüfen, Accounts, Aufräumen), then tools, then setup. */
export const NAV_GROUPS: { label?: string; items: NavItem[] }[] = [
  {
    items: [
      { page: "dashboard", label: "Übersicht", icon: Gauge },
      { page: "review", label: "Prüfen", icon: ListChecks },
      { page: "accounts", label: "Accounts", icon: Users },
      { page: "cleanup", label: "Aufräumen", icon: Eraser },
    ],
  },
  {
    label: "Weitere Werkzeuge",
    items: [
      { page: "newsletter", label: "Newsletter", icon: Mail },
      { page: "billing", label: "Abos und Zahlungen", icon: CreditCard },
      { page: "security", label: "Sicherheit", icon: ShieldAlert },
      { page: "requests", label: "Anträge", icon: FileText },
    ],
  },
  {
    items: [
      { page: "sources", label: "Quellen", icon: Inbox },
      { page: "settings", label: "Einstellungen", icon: Settings },
    ],
  },
];

export const NAV: NavItem[] = NAV_GROUPS.flatMap((g) => g.items);
