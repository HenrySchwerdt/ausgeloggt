// Typed client for the Tauri backend (contract: docs/IPC.md).
//
// Hand-written, no codegen: the DTO types below mirror the serde JSON produced by
// desktop/src-tauri (default serde enum representation: unit variants are strings,
// struct / newtype variants are `{ "Variant": payload }`; Option<T> is `T | null`;
// chrono timestamps are RFC 3339 strings; Uuid is a string; tuples are arrays).
//
// Conventions
// - Command names are snake_case, argument keys camelCase (Tauri default). Nested objects
//   keep their serde field names (snake_case for model types, camelCase for `MailboxDraft`,
//   `MailboxPatch` and the letter arguments, which mirror the flat command arguments).
// - Every command rejects with an `IpcError` (see below), never with a bare string.
// - Long work returns a `JobId`; progress and completion arrive as events (`onJobProgress`,
//   `onJobDone`) or through `runJob`.
// - Secrets are write-only: mailbox passwords, the Anthropic and the HIBP key never come back.
//   Read the `has_*` flags instead.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ============================================================================
// Primitives
// ============================================================================

export type Uuid = string;
/** RFC 3339 timestamp (chrono `DateTime<Utc>`). */
export type IsoDate = string;
export type JobId = string;

// ============================================================================
// Errors
// ============================================================================

export type ErrorCode =
  | "VaultLocked"
  | "WrongPassword"
  | "NotFound"
  | "Invalid"
  | "Network"
  | "Auth"
  | "Busy"
  | "Io"
  | "Internal";

export interface IpcErrorPayload {
  code: ErrorCode;
  /** German, user-facing. */
  message: string;
}

export class IpcError extends Error implements IpcErrorPayload {
  readonly code: ErrorCode;
  constructor(code: ErrorCode, message: string) {
    super(message);
    this.name = "IpcError";
    this.code = code;
  }
  static from(raw: unknown): IpcError {
    if (raw instanceof IpcError) return raw;
    if (raw && typeof raw === "object" && "code" in raw && "message" in raw) {
      const r = raw as IpcErrorPayload;
      return new IpcError(r.code, String(r.message));
    }
    return new IpcError("Internal", typeof raw === "string" ? raw : "Unbekannter Fehler");
  }
}

// ============================================================================
// Core model (crates/ausgeloggt-core/src/model.rs)
// ============================================================================

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

export interface MailRef {
  date: IsoDate | null;
  from: string;
  subject: string;
  message_id: string | null;
  raw_headers: string;
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
  post_unsubscribe_mails: MailRef[];
}

export type EvidenceKind =
  | "Welcome"
  | "Verification"
  | "PasswordReset"
  | "LoginAlert"
  | "SsoGrant"
  | "SavedPassword"
  | "DataExport"
  | "Breach"
  | "Receipt"
  | "Newsletter"
  | "OtherMail"
  | "Manual"
  | "Payment"
  | "TwoFactor"
  | "Screenshot";

export type PasswordFormat =
  | "Chrome"
  | "Firefox"
  | "Safari"
  | "Edge"
  | "Bitwarden"
  | "OnePassword"
  | "KeePass"
  | "LastPass"
  | "Proton"
  | "Dashlane"
  | "Generic";

export type SsoProvider =
  | "Google"
  | "Apple"
  | "Microsoft"
  | "GitHub"
  | "Facebook"
  | "Discord"
  | "X"
  | "LinkedIn"
  | "Amazon"
  | { Other: string };

export type SourceKind =
  | { Mail: { mailbox: string } }
  | { PasswordExport: { format: PasswordFormat } }
  | "GoogleTakeout"
  | "FacebookExport"
  | "GenericList"
  | "GitHub"
  | "HaveIBeenPwned"
  | "Manual"
  | { Screenshot: { provider: SsoProvider | null } };

export type AccountStatus =
  | "Discovered"
  | "Confirmed"
  | "Keep"
  | "DeletionPending"
  | "Deleted"
  | "NotAnAccount"
  | "Ignored";

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

export type PaymentKind =
  | "Charge"
  | "Renewal"
  | "TrialStarted"
  | "PriceChange"
  | "Refund"
  | "Cancelled"
  | "PaymentFailed";

export type PaymentMethod =
  | { Card: { brand: string; last4: string } }
  | "PayPal"
  | { Sepa: { iban_last4: string | null } }
  | "ApplePay"
  | "GooglePay"
  | "AmazonPay"
  | "Klarna"
  | "Invoice"
  | { Other: string };

export interface PaymentRecord {
  date: IsoDate | null;
  kind: PaymentKind;
  /** Minor units (cents); negative for refunds. */
  amount_minor: number | null;
  currency: string | null;
  plan: string | null;
  method: PaymentMethod | null;
  via: string | null;
  summary: string;
  source_ref: string | null;
}

export interface BreachRecord {
  name: string;
  title: string;
  /** `YYYY-MM-DD` */
  breach_date: string | null;
  data_classes: string[];
  /** true = one of the user's identifiers is confirmed in it. */
  confirmed_for_user: boolean;
}

export interface SecurityInfo {
  password_reuse_count: number | null;
  password_pwned: boolean | null;
  has_2fa: boolean | null;
  phone_login: boolean;
  breaches: BreachRecord[];
  checked_at: IsoDate | null;
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
  payments: PaymentRecord[];
  security: SecurityInfo;
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

export interface FolderProgress {
  folder: string;
  uid_validity: number;
  last_uid: number;
}

export type RequestKind =
  | "Erasure"
  | "Access"
  | "Objection"
  | "Cancellation"
  | "CeaseAndDesist"
  | "Complaint"
  | "Reminder"
  | "CreditBureauAccess"
  | "AdvertisingObjection";
export type RequestChannel = "Email" | "Post" | "WebForm";
export type RequestStatus = "Draft" | "Sent" | "Answered" | "Done" | "Rejected";

export interface DataRequest {
  id: Uuid;
  kind: RequestKind;
  target: string;
  domain: string | null;
  account_id: Uuid | null;
  subscription_id: Uuid | null;
  parent_id: Uuid | null;
  channel: RequestChannel;
  /** E-mail address or postal address (multi-line). */
  recipient: string;
  subject: string;
  body: string;
  created_at: IsoDate;
  sent_at: IsoDate | null;
  deadline: IsoDate | null;
  status: RequestStatus;
  notes: string;
}

export type AiProvider = "Off" | "Ollama" | "Anthropic";

export interface Identity {
  street: string;
  postal_code: string;
  city: string;
  country: string;
  /** ISO 3166-2, e.g. `DE-BY`; empty = derive from the postal code. */
  state: string;
  /** `YYYY-MM-DD` */
  birth_date: string;
  previous_addresses: string[];
  phone: string;
  email: string;
}

// ============================================================================
// DTOs: what actually crosses the IPC boundary (secrets removed)
// ============================================================================

/** `AiSettings` without the Anthropic key, plus a flag. */
export interface AiSettingsDto {
  provider: AiProvider;
  ollama_url: string;
  ollama_model: string;
  anthropic_model: string;
  has_anthropic_key: boolean;
}

/** `Settings` without both API keys, plus flags. */
export interface SettingsDto {
  auto_lock_minutes: number;
  min_confidence: number;
  full_name: string;
  letter_language: "de" | "en";
  allow_catalog_updates: boolean;
  dark_mode: boolean;
  ai: AiSettingsDto;
  fetch_logos: boolean;
  identity: Identity;
  check_pwned_passwords: boolean;
  read_receipt_bodies: boolean;
  auto_rescan_days: number;
  notifications: boolean;
  has_hibp_key: boolean;
}

/** Writable part of the settings (`settings_update`); secrets are ignored if sent. */
export type SettingsPatch = {
  [K in keyof Omit<SettingsDto, "has_hibp_key" | "ai" | "identity">]?: SettingsDto[K];
} & {
  ai?: Partial<Omit<AiSettingsDto, "has_anthropic_key">>;
  identity?: Partial<Identity>;
};

export interface MailboxDto {
  id: Uuid;
  address: string;
  imap_host: string;
  imap_port: number;
  username: string;
  folders: string[];
  progress: FolderProgress[];
  last_scan: IsoDate | null;
  has_password: boolean;
}

/** `get_snapshot`: the whole vault minus secrets. */
export interface SnapshotDto {
  schema: number;
  accounts: Account[];
  runs: SourceRun[];
  mailboxes: MailboxDto[];
  settings: SettingsDto;
  subscriptions: Subscription[];
  requests: DataRequest[];
}

// ---- insights ---------------------------------------------------------------------

export type BillingInterval =
  | "Weekly" | "BiWeekly" | "Monthly" | "BiMonthly" | "Quarterly" | "FourMonthly" | "HalfYearly" | "Yearly";

export interface BillingSummary {
  account_id: Uuid;
  active_subscription: boolean;
  interval: BillingInterval | null;
  amount_minor: number | null;
  currency: string | null;
  /** Normalised to a month, minor units. */
  monthly_minor: number | null;
  plan: string | null;
  last_charge: IsoDate | null;
  next_charge_estimate: IsoDate | null;
  price_increased: boolean;
  cancelled_at: IsoDate | null;
  methods: PaymentMethod[];
  via: string | null;
  possibly_unused: boolean;
  /** `[currency, minor units]` */
  spent_last_12m: [string, number][];
}

export type RiskReason =
  | "PaymentDataStored"
  | "ActiveSubscription"
  | "ServiceBreached"
  | "IdentifierInBreach"
  | "PasswordPwned"
  | "PasswordReused"
  | "No2faButSupported"
  | "InactiveForYears"
  | "PhoneLogin"
  | "ForeignRecoveryAddress"
  | "SsoLinked";

export interface RiskAssessment {
  account_id: Uuid;
  /** 0 (harmless) .. 100 (act now) */
  score: number;
  reasons: RiskReason[];
  next_step_de: string;
}

export interface PrivacyScore {
  /** 0..100, higher is better */
  score: number;
  /** `[German label, points achieved, points possible]` */
  parts: [string, number, number][];
  open_accounts: number;
  deleted_accounts: number;
  high_risk_accounts: number;
  pending_requests: number;
  overdue_requests: number;
  /** false until anything was scanned or imported: show "noch keine Daten", not a score */
  has_data: boolean;
}

export interface AliasAdvice {
  account_id: Uuid;
  current_identifier: string;
  reason_de: string;
}

export interface InsightsDto {
  billing: BillingSummary[];
  /** `[currency, monthly minor, yearly minor, subscription count]` */
  totals: [string, number, number, number][];
  payment_index: [PaymentMethod, Uuid[]][];
  payment_accounts: Uuid[];
  /** Highest risk first. */
  risks: RiskAssessment[];
  score: PrivacyScore;
  aliases: AliasAdvice[];
  overdue_requests: Uuid[];
}

// ---- catalog / reference data -----------------------------------------------------

export type Difficulty = "Easy" | "Medium" | "Hard" | "Impossible" | "Unknown";

export interface DeletionInfo {
  name: string;
  domains: string[];
  url: string | null;
  difficulty: Difficulty;
  notes: string | null;
  email: string | null;
  legal_name: string | null;
  /** Multi-line postal address. */
  address: string | null;
  privacy_email: string | null;
  slug: string | null;
  relevant_countries: string[];
}

export interface TwoFactorInfo {
  methods: string[];
  documentation: string | null;
}

export interface CatalogInfo {
  deletion: DeletionInfo | null;
  two_factor: TwoFactorInfo | null;
  /** Entry with a postal address if one exists. */
  contact: DeletionInfo | null;
}

export type OrgKind = "CreditBureau" | "AddressBroker" | "OptOutList";

export interface Organization {
  id: string;
  name: string;
  legal_name: string;
  kind: OrgKind;
  address_lines: string[];
  email: string | null;
  web_form: string | null;
  description_de: string;
}

export interface Authority {
  /** `DE-BY`, ..., or `DE` for the federal BfDI. */
  state: string;
  name: string;
  address_lines: string[];
  email: string | null;
  complaint_form: string | null;
}

export interface HelpLink {
  title_de: string;
  url: string;
  description_de: string;
}

export interface AliasService extends HelpLink {
  /** Relay domains of the service. */
  domains: string[];
}

export interface ReferenceData {
  organizations: Organization[];
  authorities: Authority[];
  search_links: HelpLink[];
  payment_hubs: HelpLink[];
  alias_services: AliasService[];
}

// ---- mail -------------------------------------------------------------------------

export interface ProviderPreset {
  id: string;
  name: string;
  imap_host: string;
  imap_port: number;
  domains: string[];
  default_folders: string[];
  app_password_url: string | null;
  hint_de: string;
  smtp: { host: string; port: number; implicit_tls: boolean } | null;
}

/** Arguments of `mailbox_add` and the `draft` of `mailbox_test`. */
export interface MailboxDraft {
  address: string;
  imapHost: string;
  imapPort: number;
  username: string;
  password: string;
  folders: string[];
}

/** `mailbox_update`: every field optional; an empty / missing password keeps the stored one. */
export interface MailboxPatch {
  address?: string;
  imapHost?: string;
  imapPort?: number;
  username?: string;
  /** Write-only. */
  password?: string;
  folders?: string[];
}

// ---- imports ----------------------------------------------------------------------

export interface ImportDetection {
  kind: "passwords" | "google_takeout" | "facebook_export" | "generic_list";
  label_de: string;
  /** Only for `kind === "passwords"`. */
  format?: PasswordFormat;
}

export interface ScreenshotEntry {
  name: string;
  domain?: string | null;
  relay_address?: string | null;
}

/** `job://done` payload of `screenshot_read`. */
export interface ScreenshotReadPayload {
  provider: SsoProvider | null;
  entries: ScreenshotEntry[];
}

export interface ScreenshotCommitResult {
  new_accounts: number;
  updated: number;
}

// ---- letters ----------------------------------------------------------------------

export type LetterKind =
  | "Erasure"
  | "Access"
  | "ObjectionAndAccess"
  | "Cancellation"
  | "CeaseAndDesist"
  | "CreditBureauAccess"
  | "AddressBrokerObjection"
  | "Complaint"
  | "Reminder";

export interface Party {
  name: string;
  address_lines: string[];
  email?: string | null;
}

/** Arguments of `letter_preview` / `letter_save_pdf` (which also takes `path`). */
export interface LetterArgs {
  kind: LetterKind;
  /** Erasure, Access, ObjectionAndAccess, Cancellation */
  accountId?: Uuid;
  /** CeaseAndDesist */
  subscriptionId?: Uuid;
  /** Complaint, Reminder */
  requestId?: Uuid;
  /** CreditBureauAccess, AddressBrokerObjection (id from `reference_data().organizations`) */
  orgId?: string;
  /** Complaint: ISO 3166-2 state; default `settings.identity.state` / postal code */
  authorityState?: string;
  /** Overrides the recipient resolved from catalog / organisations / authorities. */
  recipient?: Party;
}

export interface LetterPreview {
  subject: string;
  /** Mail body (the subject travels separately). Placeholders like `[Name]` mark missing data. */
  text: string;
  recipient: Party;
  recipient_email: string | null;
  deadline: IsoDate | null;
  /** German labels of missing sender fields ("Name", "Straße", "PLZ", "Ort"). */
  missing_sender_fields: string[];
  /** No recipient could be resolved: ask the user (pass `recipient`). */
  missing_recipient: boolean;
}

export interface NewRequest {
  kind: RequestKind;
  target: string;
  domain?: string;
  accountId?: Uuid;
  subscriptionId?: Uuid;
  parentId?: Uuid;
  channel: RequestChannel;
  recipient: string;
  subject: string;
  body: string;
  /** true = already sent now (sets `sent_at`, status Sent and the deadline). */
  sent: boolean;
}

export interface RequestUpdate {
  id: Uuid;
  status?: RequestStatus;
  notes?: string;
  sentAt?: IsoDate;
}

// ---- jobs and events --------------------------------------------------------------

export type JobKind =
  | "scan"
  | "import"
  | "unsubscribe"
  | "ai_classify"
  | "screenshot_read"
  | "breach_check"
  | "catalog_refresh";

export interface JobItem {
  id: string;
  ok: boolean;
  message: string;
}

export interface JobProgress {
  jobId: JobId;
  kind: JobKind;
  done: number;
  total: number;
  message: string;
  item?: JobItem;
}

export interface JobDone<P = unknown> {
  jobId: JobId;
  kind: JobKind;
  ok: boolean;
  /** German one-liner; "Abgebrochen" if the user cancelled (then `error` is absent). */
  summary: string;
  error?: IpcErrorPayload;
  payload?: P;
}

export interface RunningJob {
  jobId: JobId;
  kind: JobKind;
  done: number;
  total: number;
}

/** Payloads of `job://done` per kind (all fields camelCase). */
export interface ScanDonePayload {
  mailboxes: number;
  failed: number;
  mails: number;
  findings: number;
  newAccounts: number;
  newSubscriptions: number;
  payments: number;
}
export interface ImportDonePayload {
  files: number;
  imported: number;
  findings: number;
  newAccounts: number;
  warnings: string[];
}
export interface UnsubscribeDonePayload {
  ok: number;
  failed: number;
  skipped: number;
  results: { id: Uuid; ok: boolean; skipped: boolean; message: string }[];
}
export interface ClassifyDonePayload {
  checked: number;
  changed: number;
}
export interface BreachDonePayload {
  catalogue: number;
  newBreaches: number;
  addressesChecked: number;
  confirmedNew: number;
  hibpKey: boolean;
}
export interface CatalogRefreshDonePayload {
  entries: number;
}

export const EVENTS = {
  jobProgress: "job://progress",
  jobDone: "job://done",
  vaultChanged: "vault://changed",
  vaultLocked: "vault://locked",
  /** Not in the original contract: the debounced background save failed. */
  saveFailed: "vault://save-failed",
  scheduleRescan: "schedule://rescan",
} as const;

export interface VaultStatus {
  exists: boolean;
  unlocked: boolean;
  path: string;
}

export interface AiModels {
  text: string[];
  vision: string[];
}

export type SecretKind = "anthropic" | "hibp";
export type PdfKitKind = "legacy" | "report";

// ============================================================================
// Calls
// ============================================================================

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw IpcError.from(e);
  }
}

// ---- vault ------------------------------------------------------------------------

export const vault = {
  status: () => call<VaultStatus>("vault_status"),
  /** Min. 10 characters (`Invalid` otherwise). `path` = custom vault location. */
  create: (password: string, path?: string) => call<void>("vault_create", { password, path }),
  /** `WrongPassword` on failure. */
  unlock: (password: string, path?: string) => call<void>("vault_unlock", { password, path }),
  lock: () => call<void>("vault_lock"),
  /** UI activity ping for the auto-lock timer. */
  touch: () => call<void>("vault_touch"),
  changePassword: (current: string, next: string) =>
    call<void>("vault_change_password", { current, next }),
  /** Plain JSON backup (contains secrets: the user chose to export). */
  exportJson: (path: string) => call<void>("vault_export_json", { path }),
};

// ---- data -------------------------------------------------------------------------

export const data = {
  snapshot: () => call<SnapshotDto>("get_snapshot"),
  insights: () => call<InsightsDto>("get_insights"),
  catalogInfo: (q: { domain?: string; name?: string }) => call<CatalogInfo>("catalog_info", q),
  referenceData: () => call<ReferenceData>("reference_data"),
  /** Favicon from the service's own domain as data URL, `null` if none / disabled. */
  logo: (domain: string) => call<string | null>("logo_get", { domain }),
};

// ---- accounts ---------------------------------------------------------------------

export const accounts = {
  setStatus: (ids: Uuid[], status: AccountStatus) => call<void>("account_set_status", { ids, status }),
  setNotes: (id: Uuid, notes: string) => call<void>("account_set_notes", { id, notes }),
  add: (domain: string, name?: string, identifier?: string) =>
    call<Uuid>("account_add", { domain, name, identifier }),
  /** Removes the records from the vault, not the real account. */
  remove: (ids: Uuid[]) => call<void>("account_remove", { ids }),
  /** Sets DeletionPending (`Confirmed` sets Deleted); a GDPR mail starts the 1 month deadline. */
  addDeletionStep: (id: Uuid, method: DeletionMethod, note: string) =>
    call<void>("account_add_deletion_step", { id, method, note }),
};

// ---- mailboxes and scanning -------------------------------------------------------

export const mail = {
  presets: () => call<ProviderPreset[]>("mail_presets"),
  add: (m: MailboxDraft) => call<Uuid>("mailbox_add", { ...m }),
  update: (id: Uuid, patch: MailboxPatch) => call<void>("mailbox_update", { id, patch }),
  remove: (id: Uuid) => call<void>("mailbox_remove", { id }),
  /** Returns the folder list. */
  testSaved: (id: Uuid) => call<string[]>("mailbox_test", { id }),
  testDraft: (draft: MailboxDraft) => call<string[]>("mailbox_test", { draft }),
  /** `full` ignores the incremental progress. */
  scanStart: (mailboxIds: Uuid[], full = false) => call<JobId>("scan_start", { mailboxIds, full }),
};

// ---- imports ----------------------------------------------------------------------

export const imports = {
  detect: (path: string) => call<ImportDetection>("import_detect", { path }),
  start: (paths: string[], checkPwned: boolean) => call<JobId>("import_start", { paths, checkPwned }),
  /** Done payload: `ScreenshotReadPayload`. */
  screenshotRead: (paths: string[]) => call<JobId>("screenshot_read", { paths }),
  screenshotCommit: (provider: SsoProvider | null, entries: ScreenshotEntry[]) =>
    call<ScreenshotCommitResult>("screenshot_commit", { provider, entries }),
};

// ---- newsletters ------------------------------------------------------------------

export const newsletters = {
  setStatus: (ids: Uuid[], status: SubscriptionStatus) =>
    call<void>("subscription_set_status", { ids, status }),
  /** Marks the category as decided by the user (never overwritten by rules or AI). */
  setCategory: (ids: Uuid[], category: MailCategory) =>
    call<void>("subscription_set_category", { ids, category }),
  /** One-click / mailto only; link-only and transactional lists are reported, not executed. */
  unsubscribeStart: (ids: Uuid[]) => call<JobId>("unsubscribe_start", { ids }),
  aiClassifyStart: () => call<JobId>("ai_classify_start"),
};

// ---- security ---------------------------------------------------------------------

export const security = {
  breachCheckStart: () => call<JobId>("breach_check_start"),
};

// ---- letters and requests ---------------------------------------------------------

export const letters = {
  preview: (a: LetterArgs) => call<LetterPreview>("letter_preview", { ...a }),
  /** `path` comes from the dialog plugin's save dialog. */
  savePdf: (a: LetterArgs, path: string) => call<void>("letter_save_pdf", { ...a, path }),
  mailtoUrl: (subject: string, body: string, to?: string) =>
    call<string>("mailto_url", { to, subject, body }),
};

export const requests = {
  create: (r: NewRequest) => call<Uuid>("request_create", { ...r }),
  update: (r: RequestUpdate) => call<void>("request_update", { ...r }),
  remove: (id: Uuid) => call<void>("request_remove", { id }),
};

// ---- settings and AI --------------------------------------------------------------

export const settings = {
  update: (patch: SettingsPatch) => call<void>("settings_update", { patch }),
  /** Write-only; an empty value clears the key. */
  setSecret: (kind: SecretKind, value: string) => call<void>("secret_set", { kind, value }),
  aiTest: () => call<string>("ai_test"),
  aiModels: () => call<AiModels>("ai_models"),
  catalogRefreshStart: () => call<JobId>("catalog_refresh_start"),
  generatePdfKit: (kind: PdfKitKind, path: string) => call<void>("generate_pdf_kit", { kind, path }),
};

// ---- jobs -------------------------------------------------------------------------

export const jobs = {
  cancel: (jobId: JobId) => call<void>("job_cancel", { jobId }),
  running: () => call<RunningJob[]>("jobs_running"),
};

// ============================================================================
// Events
// ============================================================================

export const onJobProgress = (cb: (p: JobProgress) => void): Promise<UnlistenFn> =>
  listen<JobProgress>(EVENTS.jobProgress, (e) => cb(e.payload));

export const onJobDone = (cb: (d: JobDone) => void): Promise<UnlistenFn> =>
  listen<JobDone>(EVENTS.jobDone, (e) => cb(e.payload));

/** After every vault mutation: refetch what the screen shows. */
export const onVaultChanged = (cb: (revision: number) => void): Promise<UnlistenFn> =>
  listen<{ revision: number }>(EVENTS.vaultChanged, (e) => cb(e.payload.revision));

/** Auto-lock or `vault_lock`: drop all state and show the unlock screen. */
export const onVaultLocked = (cb: () => void): Promise<UnlistenFn> =>
  listen(EVENTS.vaultLocked, () => cb());

export const onSaveFailed = (cb: (message: string) => void): Promise<UnlistenFn> =>
  listen<{ message: string }>(EVENTS.saveFailed, (e) => cb(e.payload.message));

/** The scheduler started an automatic rescan of these mailboxes. */
export const onScheduledRescan = (cb: (mailboxIds: Uuid[]) => void): Promise<UnlistenFn> =>
  listen<{ mailboxIds: Uuid[] }>(EVENTS.scheduleRescan, (e) => cb(e.payload.mailboxIds));

/**
 * Start a job and wait for it. Listeners are attached *before* the command is sent, so
 * neither early progress nor a very fast `job://done` is missed.
 *
 *   const done = await runJob(() => mail.scanStart(ids), { onProgress: setProgress });
 */
export async function runJob<P = unknown>(
  start: () => Promise<JobId>,
  opts: { onProgress?: (p: JobProgress) => void; onStarted?: (id: JobId) => void } = {},
): Promise<JobDone<P>> {
  let jobId: JobId | null = null;
  const early: JobProgress[] = [];
  const earlyDone: JobDone<P>[] = [];
  let resolve!: (d: JobDone<P>) => void;
  const finished = new Promise<JobDone<P>>((r) => (resolve = r));

  const unProgress = await onJobProgress((p) => {
    if (jobId === null) early.push(p);
    else if (p.jobId === jobId) opts.onProgress?.(p);
  });
  const unDone = await onJobDone((d) => {
    if (jobId === null) earlyDone.push(d as JobDone<P>);
    else if (d.jobId === jobId) resolve(d as JobDone<P>);
  });
  try {
    jobId = await start();
    opts.onStarted?.(jobId);
    for (const p of early) if (p.jobId === jobId) opts.onProgress?.(p);
    // The job may have finished before the command returned its id.
    const already = earlyDone.find((d) => d.jobId === jobId);
    if (already) resolve(already);
    return await finished;
  } finally {
    unProgress();
    unDone();
  }
}

// ============================================================================
// Sending and automation (backend phase 3; docs/IPC.md "Sending and automation")
// Declaration merging adds the new fields to the existing interfaces above.
// ============================================================================

export interface Account {
  /** The user wants an erasure request sent automatically. */
  queued_for_deletion?: boolean;
}

export interface DataRequest {
  /** Message-ID of the mail as sent (matches the copy in the Sent folder). */
  message_id?: string | null;
  mailbox_id?: Uuid | null;
  /** File name of the attached PDF. */
  attachment?: string | null;
}

export interface Subscription {
  last_message_id?: string | null;
}

export interface SettingsDto {
  /** Seconds between two mails from one mailbox (default 20). */
  send_gap_seconds?: number;
  /** Mails per mailbox and day; 0 = automatic (40, Gmail 100). */
  daily_send_cap?: number;
  start_minimized_to_tray?: boolean;
  autostart?: boolean;
  /** Skip auto-lock while an automation is enabled. Weakens protection, show a warning. */
  keep_unlocked_for_automation?: boolean;
}

export interface SnapshotDto {
  automations?: AutomationRule[];
}

export interface AutomationRule {
  id: Uuid;
  mailbox_id: Uuid;
  enabled: boolean;
  /** 1, 7, 14 or 30. */
  every_days: number;
  /** Local hour 0..23. */
  hour: number;
  rescan: boolean;
  send_erasure_for_queue: boolean;
  send_reminders: boolean;
  /** Only prepares drafts, complaints are never sent automatically. */
  prepare_complaints: boolean;
  last_run: string | null;
  last_result: string;
}

/** Input of `automation_upsert`: `id` optional (new rule), `last_*` are ignored. */
export type AutomationRuleInput = Omit<AutomationRule, "id" | "last_run" | "last_result"> & {
  id?: Uuid;
};

export interface BulkSendItem {
  kind: LetterKind;
  accountId: Uuid;
  recipient?: Party;
}

export interface SendCheck {
  ok: { accountId: Uuid; name: string; recipientEmail: string; subject: string }[];
  problems: { accountId: Uuid; reason: string }[];
  remainingToday: number;
  dailyCap: number;
  gapSeconds: number;
}

/** `job://done` payload of `letter_send`. */
export interface LetterSendResult {
  requestId: Uuid;
  messageId: string;
  sentAt: string;
  appendedTo: string | null;
  appendError: string | null;
}

/** `job://done` payload of `letters_send_bulk`. */
export interface BulkSendResult {
  sent: number;
  failed: number;
  skipped: number;
  capReached: boolean;
  results: {
    id: Uuid;
    ok: boolean;
    skipped: boolean;
    message: string;
    reason: "Skipped" | "Blocked" | "CapReached" | "SendFailed" | null;
  }[];
}

/** `job://done` payload of an automation run. */
export interface AutomationRunResult {
  runs: { ruleId: Uuid; summary: string; failed: number }[];
}

export type UnsubscribeFailureReason =
  | "Http"
  | "Network"
  | "UnsafeTarget"
  | "NoAutomaticMethod"
  | "Refused"
  | "Smtp";

/** Per item result in the `unsubscribe` job payload and its progress `item`. */
export interface UnsubscribeItemResult {
  id: Uuid;
  ok: boolean;
  skipped?: boolean;
  message: string;
  reason: UnsubscribeFailureReason | null;
}

export interface ManualLink {
  url: string;
  source: "header" | "mail";
  label: string;
}

export interface ManualLinks {
  links: ManualLink[];
  mailto?: string;
}

export const sending = {
  /** Builds the same letter as `letter_preview`, sends it and records the request. */
  letterSend: (a: {
    kind: LetterKind;
    accountId?: Uuid;
    subscriptionId?: Uuid;
    requestId?: Uuid;
    orgId?: string;
    authorityState?: string;
    recipient?: Party;
    mailboxId: Uuid;
    attachPdf: boolean;
  }) => call<JobId>("letter_send", a),
  lettersSendBulk: (items: BulkSendItem[], mailboxId: Uuid, attachPdf: boolean) =>
    call<JobId>("letters_send_bulk", { items, mailboxId, attachPdf }),
  /** Dry run of the bulk send, nothing is sent. */
  lettersSendCheck: (items: BulkSendItem[], mailboxId: Uuid, attachPdf = false) =>
    call<SendCheck>("letters_send_check", { items, mailboxId, attachPdf }),
  accountSetQueued: (ids: Uuid[], queued: boolean) =>
    call<void>("account_set_queued", { ids, queued }),
};

export const automations = {
  list: () => call<AutomationRule[]>("automation_list"),
  upsert: (rule: AutomationRuleInput) => call<Uuid>("automation_upsert", { rule }),
  remove: (id: Uuid) => call<void>("automation_remove", { id }),
  runNow: (id: Uuid) => call<JobId>("automation_run_now", { id }),
  autostartSet: (enabled: boolean) => call<void>("autostart_set", { enabled }),
};

export const unsubscribeFallback = {
  /** Header links always; `fromMail` additionally reads the newest mail of the list (explicit click). */
  manualLinks: (subscriptionId: Uuid, fromMail = false) =>
    call<ManualLinks>("unsubscribe_manual_links", { subscriptionId, fromMail }),
};
