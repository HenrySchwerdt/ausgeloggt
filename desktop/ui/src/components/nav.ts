import {
  CreditCard, Eraser, FileText, Gauge, Inbox, Mail, Settings, ShieldAlert, Users, type LucideIcon,
} from "lucide-react";
import type { Page } from "@/lib/router";

export const PAGE_TITLE: Record<Page, string> = {
  dashboard: "Übersicht",
  accounts: "Accounts",
  cleanup: "Löschen und Kündigen",
  newsletter: "Newsletter",
  billing: "Abos und Zahlungen",
  security: "Sicherheit",
  requests: "Anträge",
  sources: "Quellen",
  settings: "Einstellungen",
};

export interface NavItem { page: Page; label: string; icon: LucideIcon }

export const NAV: NavItem[] = [
  { page: "dashboard", label: "Übersicht", icon: Gauge },
  { page: "accounts", label: "Accounts", icon: Users },
  { page: "cleanup", label: "Löschen und Kündigen", icon: Eraser },
  { page: "newsletter", label: "Newsletter", icon: Mail },
  { page: "billing", label: "Abos und Zahlungen", icon: CreditCard },
  { page: "security", label: "Sicherheit", icon: ShieldAlert },
  { page: "requests", label: "Anträge", icon: FileText },
  { page: "sources", label: "Quellen", icon: Inbox },
  { page: "settings", label: "Einstellungen", icon: Settings },
];
