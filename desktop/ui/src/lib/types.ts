// Types come from ipc.ts (mirror of the Rust DTOs). This file adds labels and small helpers.
import type {
  AccountStatus, BillingInterval, DeletionMethod, Difficulty, EvidenceKind, LetterKind, MailCategory,
  MailboxDto, PasswordFormat, PaymentMethod, RequestKind, RequestStatus, RiskReason, SettingsDto, SnapshotDto,
  SourceKind, SsoProvider, SubscriptionStatus,
} from "./ipc";

export type {
  Uuid, IsoDate, JobId, ErrorCode, IpcErrorPayload,
  UnsubscribeMethod, MailCategory, CategorySource, SubscriptionStatus, UnsubscribeAttempt, MailRef, Subscription,
  EvidenceKind, PasswordFormat, SsoProvider, SourceKind, AccountStatus, DeletionMethod, DeletionStep, Evidence,
  PaymentKind, PaymentMethod, PaymentRecord, BreachRecord, SecurityInfo, Account, SourceRun, FolderProgress,
  RequestKind, RequestChannel, RequestStatus, DataRequest, AiProvider, Identity, AiSettingsDto, SettingsPatch,
  BillingInterval, BillingSummary, RiskReason, RiskAssessment, PrivacyScore, AliasAdvice, InsightsDto,
  Difficulty, DeletionInfo, TwoFactorInfo, CatalogInfo, OrgKind, Organization, Authority, HelpLink, AliasService,
  ReferenceData, ProviderPreset, MailboxDraft, MailboxPatch, ImportDetection, ScreenshotEntry, ScreenshotReadPayload,
  ScreenshotCommitResult, LetterKind, Party, LetterArgs, LetterPreview, NewRequest, RequestUpdate, JobKind, JobItem,
  JobProgress, JobDone, RunningJob, ScanDonePayload, ImportDonePayload, UnsubscribeDonePayload, ClassifyDonePayload,
  BreachDonePayload, CatalogRefreshDonePayload, VaultStatus, AiModels,
} from "./ipc";

export type Snapshot = SnapshotDto;
export type Settings = SettingsDto;
export type Mailbox = MailboxDto;

// ---- tagged union helpers ----

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

// ---- German labels (no dashes, no hyphenated compounds) ----

export const ACCOUNT_STATUS_LABEL: Record<AccountStatus, string> = {
  Discovered: "Zu prüfen",
  Confirmed: "Bestätigt",
  Keep: "Behalten",
  DeletionPending: "Löschung läuft",
  Deleted: "Gelöscht",
  NotAnAccount: "Kein Account",
  Ignored: "Ausgeblendet",
};

export const EVIDENCE_LABEL: Record<EvidenceKind, string> = {
  Welcome: "Willkommensmail",
  Verification: "Mailbestätigung",
  PasswordReset: "Passwort zurückgesetzt",
  LoginAlert: "Anmeldehinweis",
  SsoGrant: "Anmeldung über Drittanbieter",
  SavedPassword: "Gespeichertes Passwort",
  DataExport: "Datenexport",
  Breach: "Datenleck",
  Receipt: "Bestellung oder Rechnung",
  Newsletter: "Newsletter",
  OtherMail: "Sonstige Mail",
  Manual: "Manuell",
  Payment: "Zahlung",
  TwoFactor: "Zweiter Faktor",
  Screenshot: "Screenshot",
};

export const PASSWORD_FORMAT_LABEL: Record<PasswordFormat, string> = {
  Chrome: "Chrome", Firefox: "Firefox", Safari: "Apple Passwörter", Edge: "Edge",
  Bitwarden: "Bitwarden", OnePassword: "1Password", KeePass: "KeePass", LastPass: "LastPass",
  Proton: "Proton Pass", Dashlane: "Dashlane", Generic: "CSV",
};

export function sourceLabel(s: SourceKind): string {
  if (typeof s === "string") {
    return { GoogleTakeout: "Google Takeout", FacebookExport: "Facebook Export", GenericList: "Liste",
      GitHub: "GitHub", HaveIBeenPwned: "Have I Been Pwned", Manual: "Manuell" }[s];
  }
  if ("Mail" in s) return s.Mail.mailbox;
  if ("Screenshot" in s) return s.Screenshot.provider ? `Screenshot ${ssoLabel(s.Screenshot.provider)}` : "Screenshot";
  return `Passwörter ${PASSWORD_FORMAT_LABEL[s.PasswordExport.format]}`;
}

/** Short source group label for filters. */
export function sourceGroup(s: SourceKind): string {
  if (typeof s === "string") return sourceLabel(s);
  if ("Mail" in s) return "Postfach";
  if ("Screenshot" in s) return "Screenshot";
  return "Passwortexport";
}

export const SUBSCRIPTION_STATUS_LABEL: Record<SubscriptionStatus, string> = {
  Active: "Aktiv", Keep: "Behalten", Unsubscribed: "Abgemeldet",
  StillReceiving: "Kommt trotzdem", Failed: "Fehlgeschlagen",
};
export const MAIL_CATEGORY_LABEL: Record<MailCategory, string> = {
  Marketing: "Werbung", Newsletter: "Newsletter", Social: "Soziale Netze",
  Transactional: "Wichtig", Spam: "Spam", Unknown: "Unbekannt",
};

export const DIFFICULTY_LABEL: Record<Difficulty, { label: string; dot: string }> = {
  Easy: { label: "Einfach", dot: "bg-s-green" },
  Medium: { label: "Mittel", dot: "bg-s-yellow" },
  Hard: { label: "Aufwendig", dot: "bg-s-red" },
  Impossible: { label: "Nur per Brief", dot: "bg-s-red" },
  Unknown: { label: "Unbekannt", dot: "bg-s-gray" },
};

export const LETTER_LABEL: Record<LetterKind, string> = {
  Erasure: "Löschantrag",
  Access: "Auskunft",
  ObjectionAndAccess: "Widerspruch und Auskunft",
  Cancellation: "Kündigung",
  CeaseAndDesist: "Unterlassung",
  CreditBureauAccess: "Selbstauskunft",
  AddressBrokerObjection: "Werbewiderspruch",
  Complaint: "Beschwerde",
  Reminder: "Erinnerung",
};

export const REQUEST_KIND_LABEL: Record<RequestKind, string> = {
  Erasure: "Löschantrag",
  Access: "Auskunft",
  Objection: "Widerspruch",
  Cancellation: "Kündigung",
  CeaseAndDesist: "Unterlassung",
  Complaint: "Beschwerde",
  Reminder: "Erinnerung",
  CreditBureauAccess: "Selbstauskunft",
  AdvertisingObjection: "Werbewiderspruch",
};

export const REQUEST_STATUS_LABEL: Record<RequestStatus, string> = {
  Draft: "Entwurf", Sent: "Gesendet", Answered: "Beantwortet", Done: "Erledigt", Rejected: "Abgelehnt",
};

export const INTERVAL_LABEL: Record<BillingInterval, string> = {
  Weekly: "wöchentlich", BiWeekly: "alle zwei Wochen", BiMonthly: "alle zwei Monate", FourMonthly: "alle vier Monate", Monthly: "monatlich", Quarterly: "vierteljährlich", HalfYearly: "halbjährlich", Yearly: "jährlich",
};

export const RISK_REASON_LABEL: Record<RiskReason, string> = {
  PaymentDataStored: "Zahlungsdaten gespeichert",
  ActiveSubscription: "Aktives Abo",
  ServiceBreached: "Dienst hatte ein Datenleck",
  IdentifierInBreach: "Deine Adresse ist in einem Leak",
  PasswordPwned: "Passwort geleakt",
  PasswordReused: "Passwort mehrfach genutzt",
  No2faButSupported: "2FA fehlt",
  InactiveForYears: "Seit Jahren ungenutzt",
  PhoneLogin: "Anmeldung per Telefonnummer",
  ForeignRecoveryAddress: "Fremde Wiederherstellungsadresse",
  SsoLinked: "Über Drittanbieter angemeldet",
};

export function paymentMethodLabel(m: PaymentMethod): string {
  if (typeof m === "string") {
    return { PayPal: "PayPal", ApplePay: "Apple Pay", GooglePay: "Google Pay", AmazonPay: "Amazon Pay", Klarna: "Klarna", Invoice: "Rechnung" }[m];
  }
  if ("Card" in m) return `${m.Card.brand} •••• ${m.Card.last4}`;
  if ("Sepa" in m) return m.Sepa.iban_last4 ? `Lastschrift •••• ${m.Sepa.iban_last4}` : "Lastschrift";
  return m.Other;
}
export function paymentMethodKey(m: PaymentMethod): string {
  return typeof m === "string" ? m : JSON.stringify(m);
}

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
