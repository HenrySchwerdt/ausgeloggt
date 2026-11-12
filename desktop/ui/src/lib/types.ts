// Mirrors crates/spurlos-core/src/model.rs (serde default representation).
// Unit variants -> string, struct/newtype variants -> { Variant: payload }.

export type Uuid = string;
/** RFC 3339 timestamp (chrono DateTime<Utc>). */
export type IsoDate = string;

export type UnsubscribeMethod =
  | { OneClick: { url: string } }
  | { Mailto: { address: string; subject: string | null; body: string | null } }
  | { Link: { url: string } };

export type MailCategory = "Marketing" | "Newsletter" | "Social" | "Transactional" | "Spam" | "Unknown";
export type CategorySource = "Rules" | "Ai" | "User";
export type SubscriptionStatus = "Active" | "Keep" | "Unsubscribed" | "StillReceiving" | "Failed";

export interface UnsubscribeAttempt {
  at: IsoDate;
  method: UnsubscribeMethod;
  ok: boolean;
  message: string;
}

export interface Subscription {
  id: Uuid;
  mailbox: string;
  list_key: string;
  domain: string;
  name: string;
  sender: string;
  unsubscribe: UnsubscribeMethod[];
  message_count: number;
  frequency_per_month: number;
  first_seen: IsoDate | null;
  last_seen: IsoDate | null;
  sample_subjects: string[];
  category: MailCategory;
  category_source: CategorySource;
  status: SubscriptionStatus;
  history: UnsubscribeAttempt[];
}

export type EvidenceKind =
  | "Welcome" | "Verification" | "PasswordReset" | "LoginAlert" | "SsoGrant" | "SavedPassword"
  | "DataExport" | "Breach" | "Receipt" | "Newsletter" | "OtherMail" | "Manual";

export type PasswordFormat =
  | "Chrome" | "Firefox" | "Safari" | "Edge" | "Bitwarden" | "OnePassword" | "KeePass"
  | "LastPass" | "Proton" | "Dashlane" | "Generic";

export type SourceKind =
  | { Mail: { mailbox: string } }
  | { PasswordExport: { format: PasswordFormat } }
  | "GoogleTakeout"
  | "FacebookExport"
  | "GenericList"
  | "GitHub"
  | "HaveIBeenPwned"
  | "Manual";

export type SsoProvider =
  | "Google" | "Apple" | "Microsoft" | "GitHub" | "Facebook" | "Discord" | "X" | "LinkedIn" | "Amazon"
  | { Other: string };

export type AccountStatus =
  | "Discovered" | "Confirmed" | "Keep" | "DeletionPending" | "Deleted" | "NotAnAccount" | "Ignored";

export type DeletionMethod =
  | { DirectLink: { url: string } }
  | { GdprEmail: { to: string } }
  | { SsoRevoked: { provider: SsoProvider } }
  | "Confirmed"
  | "Other";

export interface DeletionStep {
  at: IsoDate;
  method: DeletionMethod;
  note: string;
  deadline: IsoDate | null;
}

export interface Evidence {
  kind: EvidenceKind;
  source: SourceKind;
  summary: string;
  date: IsoDate | null;
  source_ref: string | null;
}

export interface Account {
  id: Uuid;
  domain: string;
  name: string;
  identifiers: string[];
  sso: SsoProvider[];
  evidence: Evidence[];
  status: AccountStatus;
  confidence: number;
  first_seen: IsoDate | null;
  last_seen: IsoDate | null;
  tags: string[];
  notes: string;
  deletion: DeletionStep[];
  created_at: IsoDate;
  updated_at: IsoDate;
}

export interface SourceRun {
  id: Uuid;
  source: SourceKind;
  started_at: IsoDate;
  finished_at: IsoDate | null;
  items_scanned: number;
  findings: number;
  new_accounts: number;
  error: string | null;
}

export interface MailboxConfig {
  id: Uuid;
  address: string;
  imap_host: string;
  imap_port: number;
  username: string;
  /** Never sent to the UI by the real backend (redacted). */
  password: string;
  folders: string[];
  progress: { folder: string; uid_validity: number; last_uid: number }[];
  last_scan: IsoDate | null;
}

export type AiProvider = "Off" | "Ollama" | "Anthropic";

export interface AiSettings {
  provider: AiProvider;
  ollama_url: string;
  ollama_model: string;
  anthropic_api_key: string;
  anthropic_model: string;
}

export interface Settings {
  auto_lock_minutes: number;
  min_confidence: number;
  full_name: string;
  letter_language: "de" | "en";
  allow_catalog_updates: boolean;
  dark_mode: boolean;
  ai: AiSettings;
  fetch_logos: boolean;
}

/** What the UI holds after unlocking (VaultData minus secrets). */
export interface Snapshot {
  schema: number;
  accounts: Account[];
  runs: SourceRun[];
  mailboxes: MailboxConfig[];
  settings: Settings;
  subscriptions: Subscription[];
}

// ---- helpers for the tagged unions ----

export function sourceTag(s: SourceKind): string {
  return typeof s === "string" ? s : Object.keys(s)[0]!;
}
export function ssoLabel(p: SsoProvider): string {
  return typeof p === "string" ? p : p.Other;
}
export function ssoKey(p: SsoProvider): string {
  return typeof p === "string" ? p : `Other:${p.Other}`;
}
export function methodTag(m: DeletionMethod): string {
  return typeof m === "string" ? m : Object.keys(m)[0]!;
}

export const ACCOUNT_STATUS_LABEL: Record<AccountStatus, string> = {
  Discovered: "Neu entdeckt",
  Confirmed: "Bestätigt",
  Keep: "Behalten",
  DeletionPending: "Löschung läuft",
  Deleted: "Gelöscht",
  NotAnAccount: "Kein Account",
  Ignored: "Ignoriert",
};

export const EVIDENCE_LABEL: Record<EvidenceKind, string> = {
  Welcome: "Willkommensmail",
  Verification: "E-Mail-Bestätigung",
  PasswordReset: "Passwort-Reset",
  LoginAlert: "Login-Hinweis",
  SsoGrant: "SSO-Verknüpfung",
  SavedPassword: "Gespeichertes Passwort",
  DataExport: "Datenexport",
  Breach: "Datenleck",
  Receipt: "Bestellung/Rechnung",
  Newsletter: "Newsletter",
  OtherMail: "Sonstige Mail",
  Manual: "Manuell",
};

export const PASSWORD_FORMAT_LABEL: Record<PasswordFormat, string> = {
  Chrome: "Chrome", Firefox: "Firefox", Safari: "Safari / Apple Passwörter", Edge: "Edge",
  Bitwarden: "Bitwarden", OnePassword: "1Password", KeePass: "KeePass", LastPass: "LastPass",
  Proton: "Proton Pass", Dashlane: "Dashlane", Generic: "CSV",
};

export function sourceLabel(s: SourceKind): string {
  if (typeof s === "string") {
    return { GoogleTakeout: "Google Takeout", FacebookExport: "Facebook-Export", GenericList: "Liste",
      GitHub: "GitHub", HaveIBeenPwned: "Have I Been Pwned", Manual: "Manuell" }[s];
  }
  if ("Mail" in s) return `Postfach ${s.Mail.mailbox}`;
  return `Passwörter (${PASSWORD_FORMAT_LABEL[s.PasswordExport.format]})`;
}

/** Short source group label for filters. */
export function sourceGroup(s: SourceKind): string {
  if (typeof s === "string") return sourceLabel(s);
  if ("Mail" in s) return "Postfach";
  return "Passwort-Export";
}

export const SUBSCRIPTION_STATUS_LABEL: Record<SubscriptionStatus, string> = {
  Active: "Aktiv", Keep: "Behalten", Unsubscribed: "Abgemeldet",
  StillReceiving: "Kommt trotzdem", Failed: "Fehlgeschlagen",
};
export const MAIL_CATEGORY_LABEL: Record<MailCategory, string> = {
  Marketing: "Werbung", Newsletter: "Newsletter", Social: "Social",
  Transactional: "Wichtig/Transaktional", Spam: "Spam", Unknown: "Unbekannt",
};

export function connectionsUrl(p: SsoProvider): string | null {
  if (typeof p !== "string") return null;
  const m: Record<string, string> = {
    Google: "https://myaccount.google.com/connections",
    Apple: "https://account.apple.com/account/manage",
    Microsoft: "https://account.live.com/consent/Manage",
    GitHub: "https://github.com/settings/applications",
    Facebook: "https://www.facebook.com/settings?tab=applications",
    Discord: "https://discord.com/settings/authorized-apps",
    X: "https://x.com/settings/connected_apps",
    LinkedIn: "https://www.linkedin.com/mypreferences/d/permitted-services",
    Amazon: "https://www.amazon.com/ap/adam",
  };
  return m[p] ?? null;
}
