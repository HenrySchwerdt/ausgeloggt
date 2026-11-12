import { Gauge, Mail, Settings, Trash2, Users, Inbox, type LucideIcon } from "lucide-react";
import type { Page } from "@/lib/router";

export const PAGE_TITLE: Record<Page, string> = {
  dashboard: "Übersicht",
  accounts: "Accounts",
  newsletter: "Newsletter",
  deletions: "Löschungen",
  sources: "Quellen",
  settings: "Einstellungen",
};

export interface NavItem { page: Page; label: string; icon: LucideIcon }

export const NAV: NavItem[] = [
  { page: "dashboard", label: "Übersicht", icon: Gauge },
  { page: "accounts", label: "Accounts", icon: Users },
  { page: "newsletter", label: "Newsletter", icon: Mail },
  { page: "deletions", label: "Löschungen", icon: Trash2 },
  { page: "sources", label: "Quellen", icon: Inbox },
  { page: "settings", label: "Einstellungen", icon: Settings },
];
