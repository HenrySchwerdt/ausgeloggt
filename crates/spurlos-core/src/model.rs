//! Shared data model. This is the contract between all crates. Change with care:
//! only additive changes (new fields with `#[serde(default)]`, new enum variants).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const SCHEMA_VERSION: u32 = 1;

/// Everything stored inside the encrypted vault file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VaultData {
    /// Schema version of this struct, for future migrations.
    pub schema: u32,
    pub accounts: Vec<Account>,
    /// History of import / scan runs.
    #[serde(default)]
    pub runs: Vec<SourceRun>,
    /// Configured mailboxes (credentials live here, encrypted with the vault).
    #[serde(default)]
    pub mailboxes: Vec<MailboxConfig>,
    #[serde(default)]
    pub settings: Settings,
    /// Newsletters / mailing lists per mailbox (the "unsubscribe" feature).
    #[serde(default)]
    pub subscriptions: Vec<Subscription>,
}

/// A newsletter / mailing list sending to one of the user's mailboxes.
/// Key: (mailbox, list_key). `list_key` = List-Id if present, else sender address, lowercase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subscription {
    pub id: Uuid,
    /// Address of the user's mailbox that receives it.
    pub mailbox: String,
    pub list_key: String,
    /// Canonical registrable domain of the sender (links to an `Account` with this domain).
    pub domain: String,
    /// Display name, e.g. "Zalando Newsletter".
    pub name: String,
    pub sender: String,
    /// Unsubscribe options parsed from `List-Unsubscribe` (newest mail wins).
    #[serde(default)]
    pub unsubscribe: Vec<UnsubscribeMethod>,
    pub message_count: u64,
    /// Mails per 30 days over the observed period (for "nervt am meisten" sorting).
    pub frequency_per_month: f32,
    pub first_seen: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
    /// Up to 5 most recent subjects (for the UI and for AI classification).
    #[serde(default)]
    pub sample_subjects: Vec<String>,
    #[serde(default)]
    pub category: MailCategory,
    /// Who decided the category.
    #[serde(default)]
    pub category_source: CategorySource,
    #[serde(default)]
    pub status: SubscriptionStatus,
    #[serde(default)]
    pub history: Vec<UnsubscribeAttempt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnsubscribeMethod {
    /// RFC 8058 one-click: POST `List-Unsubscribe=One-Click` to `url`. Safe to automate.
    OneClick { url: String },
    /// `mailto:` unsubscribe; send a mail via SMTP. Safe to automate.
    Mailto {
        address: String,
        subject: Option<String>,
        body: Option<String>,
    },
    /// Plain https link. Only ever opened in the browser on explicit user click.
    Link { url: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum MailCategory {
    /// Advertising, promotions, deals.
    Marketing,
    /// Content newsletter the user may want (blogs, news digests).
    Newsletter,
    /// Social network notifications.
    Social,
    /// Receipts, shipping, account mails. Never suggest unsubscribing.
    Transactional,
    /// Likely spam / unsolicited.
    Spam,
    #[default]
    Unknown,
}

impl MailCategory {
    pub fn label_de(self) -> &'static str {
        match self {
            MailCategory::Marketing => "Werbung",
            MailCategory::Newsletter => "Newsletter",
            MailCategory::Social => "Social",
            MailCategory::Transactional => "Wichtig/Transaktional",
            MailCategory::Spam => "Spam",
            MailCategory::Unknown => "Unbekannt",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CategorySource {
    #[default]
    Rules,
    Ai,
    User,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum SubscriptionStatus {
    #[default]
    Active,
    /// User wants to keep receiving it.
    Keep,
    /// Unsubscribe sent; waiting to see if mails stop.
    Unsubscribed,
    /// Mails kept coming > 7 days after unsubscribing.
    StillReceiving,
    Failed,
}

impl SubscriptionStatus {
    pub fn label_de(&self) -> &'static str {
        match self {
            SubscriptionStatus::Active => "Aktiv",
            SubscriptionStatus::Keep => "Behalten",
            SubscriptionStatus::Unsubscribed => "Abgemeldet",
            SubscriptionStatus::StillReceiving => "Kommt trotzdem",
            SubscriptionStatus::Failed => "Fehlgeschlagen",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnsubscribeAttempt {
    pub at: DateTime<Utc>,
    pub method: UnsubscribeMethod,
    pub ok: bool,
    pub message: String,
}

/// What the mail scan observes about one list in one scan (merged into `Subscription`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubscriptionObservation {
    pub mailbox: String,
    pub list_key: String,
    pub sender: String,
    pub sender_name: Option<String>,
    /// Sender domain (any form, merge normalises).
    pub domain: String,
    pub unsubscribe: Vec<UnsubscribeMethod>,
    pub message_count: u64,
    pub first_seen: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
    /// Most recent subjects first, max 5.
    pub sample_subjects: Vec<String>,
    /// Rule-based guess.
    pub category: MailCategory,
}

/// One online account at one service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
    /// Canonical registrable domain, lowercase, e.g. `spotify.com`. Primary grouping key.
    pub domain: String,
    /// Human readable service name, e.g. `Spotify`.
    pub name: String,
    /// Logins / e-mail addresses / usernames seen for this account.
    #[serde(default)]
    pub identifiers: Vec<String>,
    /// SSO providers this account is linked to ("Sign in with ...").
    #[serde(default)]
    pub sso: Vec<SsoProvider>,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default)]
    pub status: AccountStatus,
    /// 0.0 ..= 1.0, how sure we are this is a real account. Recomputed by merge.
    pub confidence: f32,
    pub first_seen: Option<DateTime<Utc>>,
    pub last_seen: Option<DateTime<Utc>>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub notes: String,
    /// History of deletion attempts / requests for this account.
    #[serde(default)]
    pub deletion: Vec<DeletionStep>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One piece of evidence that an account exists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub kind: EvidenceKind,
    pub source: SourceKind,
    /// Short human readable description, e.g. the mail subject.
    pub summary: String,
    pub date: Option<DateTime<Utc>>,
    /// Stable id inside the source (mail Message-ID, CSV row hash ...) for dedup.
    pub source_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EvidenceKind {
    /// "Welcome to X", "Thanks for signing up".
    Welcome,
    /// "Confirm your e-mail address".
    Verification,
    /// "Reset your password".
    PasswordReset,
    /// "New sign-in to your account", 2FA codes, login alerts.
    LoginAlert,
    /// SSO provider says: "X was granted access to your account".
    SsoGrant,
    /// Credentials saved in a password manager / browser.
    SavedPassword,
    /// Listed in a data export (Google Takeout, Facebook, ...).
    DataExport,
    /// Account / domain appears in a known data breach.
    Breach,
    /// Order / invoice / receipt. Weak evidence (could be a guest checkout).
    Receipt,
    /// Newsletter / marketing. Weak evidence.
    Newsletter,
    /// Other transactional mail from the service.
    OtherMail,
    /// Added by hand.
    Manual,
}

impl EvidenceKind {
    pub const ALL: [EvidenceKind; 12] = [
        EvidenceKind::Welcome,
        EvidenceKind::Verification,
        EvidenceKind::PasswordReset,
        EvidenceKind::LoginAlert,
        EvidenceKind::SsoGrant,
        EvidenceKind::SavedPassword,
        EvidenceKind::DataExport,
        EvidenceKind::Breach,
        EvidenceKind::Receipt,
        EvidenceKind::Newsletter,
        EvidenceKind::OtherMail,
        EvidenceKind::Manual,
    ];

    /// How strongly this kind of evidence indicates a real account (0..=1).
    pub fn weight(self) -> f32 {
        match self {
            EvidenceKind::Manual => 1.0,
            EvidenceKind::SavedPassword => 0.95,
            EvidenceKind::SsoGrant => 0.95,
            EvidenceKind::Welcome => 0.9,
            EvidenceKind::LoginAlert => 0.9,
            EvidenceKind::Verification => 0.85,
            EvidenceKind::PasswordReset => 0.85,
            EvidenceKind::DataExport => 0.8,
            EvidenceKind::Breach => 0.7,
            EvidenceKind::Receipt => 0.35,
            EvidenceKind::OtherMail => 0.3,
            EvidenceKind::Newsletter => 0.15,
        }
    }

    pub fn label_de(self) -> &'static str {
        match self {
            EvidenceKind::Welcome => "Willkommensmail",
            EvidenceKind::Verification => "E-Mail-Bestätigung",
            EvidenceKind::PasswordReset => "Passwort-Reset",
            EvidenceKind::LoginAlert => "Login-Hinweis",
            EvidenceKind::SsoGrant => "SSO-Verknüpfung",
            EvidenceKind::SavedPassword => "Gespeichertes Passwort",
            EvidenceKind::DataExport => "Datenexport",
            EvidenceKind::Breach => "Datenleck",
            EvidenceKind::Receipt => "Bestellung/Rechnung",
            EvidenceKind::Newsletter => "Newsletter",
            EvidenceKind::OtherMail => "Sonstige Mail",
            EvidenceKind::Manual => "Manuell",
        }
    }
}

/// Where a finding came from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceKind {
    /// Mail scan; `mailbox` is the address that was scanned.
    Mail {
        mailbox: String,
    },
    PasswordExport {
        format: PasswordFormat,
    },
    GoogleTakeout,
    FacebookExport,
    /// Generic CSV/JSON/text list of domains or URLs.
    GenericList,
    GitHub,
    HaveIBeenPwned,
    Manual,
}

impl SourceKind {
    pub fn label_de(&self) -> String {
        match self {
            SourceKind::Mail { mailbox } => format!("Postfach {mailbox}"),
            SourceKind::PasswordExport { format } => format!("Passwörter ({})", format.label()),
            SourceKind::GoogleTakeout => "Google Takeout".into(),
            SourceKind::FacebookExport => "Facebook-Export".into(),
            SourceKind::GenericList => "Liste".into(),
            SourceKind::GitHub => "GitHub".into(),
            SourceKind::HaveIBeenPwned => "Have I Been Pwned".into(),
            SourceKind::Manual => "Manuell".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PasswordFormat {
    Chrome,
    Firefox,
    Safari,
    Edge,
    Bitwarden,
    OnePassword,
    KeePass,
    LastPass,
    Proton,
    Dashlane,
    Generic,
}

impl PasswordFormat {
    pub fn label(self) -> &'static str {
        match self {
            PasswordFormat::Chrome => "Chrome",
            PasswordFormat::Firefox => "Firefox",
            PasswordFormat::Safari => "Safari / Apple Passwörter",
            PasswordFormat::Edge => "Edge",
            PasswordFormat::Bitwarden => "Bitwarden",
            PasswordFormat::OnePassword => "1Password",
            PasswordFormat::KeePass => "KeePass",
            PasswordFormat::LastPass => "LastPass",
            PasswordFormat::Proton => "Proton Pass",
            PasswordFormat::Dashlane => "Dashlane",
            PasswordFormat::Generic => "CSV",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SsoProvider {
    Google,
    Apple,
    Microsoft,
    GitHub,
    Facebook,
    Discord,
    X,
    LinkedIn,
    Amazon,
    Other(String),
}

impl SsoProvider {
    pub fn label(&self) -> &str {
        match self {
            SsoProvider::Google => "Google",
            SsoProvider::Apple => "Apple",
            SsoProvider::Microsoft => "Microsoft",
            SsoProvider::GitHub => "GitHub",
            SsoProvider::Facebook => "Facebook",
            SsoProvider::Discord => "Discord",
            SsoProvider::X => "X",
            SsoProvider::LinkedIn => "LinkedIn",
            SsoProvider::Amazon => "Amazon",
            SsoProvider::Other(s) => s,
        }
    }

    /// Page where the user can see / revoke apps connected via this provider.
    pub fn connections_url(&self) -> Option<&'static str> {
        match self {
            SsoProvider::Google => Some("https://myaccount.google.com/connections"),
            SsoProvider::Apple => Some("https://account.apple.com/account/manage"),
            SsoProvider::Microsoft => Some("https://account.live.com/consent/Manage"),
            SsoProvider::GitHub => Some("https://github.com/settings/applications"),
            SsoProvider::Facebook => Some("https://www.facebook.com/settings?tab=applications"),
            SsoProvider::Discord => Some("https://discord.com/settings/authorized-apps"),
            SsoProvider::X => Some("https://x.com/settings/connected_apps"),
            SsoProvider::LinkedIn => {
                Some("https://www.linkedin.com/mypreferences/d/permitted-services")
            }
            SsoProvider::Amazon => Some("https://www.amazon.com/ap/adam"),
            SsoProvider::Other(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum AccountStatus {
    /// Found automatically, not yet reviewed by the user.
    #[default]
    Discovered,
    /// User confirmed: yes, I have an account here.
    Confirmed,
    /// User wants to keep this account.
    Keep,
    /// Deletion started (link opened / GDPR request sent); waiting.
    DeletionPending,
    /// Account is gone.
    Deleted,
    /// Not an account (newsletter, guest order ...). Hidden by default.
    NotAnAccount,
    /// User doesn't care. Hidden by default.
    Ignored,
}

impl AccountStatus {
    pub fn label_de(&self) -> &'static str {
        match self {
            AccountStatus::Discovered => "Neu entdeckt",
            AccountStatus::Confirmed => "Bestätigt",
            AccountStatus::Keep => "Behalten",
            AccountStatus::DeletionPending => "Löschung läuft",
            AccountStatus::Deleted => "Gelöscht",
            AccountStatus::NotAnAccount => "Kein Account",
            AccountStatus::Ignored => "Ignoriert",
        }
    }
}

/// One step in the deletion process of an account.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeletionStep {
    pub at: DateTime<Utc>,
    pub method: DeletionMethod,
    pub note: String,
    /// When the service must have answered (GDPR: 1 month after request).
    pub deadline: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DeletionMethod {
    /// Opened the service's own deletion page.
    DirectLink {
        url: String,
    },
    /// Sent a GDPR Art. 17 erasure request by e-mail.
    GdprEmail {
        to: String,
    },
    /// Revoked SSO access at the provider (does NOT delete the account!).
    SsoRevoked {
        provider: SsoProvider,
    },
    /// Service confirmed deletion.
    Confirmed,
    Other,
}

/// What a discovery source emits. Merged into [`Account`]s by [`crate::merge`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    /// Any domain, host, URL or e-mail address seen, e.g. `mail.spotify.com`.
    /// Merge normalises this to the canonical registrable domain.
    pub domain: String,
    /// Service name if the source knows it (e.g. password-manager entry title).
    pub name: Option<String>,
    /// Login / e-mail used at the service, if known.
    pub identifier: Option<String>,
    pub sso: Option<SsoProvider>,
    pub evidence: Evidence,
}

/// Record of one import / scan run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRun {
    pub id: Uuid,
    pub source: SourceKind,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    /// Items looked at (mails, CSV rows ...).
    pub items_scanned: u64,
    pub findings: u64,
    pub new_accounts: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MailboxConfig {
    pub id: Uuid,
    /// The e-mail address.
    pub address: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub username: String,
    /// App password. Stored only inside the encrypted vault.
    pub password: String,
    /// Folders to scan; empty = provider default (e.g. Gmail "All Mail").
    #[serde(default)]
    pub folders: Vec<String>,
    /// Incremental-scan progress per folder.
    #[serde(default)]
    pub progress: Vec<FolderProgress>,
    pub last_scan: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderProgress {
    pub folder: String,
    pub uid_validity: u32,
    /// Highest UID already scanned.
    pub last_uid: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Lock the vault after this many minutes without interaction (0 = never).
    pub auto_lock_minutes: u32,
    /// Accounts below this confidence are shown as "unsicher".
    pub min_confidence: f32,
    /// Full name used in GDPR letters.
    pub full_name: String,
    /// Language for generated GDPR letters: "de" or "en".
    pub letter_language: String,
    /// Allow network requests to refresh the deletion catalog.
    pub allow_catalog_updates: bool,
    pub dark_mode: bool,
    pub ai: AiSettings,
    /// Fetch missing logos directly from the service's own domain (never via third-party
    /// favicon services). Bundled offline logos are always used.
    pub fetch_logos: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_lock_minutes: 10,
            min_confidence: 0.5,
            full_name: String::new(),
            letter_language: "de".into(),
            allow_catalog_updates: true,
            dark_mode: true,
            ai: AiSettings::default(),
            fetch_logos: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiSettings {
    pub provider: AiProvider,
    /// Ollama base URL.
    pub ollama_url: String,
    /// Ollama model name, e.g. `llama3.2:3b`, `qwen2.5:7b`.
    pub ollama_model: String,
    /// Anthropic API key (stored only in the encrypted vault).
    pub anthropic_api_key: String,
    pub anthropic_model: String,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            provider: AiProvider::Off,
            ollama_url: "http://127.0.0.1:11434".into(),
            ollama_model: String::new(),
            anthropic_api_key: String::new(),
            anthropic_model: "claude-haiku-5-5".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AiProvider {
    /// Rules only.
    #[default]
    Off,
    /// Local model via Ollama: nothing leaves the machine. Recommended.
    Ollama,
    /// Anthropic API: sends sender + subjects (never bodies) to Anthropic. Opt-in.
    Anthropic,
}
