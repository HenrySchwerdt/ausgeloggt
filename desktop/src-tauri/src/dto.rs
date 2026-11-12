//! DTOs and read-only views for the UI. Secrets are removed here, centrally:
//! mailbox passwords, the Anthropic key and the HIBP key never leave the backend.

use std::sync::OnceLock;

use ausgeloggt_catalog::{orgs, Catalog};
use ausgeloggt_core::insights::{self, RiskContext};
use ausgeloggt_core::model::{MailboxConfig, Settings, VaultData};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::error::{IpcError, IpcResult};

/// Keys that must never be accepted from or sent to the UI.
const SECRET_SETTING_KEYS: [&str; 2] = ["hibp_api_key", "has_hibp_key"];
const SECRET_AI_KEYS: [&str; 2] = ["anthropic_api_key", "has_anthropic_key"];

fn to_value<T: Serialize>(v: &T) -> IpcResult<Value> {
    serde_json::to_value(v).map_err(|e| IpcError::internal(format!("Serialisierung: {e}")))
}

pub fn mailbox_dto(mb: &MailboxConfig) -> IpcResult<Value> {
    let mut v = to_value(mb)?;
    if let Some(o) = v.as_object_mut() {
        o.remove("password");
        o.insert("has_password".into(), json!(!mb.password.is_empty()));
    }
    Ok(v)
}

pub fn settings_dto(s: &Settings) -> IpcResult<Value> {
    let mut v = to_value(s)?;
    if let Some(o) = v.as_object_mut() {
        for k in SECRET_SETTING_KEYS {
            o.remove(k);
        }
        o.insert("has_hibp_key".into(), json!(!s.hibp_api_key.is_empty()));
        if let Some(ai) = o.get_mut("ai").and_then(Value::as_object_mut) {
            for k in SECRET_AI_KEYS {
                ai.remove(k);
            }
            ai.insert(
                "has_anthropic_key".into(),
                json!(!s.ai.anthropic_api_key.is_empty()),
            );
        }
    }
    Ok(v)
}

/// `SnapshotDto`: the whole vault minus secrets, plus the `has_*` flags.
pub fn snapshot(data: &VaultData) -> IpcResult<Value> {
    let mailboxes = data
        .mailboxes
        .iter()
        .map(mailbox_dto)
        .collect::<IpcResult<Vec<_>>>()?;
    Ok(json!({
        "schema": data.schema,
        "accounts": to_value(&data.accounts)?,
        "runs": to_value(&data.runs)?,
        "mailboxes": mailboxes,
        "settings": settings_dto(&data.settings)?,
        "subscriptions": to_value(&data.subscriptions)?,
        "requests": to_value(&data.requests)?,
        "automations": to_value(&data.automations)?,
    }))
}

/// Deep merge (JSON merge patch without `null` deletion): objects merge recursively,
/// everything else (including arrays) is replaced.
fn merge_json(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, pv) in p {
                match b.get_mut(k) {
                    Some(bv) if bv.is_object() && pv.is_object() => merge_json(bv, pv),
                    _ => {
                        b.insert(k.clone(), pv.clone());
                    }
                }
            }
        }
        (b, p) => *b = p.clone(),
    }
}

/// Apply `settings_update`: a partial `Settings`; secrets in the patch are ignored.
pub fn apply_settings_patch(current: &Settings, patch: &Value) -> IpcResult<Settings> {
    let Some(patch_obj) = patch.as_object() else {
        return Err(IpcError::invalid("Einstellungen müssen ein Objekt sein"));
    };
    let mut clean: Map<String, Value> = patch_obj.clone();
    for k in SECRET_SETTING_KEYS {
        clean.remove(k);
    }
    if let Some(ai) = clean.get_mut("ai").and_then(Value::as_object_mut) {
        for k in SECRET_AI_KEYS {
            ai.remove(k);
        }
    }
    let mut base = to_value(current)?;
    merge_json(&mut base, &Value::Object(clean));
    let s: Settings = serde_json::from_value(base)
        .map_err(|e| IpcError::invalid(format!("Ungültige Einstellungen: {e}")))?;
    if !(0.0..=1.0).contains(&s.min_confidence) {
        return Err(IpcError::invalid(
            "Mindestsicherheit muss zwischen 0 und 1 liegen",
        ));
    }
    if !matches!(s.letter_language.as_str(), "de" | "en") {
        return Err(IpcError::invalid("Briefsprache muss de oder en sein"));
    }
    Ok(s)
}

// ---- insights ---------------------------------------------------------------------

fn two_factor_domains() -> &'static [String] {
    static V: OnceLock<Vec<String>> = OnceLock::new();
    V.get_or_init(orgs::two_factor_domains)
}

fn domain_of_address(addr: &str) -> Option<String> {
    let d = addr.rsplit_once('@')?.1.trim().to_lowercase();
    (!d.is_empty()).then_some(d)
}

pub fn risk_context(data: &VaultData) -> RiskContext {
    let mut own: Vec<String> = data
        .mailboxes
        .iter()
        .filter_map(|m| domain_of_address(&m.address))
        .collect();
    if let Some(d) = domain_of_address(&data.settings.identity.email) {
        own.push(d);
    }
    own.sort();
    own.dedup();
    RiskContext {
        own_mail_domains: own,
        supports_2fa: two_factor_domains().to_vec(),
    }
}

pub fn insights_dto(data: &VaultData, now: DateTime<Utc>) -> IpcResult<Value> {
    let ctx = risk_context(data);
    let billing: Vec<_> = data
        .accounts
        .iter()
        .filter_map(|a| insights::billing_summary(a, now))
        .collect();
    Ok(json!({
        "billing": to_value(&billing)?,
        "totals": to_value(&insights::subscription_totals(data, now))?,
        "payment_index": to_value(&insights::payment_method_index(data))?,
        "payment_accounts": to_value(&insights::accounts_with_payment_data(data))?,
        "risks": to_value(&insights::risk_ranking(data, &ctx, now))?,
        "score": to_value(&insights::privacy_score(data, &ctx, now))?,
        "aliases": to_value(&insights::alias_recommendations(data))?,
        "overdue_requests": to_value(&insights::overdue_requests(data, now))?,
    }))
}

// ---- catalog ----------------------------------------------------------------------

pub fn catalog_info(
    catalog: &Catalog,
    domain: Option<&str>,
    name: Option<&str>,
) -> IpcResult<Value> {
    let domain = domain.map(str::trim).filter(|d| !d.is_empty());
    let name = name.map(str::trim).filter(|n| !n.is_empty());
    let resolvable = domain.filter(|d| !d.starts_with("unresolved:"));
    let deletion = resolvable
        .and_then(|d| catalog.lookup(d))
        .or_else(|| name.and_then(|n| catalog.lookup_name(n)));
    let contact = resolvable
        .and_then(|d| catalog.lookup_contact(d))
        .or_else(|| name.and_then(|n| catalog.lookup_name(n)));
    let two_factor = resolvable.and_then(orgs::two_factor);
    Ok(json!({
        "deletion": to_value(&deletion)?,
        "two_factor": to_value(&two_factor)?,
        "contact": to_value(&contact)?,
    }))
}

pub fn reference_data() -> IpcResult<Value> {
    let alias_services: Vec<Value> = orgs::alias_services()
        .iter()
        .map(|(link, domains)| {
            json!({
                "title_de": link.title_de,
                "url": link.url,
                "description_de": link.description_de,
                "domains": domains,
            })
        })
        .collect();
    Ok(json!({
        "organizations": to_value(&orgs::organizations())?,
        "authorities": to_value(&orgs::authorities())?,
        "search_links": to_value(&orgs::search_removal_links())?,
        "payment_hubs": to_value(&orgs::payment_hubs())?,
        "alias_services": alias_services,
    }))
}

// ---- mail presets -----------------------------------------------------------------

pub fn mail_presets() -> Vec<Value> {
    ausgeloggt_mail::presets()
        .iter()
        .map(|p| {
            // SMTP settings come from a fake mailbox on the preset's first domain.
            let probe = MailboxConfig {
                id: uuid::Uuid::nil(),
                address: format!(
                    "probe@{}",
                    p.domains.first().copied().unwrap_or("example.com")
                ),
                imap_host: p.imap_host.to_string(),
                imap_port: p.imap_port,
                username: String::new(),
                password: String::new(),
                folders: Vec::new(),
                progress: Vec::new(),
                last_scan: None,
            };
            let smtp = ausgeloggt_mail::unsubscribe::smtp_for(&probe)
                .map(|s| json!({ "host": s.host, "port": s.port, "implicit_tls": s.implicit_tls }));
            json!({
                "id": p.id,
                "name": p.name,
                "imap_host": p.imap_host,
                "imap_port": p.imap_port,
                "domains": p.domains,
                "default_folders": p.default_folders,
                "app_password_url": p.app_password_url,
                "hint_de": p.hint_de,
                "smtp": smtp,
            })
        })
        .collect()
}

// ---- mailbox arguments ------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailboxDraft {
    pub address: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub folders: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailboxPatch {
    pub address: Option<String>,
    pub imap_host: Option<String>,
    pub imap_port: Option<u16>,
    pub username: Option<String>,
    /// Write-only. `None` or empty keeps the stored password.
    pub password: Option<String>,
    pub folders: Option<Vec<String>>,
}

impl MailboxDraft {
    pub fn validate(&self) -> IpcResult<()> {
        let addr = self.address.trim();
        if !addr.contains('@') || addr.len() < 3 {
            return Err(IpcError::invalid(
                "Bitte eine gültige E-Mail-Adresse angeben",
            ));
        }
        if self.imap_host.trim().is_empty() || self.imap_port == 0 {
            return Err(IpcError::invalid("IMAP-Server und Port fehlen"));
        }
        if self.username.trim().is_empty() {
            return Err(IpcError::invalid("Benutzername fehlt"));
        }
        if self.password.is_empty() {
            return Err(IpcError::invalid("App-Passwort fehlt"));
        }
        Ok(())
    }

    pub fn into_config(self) -> MailboxConfig {
        MailboxConfig {
            id: uuid::Uuid::new_v4(),
            address: self.address.trim().to_string(),
            imap_host: self.imap_host.trim().to_string(),
            imap_port: self.imap_port,
            username: self.username.trim().to_string(),
            password: self.password,
            folders: clean_folders(self.folders),
            progress: Vec::new(),
            last_scan: None,
        }
    }
}

fn clean_folders(folders: Vec<String>) -> Vec<String> {
    folders
        .into_iter()
        .map(|f| f.trim().to_string())
        .filter(|f| !f.is_empty())
        .collect()
}

impl MailboxPatch {
    /// Apply to a stored mailbox. Changing server, user, password or folders resets the
    /// incremental scan progress (UIDs belong to the old account / folder set).
    pub fn apply(self, mb: &mut MailboxConfig) {
        let mut reset = false;
        if let Some(v) = self
            .address
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            mb.address = v;
        }
        if let Some(v) = self
            .imap_host
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            reset |= v != mb.imap_host;
            mb.imap_host = v;
        }
        if let Some(v) = self.imap_port.filter(|p| *p != 0) {
            reset |= v != mb.imap_port;
            mb.imap_port = v;
        }
        if let Some(v) = self
            .username
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            reset |= v != mb.username;
            mb.username = v;
        }
        if let Some(v) = self.password.filter(|s| !s.is_empty()) {
            mb.password = v;
        }
        if let Some(v) = self.folders {
            let v = clean_folders(v);
            reset |= v != mb.folders;
            mb.folders = v;
        }
        if reset {
            mb.progress.clear();
            mb.last_scan = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ausgeloggt_core::model::Account;

    fn data_with_secrets() -> VaultData {
        let mut d = VaultData::default();
        d.settings.hibp_api_key = "HIBP-SECRET-123".into();
        d.settings.ai.anthropic_api_key = "sk-ant-SECRET-456".into();
        d.mailboxes.push(MailboxConfig {
            id: uuid::Uuid::new_v4(),
            address: "me@example.org".into(),
            imap_host: "imap.example.org".into(),
            imap_port: 993,
            username: "me".into(),
            password: "MAILBOX-SECRET-789".into(),
            folders: vec![],
            progress: vec![],
            last_scan: None,
        });
        d.accounts.push(Account::new("spotify.com", "Spotify"));
        d
    }

    #[test]
    fn snapshot_strips_all_secrets_and_adds_flags() {
        let d = data_with_secrets();
        let v = snapshot(&d).unwrap();
        let text = v.to_string();
        for secret in ["HIBP-SECRET-123", "sk-ant-SECRET-456", "MAILBOX-SECRET-789"] {
            assert!(!text.contains(secret), "{secret} leaked");
        }
        assert!(!text.contains("hibp_api_key"));
        assert!(!text.contains("anthropic_api_key"));
        assert!(v["mailboxes"][0].get("password").is_none());
        assert_eq!(v["mailboxes"][0]["has_password"], true);
        assert_eq!(v["settings"]["has_hibp_key"], true);
        assert_eq!(v["settings"]["ai"]["has_anthropic_key"], true);
        assert_eq!(v["accounts"][0]["domain"], "spotify.com");
        // other settings survive
        assert_eq!(v["settings"]["auto_lock_minutes"], 10);
    }

    #[test]
    fn flags_are_false_when_nothing_is_set() {
        let v = snapshot(&VaultData::default()).unwrap();
        assert_eq!(v["settings"]["has_hibp_key"], false);
        assert_eq!(v["settings"]["ai"]["has_anthropic_key"], false);
    }

    #[test]
    fn settings_patch_merges_and_ignores_secrets() {
        let d = data_with_secrets();
        let patch = json!({
            "full_name": "Erika Muster",
            "ai": { "provider": "Ollama", "anthropic_api_key": "evil", "has_anthropic_key": false },
            "hibp_api_key": "evil2",
            "identity": { "city": "Berlin" },
        });
        let s = apply_settings_patch(&d.settings, &patch).unwrap();
        assert_eq!(s.full_name, "Erika Muster");
        assert_eq!(s.ai.provider, ausgeloggt_core::AiProvider::Ollama);
        assert_eq!(s.ai.anthropic_api_key, "sk-ant-SECRET-456");
        assert_eq!(s.hibp_api_key, "HIBP-SECRET-123");
        assert_eq!(s.identity.city, "Berlin");
        assert_eq!(s.identity.country, "Deutschland");
        assert_eq!(s.ai.ollama_url, d.settings.ai.ollama_url);
    }

    #[test]
    fn settings_patch_validates() {
        let s = Settings::default();
        assert!(apply_settings_patch(&s, &json!({ "min_confidence": 7 })).is_err());
        assert!(apply_settings_patch(&s, &json!({ "letter_language": "fr" })).is_err());
        assert!(apply_settings_patch(&s, &json!({ "auto_lock_minutes": "x" })).is_err());
        assert!(apply_settings_patch(&s, &json!(5)).is_err());
    }

    #[test]
    fn mailbox_patch_keeps_password_and_resets_progress_on_server_change() {
        let mut mb = data_with_secrets().mailboxes.remove(0);
        mb.progress.push(ausgeloggt_core::FolderProgress {
            folder: "INBOX".into(),
            uid_validity: 1,
            last_uid: 5,
        });
        mb.last_scan = Some(Utc::now());
        MailboxPatch {
            password: Some(String::new()),
            ..Default::default()
        }
        .apply(&mut mb);
        assert_eq!(mb.password, "MAILBOX-SECRET-789");
        assert_eq!(mb.progress.len(), 1);
        MailboxPatch {
            imap_host: Some("imap.other.org".into()),
            password: Some("neu".into()),
            ..Default::default()
        }
        .apply(&mut mb);
        assert_eq!(mb.password, "neu");
        assert!(mb.progress.is_empty() && mb.last_scan.is_none());
    }

    #[test]
    fn draft_validation() {
        let mut d = MailboxDraft {
            address: "a@b.de".into(),
            imap_host: "imap.b.de".into(),
            imap_port: 993,
            username: "a".into(),
            password: "x".into(),
            folders: vec![" INBOX ".into(), "".into()],
        };
        assert!(d.validate().is_ok());
        assert_eq!(d.clone().into_config().folders, vec!["INBOX"]);
        d.password.clear();
        assert!(d.validate().is_err());
    }

    #[test]
    fn insights_and_reference_data_serialize() {
        let d = data_with_secrets();
        let v = insights_dto(&d, Utc::now()).unwrap();
        for k in [
            "billing",
            "totals",
            "payment_index",
            "payment_accounts",
            "risks",
            "score",
            "aliases",
            "overdue_requests",
        ] {
            assert!(v.get(k).is_some(), "{k} missing");
        }
        assert!(v["score"]["score"].is_number());
        let r = reference_data().unwrap();
        assert!(r["organizations"].as_array().unwrap().len() > 3);
        assert!(r["alias_services"][0]["domains"].is_array());
        assert!(!mail_presets().is_empty());
    }

    #[test]
    fn catalog_info_finds_known_service() {
        let c = Catalog::bundled();
        let v = catalog_info(&c, Some("spotify.com"), None).unwrap();
        assert!(v["deletion"].is_object());
        let none = catalog_info(&c, Some("gibt-es-nicht-12345.example"), None).unwrap();
        assert!(none["deletion"].is_null());
    }
}
