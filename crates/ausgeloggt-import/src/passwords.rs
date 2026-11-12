//! Password-manager / browser CSV and Bitwarden JSON importers.
//!
//! Only URL, title, username and (where available) a date are read from a row. Password,
//! notes, TOTP and custom fields are never copied out of the `csv` record.

use std::collections::HashSet;

use ausgeloggt_core::{Evidence, EvidenceKind, Finding, PasswordFormat, SourceKind};
use serde_json::Value;
use zeroize::Zeroizing;

use crate::util::{host_of, parse_ts, source_ref};
use crate::{ImportError, ImportResult};

/// Which kind of CSV a header row describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CsvKind {
    Passwords(PasswordFormat),
    /// CSV with a URL-ish column but no password column.
    List,
}

/// One login row handed to the audit. The password lives only in a zeroizing buffer;
/// the type deliberately has no `Debug` impl.
pub(crate) struct PwRow {
    pub domain: String,
    pub identifier: Option<String>,
    pub password: Zeroizing<String>,
    pub has_totp: bool,
}

/// How findings from a table are labelled.
pub(crate) enum Ctx {
    Passwords(PasswordFormat),
    List,
}

impl Ctx {
    fn source(&self) -> SourceKind {
        match self {
            Ctx::Passwords(f) => SourceKind::PasswordExport { format: *f },
            Ctx::List => SourceKind::GenericList,
        }
    }
    fn kind(&self) -> EvidenceKind {
        match self {
            Ctx::Passwords(_) => EvidenceKind::SavedPassword,
            Ctx::List => EvidenceKind::DataExport,
        }
    }
    fn summary(&self) -> String {
        match self {
            Ctx::Passwords(PasswordFormat::Generic) => "Gespeichert in einer CSV-Datei".into(),
            Ctx::Passwords(f) => format!("Gespeichert in {}", short_label(*f)),
            Ctx::List => "In einer Liste aufgeführt".into(),
        }
    }
    fn tag(&self) -> &'static str {
        match self {
            Ctx::Passwords(_) => "pw",
            Ctx::List => "list",
        }
    }
    fn label(&self) -> &'static str {
        match self {
            Ctx::Passwords(f) => short_label(*f),
            Ctx::List => "list",
        }
    }
}

fn short_label(f: PasswordFormat) -> &'static str {
    match f {
        PasswordFormat::Safari => "Safari / Apple Passwörter",
        other => other.label(),
    }
}

#[derive(Debug, Clone)]
struct Layout {
    url: Vec<usize>,
    name: Option<usize>,
    user: Vec<usize>,
    date: Vec<usize>,
    /// Row type column (Bitwarden / Proton): only `login` rows are kept.
    kind: Option<usize>,
    /// KeePass group column (recycle bin rows are skipped).
    group: Option<usize>,
    split_commas: bool,
    /// Password column (read only transiently for the audit).
    pass: Option<usize>,
    /// TOTP / OTP columns (only checked for non-emptiness).
    totp: Vec<usize>,
}

fn all_cols(h: &[String], names: &[&str]) -> Vec<usize> {
    names
        .iter()
        .filter_map(|n| h.iter().position(|x| x == n))
        .collect()
}

fn first_col(h: &[String], names: &[&str]) -> Option<usize> {
    all_cols(h, names).first().copied()
}

fn has(h: &[String], n: &str) -> bool {
    h.iter().any(|x| x == n)
}

const GENERIC_URL: &[&str] = &[
    "url",
    "website",
    "web site",
    "domain",
    "login_uri",
    "hostname",
    "host",
    "site",
    "link",
    "address",
    "uri",
];
const GENERIC_USER: &[&str] = &[
    "username",
    "user",
    "login",
    "login_username",
    "email",
    "e-mail",
    "user name",
];
const GENERIC_NAME: &[&str] = &["name", "title", "site name"];

/// Decide the CSV flavour from the (lowercased, trimmed) header cells.
pub(crate) fn sniff_headers(h: &[String]) -> Option<CsvKind> {
    use PasswordFormat as F;
    let pw = |f| Some(CsvKind::Passwords(f));
    if has(h, "httprealm") && has(h, "formactionorigin") {
        return pw(F::Firefox);
    }
    if has(h, "login_uri") && has(h, "login_username") {
        return pw(F::Bitwarden);
    }
    if has(h, "grouping") && has(h, "extra") && has(h, "url") && has(h, "username") {
        return pw(F::LastPass);
    }
    if has(h, "vault") && has(h, "createtime") && has(h, "url") {
        return pw(F::Proton);
    }
    if (has(h, "username2") || has(h, "otpsecret")) && has(h, "title") && has(h, "url") {
        return pw(F::Dashlane);
    }
    if has(h, "group") && has(h, "title") && has(h, "username") && has(h, "url") {
        return pw(F::KeePass);
    }
    if has(h, "title")
        && has(h, "username")
        && has(h, "password")
        && first_col(h, &["url", "website", "urls"]).is_some()
    {
        let one_password_only = ["favorite", "archived", "tags", "type", "folder", "website"]
            .iter()
            .any(|c| has(h, c));
        if has(h, "otpauth") && !one_password_only {
            return pw(F::Safari);
        }
        return pw(F::OnePassword);
    }
    if has(h, "name") && has(h, "url") && has(h, "username") && has(h, "password") {
        return pw(F::Chrome);
    }
    if first_col(h, GENERIC_URL).is_some() {
        let has_pw = h
            .iter()
            .any(|x| matches!(x.as_str(), "password" | "pass" | "pwd" | "login_password"));
        return Some(if has_pw {
            CsvKind::Passwords(F::Generic)
        } else {
            CsvKind::List
        });
    }
    None
}

fn layout_for(kind: CsvKind, h: &[String]) -> Layout {
    use PasswordFormat as F;
    let mut l = Layout {
        url: all_cols(h, &["url"]),
        name: first_col(h, &["name", "title"]),
        user: all_cols(h, &["username"]),
        date: Vec::new(),
        kind: None,
        group: None,
        split_commas: false,
        pass: first_col(h, &["password", "login_password", "pass", "pwd"]),
        totp: all_cols(h, &["login_totp", "totp", "otpauth", "otpsecret", "otp"]),
    };
    match kind {
        CsvKind::Passwords(F::Firefox) => {
            l.name = None;
            l.date = all_cols(h, &["timepasswordchanged", "timelastused", "timecreated"]);
        }
        CsvKind::Passwords(F::Bitwarden) => {
            l.url = all_cols(h, &["login_uri"]);
            l.name = first_col(h, &["name"]);
            l.user = all_cols(h, &["login_username"]);
            l.kind = first_col(h, &["type"]);
            l.split_commas = true;
        }
        CsvKind::Passwords(F::LastPass) => l.name = first_col(h, &["name"]),
        CsvKind::Passwords(F::Proton) => {
            l.name = first_col(h, &["name"]);
            l.user = all_cols(h, &["username", "email"]);
            l.date = all_cols(h, &["modifytime", "createtime"]);
            l.kind = first_col(h, &["type"]);
        }
        CsvKind::Passwords(F::Dashlane) => {
            l.name = first_col(h, &["title"]);
            l.user = all_cols(h, &["username", "username2", "email"]);
        }
        CsvKind::Passwords(F::KeePass) => {
            l.name = first_col(h, &["title"]);
            l.date = all_cols(h, &["last modified", "created"]);
            l.group = first_col(h, &["group"]);
        }
        CsvKind::Passwords(F::OnePassword) => {
            l.url = all_cols(h, &["url", "website", "urls"]);
            l.name = first_col(h, &["title"]);
        }
        CsvKind::Passwords(F::Safari) => l.name = first_col(h, &["title"]),
        CsvKind::Passwords(F::Chrome | F::Edge) => l.name = first_col(h, &["name"]),
        CsvKind::Passwords(F::Generic) | CsvKind::List => {
            l.url = all_cols(h, GENERIC_URL);
            l.name = first_col(h, GENERIC_NAME);
            l.user = all_cols(h, GENERIC_USER);
        }
    }
    l
}

/// Build a reader over `text`; returns the reader and the normalised header cells.
fn open(text: &str) -> Result<(csv::Reader<&[u8]>, Vec<String>), ImportError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let count = |c: char| first.matches(c).count();
    let delim = [(b',', count(',')), (b';', count(';')), (b'\t', count('\t'))]
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(d, _)| d)
        .unwrap_or(b',');
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delim)
        .from_reader(text.as_bytes());
    let mut headers = None;
    for rec in rdr.records() {
        let rec = rec.map_err(|e| ImportError::Invalid(format!("CSV: {e}")))?;
        if rec.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        headers = Some(
            rec.iter()
                .map(|c| c.trim().trim_matches('\u{feff}').to_lowercase())
                .collect::<Vec<_>>(),
        );
        break;
    }
    headers
        .map(|h| (rdr, h))
        .ok_or_else(|| ImportError::Invalid("Leere CSV-Datei".into()))
}

/// Cheap header sniffing without reading the rows.
pub(crate) fn sniff_csv(text: &str) -> Option<CsvKind> {
    let (_, h) = open(text).ok()?;
    sniff_headers(&h)
}

/// Import a password CSV; `as_format` re-labels a detected Chrome file (Edge uses the same
/// layout, so only the caller can know, e.g. from the file name).
pub(crate) fn import_passwords(
    text: &str,
    as_format: Option<PasswordFormat>,
) -> Result<ImportResult, ImportError> {
    import_passwords_audit(text, as_format, None)
}

/// Like [`import_passwords`], additionally handing each login row to `audit`.
pub(crate) fn import_passwords_audit(
    text: &str,
    as_format: Option<PasswordFormat>,
    audit: Option<&mut Vec<PwRow>>,
) -> Result<ImportResult, ImportError> {
    let (rdr, headers) = open(text)?;
    let kind = sniff_headers(&headers).ok_or(ImportError::UnknownFormat)?;
    let ctx = match kind {
        CsvKind::Passwords(PasswordFormat::Chrome) => {
            Ctx::Passwords(as_format.unwrap_or(PasswordFormat::Chrome))
        }
        CsvKind::Passwords(f) => Ctx::Passwords(f),
        CsvKind::List => Ctx::Passwords(PasswordFormat::Generic),
    };
    Ok(run(rdr, &headers, kind, &ctx, audit))
}

/// Import a CSV list (URL / domain column) as `GenericList`.
pub(crate) fn import_list_csv(text: &str) -> Result<ImportResult, ImportError> {
    let (rdr, headers) = open(text)?;
    if first_col(&headers, GENERIC_URL).is_none() {
        return Err(ImportError::UnknownFormat);
    }
    Ok(run(rdr, &headers, CsvKind::List, &Ctx::List, None))
}

fn run(
    mut rdr: csv::Reader<&[u8]>,
    headers: &[String],
    kind: CsvKind,
    ctx: &Ctx,
    mut audit: Option<&mut Vec<PwRow>>,
) -> ImportResult {
    let layout = layout_for(kind, headers);
    let mut out = Collector::new(ctx);
    let mut scanned = 0u64;
    let (mut no_url, mut non_login, mut broken) = (0u64, 0u64, 0u64);
    for rec in rdr.records() {
        let Ok(rec) = rec else {
            broken += 1;
            continue;
        };
        if rec.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        scanned += 1;
        let cell = |i: usize| rec.get(i).map(str::trim).unwrap_or("");
        if let Some(k) = layout.kind {
            let t = cell(k).to_lowercase();
            if !t.is_empty() && t != "login" {
                non_login += 1;
                continue;
            }
        }
        if let Some(g) = layout.group {
            let g = cell(g).to_lowercase();
            if g.starts_with("recycle bin") || g.starts_with("papierkorb") {
                non_login += 1;
                continue;
            }
        }
        let mut hosts: Vec<String> = Vec::new();
        let mut secure_note = false;
        for &i in &layout.url {
            let raw = cell(i);
            if raw.eq_ignore_ascii_case("http://sn") || raw.eq_ignore_ascii_case("http://group") {
                secure_note = true;
                continue;
            }
            for part in split_uris(raw, layout.split_commas) {
                if let Some(h) = host_of(part) {
                    if !hosts.contains(&h) {
                        hosts.push(h);
                    }
                }
            }
        }
        if hosts.is_empty() {
            if secure_note {
                non_login += 1;
            } else {
                no_url += 1;
            }
            continue;
        }
        let name = layout.name.map(cell).filter(|s| !s.is_empty());
        let user = layout.user.iter().map(|&i| cell(i)).find(|s| !s.is_empty());
        let date = layout.date.iter().find_map(|&i| parse_ts(cell(i)));
        let password = layout
            .pass
            .map(cell)
            .filter(|s| !s.is_empty())
            .map(|s| Zeroizing::new(s.to_string()));
        let has_totp = layout.totp.iter().any(|&i| !cell(i).is_empty());
        for h in hosts {
            let domain = h.clone();
            if out.push(h, name, user, date) {
                if let (Some(a), Some(p)) = (audit.as_deref_mut(), password.as_ref()) {
                    a.push(PwRow {
                        domain,
                        identifier: user.map(str::to_string),
                        password: p.clone(),
                        has_totp,
                    });
                }
            }
        }
    }
    let mut warnings = Vec::new();
    if no_url > 0 {
        warnings.push(format!("{no_url} Zeilen ohne verwertbare URL übersprungen"));
    }
    if non_login > 0 {
        warnings.push(format!(
            "{non_login} Einträge ohne Login (Notizen, Karten, Papierkorb) übersprungen"
        ));
    }
    if broken > 0 {
        warnings.push(format!("{broken} defekte Zeilen übersprungen"));
    }
    out.finish(scanned, warnings)
}

fn split_uris(raw: &str, commas: bool) -> Vec<&str> {
    raw.split(|c: char| c.is_whitespace() || (commas && c == ','))
        .filter(|s| !s.is_empty())
        .collect()
}

/// De-duplicating finding builder.
struct Collector<'a> {
    ctx: &'a Ctx,
    seen: HashSet<String>,
    findings: Vec<Finding>,
}

impl<'a> Collector<'a> {
    fn new(ctx: &'a Ctx) -> Self {
        Self {
            ctx,
            seen: HashSet::new(),
            findings: Vec::new(),
        }
    }

    fn push(
        &mut self,
        host: String,
        name: Option<&str>,
        user: Option<&str>,
        date: Option<chrono::DateTime<chrono::Utc>>,
    ) -> bool {
        let r = source_ref(&[self.ctx.tag(), self.ctx.label(), &host, user.unwrap_or("")]);
        if !self.seen.insert(r.clone()) {
            return false;
        }
        self.findings.push(Finding {
            domain: host,
            name: name.map(str::to_string),
            identifier: user.map(str::to_string),
            sso: None,
            evidence: Evidence {
                kind: self.ctx.kind(),
                source: self.ctx.source(),
                summary: self.ctx.summary(),
                date,
                source_ref: Some(r),
            },
        });
        true
    }

    fn finish(self, scanned: u64, warnings: Vec<String>) -> ImportResult {
        ImportResult {
            source: self.ctx.source(),
            items_scanned: scanned,
            findings: self.findings,
            warnings,
        }
    }
}

/// Import an unencrypted Bitwarden JSON export.
pub(crate) fn import_bitwarden_json(json: &str) -> Result<ImportResult, ImportError> {
    import_bitwarden_json_audit(json, None)
}

pub(crate) fn import_bitwarden_json_audit(
    json: &str,
    mut audit: Option<&mut Vec<PwRow>>,
) -> Result<ImportResult, ImportError> {
    let json = json.strip_prefix('\u{feff}').unwrap_or(json);
    let v: Value =
        serde_json::from_str(json).map_err(|e| ImportError::Invalid(format!("JSON: {e}")))?;
    if v.get("encrypted").and_then(Value::as_bool) == Some(true) {
        return Err(ImportError::Invalid(
            "Der Bitwarden-Export ist verschlüsselt. Bitte unverschlüsselt (.json) exportieren."
                .into(),
        ));
    }
    let items = v
        .get("items")
        .and_then(Value::as_array)
        .ok_or(ImportError::UnknownFormat)?;
    let ctx = Ctx::Passwords(PasswordFormat::Bitwarden);
    let mut out = Collector::new(&ctx);
    let (mut scanned, mut no_url, mut non_login) = (0u64, 0u64, 0u64);
    for item in items {
        scanned += 1;
        if item.get("type").and_then(Value::as_i64).unwrap_or(1) != 1 {
            non_login += 1;
            continue;
        }
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let login = item.get("login");
        let user = login
            .and_then(|l| l.get("username"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let date = ["revisionDate", "creationDate"]
            .iter()
            .find_map(|k| item.get(*k).and_then(Value::as_str).and_then(parse_ts));
        let hosts: Vec<String> = login
            .and_then(|l| l.get("uris"))
            .and_then(Value::as_array)
            .map(|uris| {
                uris.iter()
                    .filter_map(|u| u.get("uri").and_then(Value::as_str))
                    .filter_map(host_of)
                    .collect()
            })
            .unwrap_or_default();
        if hosts.is_empty() {
            no_url += 1;
            continue;
        }
        let password = login
            .and_then(|l| l.get("password"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(|s| Zeroizing::new(s.to_string()));
        let has_totp = login
            .and_then(|l| l.get("totp"))
            .and_then(Value::as_str)
            .is_some_and(|s| !s.trim().is_empty());
        for h in hosts {
            let domain = h.clone();
            if out.push(h, name, user, date) {
                if let (Some(a), Some(p)) = (audit.as_deref_mut(), password.as_ref()) {
                    a.push(PwRow {
                        domain,
                        identifier: user.map(str::to_string),
                        password: p.clone(),
                        has_totp,
                    });
                }
            }
        }
    }
    let mut warnings = Vec::new();
    if no_url > 0 {
        warnings.push(format!(
            "{no_url} Einträge ohne verwertbare URL übersprungen"
        ));
    }
    if non_login > 0 {
        warnings.push(format!(
            "{non_login} Einträge ohne Login (Notizen, Karten, Identitäten) übersprungen"
        ));
    }
    Ok(out.finish(scanned, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pw(csv: &str) -> ImportResult {
        import_passwords(csv, None).unwrap()
    }

    #[test]
    fn semicolon_csv_with_leading_blank_lines() {
        let r = pw("\n\nname;url;username;password;note\nSite;https://a.example.com;u;p;\n");
        assert_eq!(r.findings.len(), 1);
        assert_eq!(r.findings[0].domain, "a.example.com");
    }

    #[test]
    fn chrome_bom_crlf() {
        let csv = "\u{feff}name,url,username,password,note\r\nExample,https://www.example.com/login,alice@example.org,SECRET1,\"multi\r\nline note\"\r\nAndroid,android://hash@com.app/,bob,SECRET2,\r\n";
        let r = pw(csv);
        assert_eq!(r.items_scanned, 2);
        assert_eq!(r.findings.len(), 2);
        assert_eq!(r.findings[1].domain, "app.com");
        let f = &r.findings[0];
        assert_eq!(f.domain, "www.example.com");
        assert_eq!(f.name.as_deref(), Some("Example"));
        assert_eq!(f.identifier.as_deref(), Some("alice@example.org"));
        assert_eq!(f.evidence.kind, EvidenceKind::SavedPassword);
        assert_eq!(f.evidence.summary, "Gespeichert in Chrome");
        assert_eq!(
            f.evidence.source,
            SourceKind::PasswordExport {
                format: PasswordFormat::Chrome
            }
        );
        let dump = format!("{r:?}");
        assert!(!dump.contains("SECRET") && !dump.contains("note"));
    }

    #[test]
    fn source_ref_ignores_password() {
        let a = pw("name,url,username,password\nX,https://a.example.com,u,one\n");
        let b = pw("name,url,username,password\nX,https://a.example.com,u,two\n");
        assert_eq!(
            a.findings[0].evidence.source_ref,
            b.findings[0].evidence.source_ref
        );
    }

    #[test]
    fn firefox() {
        let csv = "\"url\",\"username\",\"password\",\"httpRealm\",\"formActionOrigin\",\"guid\",\"timeCreated\",\"timeLastUsed\",\"timePasswordChanged\"\n\"https://shop.example.com\",\"carol\",\"pw\",\"\",\"https://shop.example.com\",\"{g}\",\"1600000000000\",\"1650000000000\",\"1700000000000\"\n\"chrome://FirefoxAccounts\",\"x\",\"pw\",\"\",\"\",\"{h}\",\"1\",\"1\",\"1\"\n";
        let r = pw(csv);
        assert_eq!(r.findings.len(), 1);
        let f = &r.findings[0];
        assert_eq!(f.domain, "shop.example.com");
        assert_eq!(f.evidence.date.unwrap().timestamp(), 1_700_000_000);
        assert_eq!(f.evidence.summary, "Gespeichert in Firefox");
    }

    #[test]
    fn safari_and_one_password() {
        let s = pw("Title,URL,Username,Password,Notes,OTPAuth\nSite,https://safari.example.com,dave,pw,,\n");
        assert_eq!(
            s.source,
            SourceKind::PasswordExport {
                format: PasswordFormat::Safari
            }
        );
        let o = pw("Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes\nSite,https://op.example.com,erin,pw,,0,0,,\n");
        assert_eq!(
            o.source,
            SourceKind::PasswordExport {
                format: PasswordFormat::OnePassword
            }
        );
        assert_eq!(o.findings[0].domain, "op.example.com");
    }

    #[test]
    fn bitwarden_csv_multiline_uris() {
        let csv = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n,0,login,Multi,\"secret note\",,0,\"https://one.example.com\nhttps://two.example.com,https://three.example.com\",frank,pw,TOTPSECRET\n,0,note,A Note,text,,0,,,,\n,0,login,Dup,,,0,https://one.example.com,frank,pw,\n";
        let r = pw(csv);
        let hosts: Vec<_> = r.findings.iter().map(|f| f.domain.as_str()).collect();
        assert_eq!(
            hosts,
            ["one.example.com", "two.example.com", "three.example.com"]
        );
        assert!(r.warnings.iter().any(|w| w.contains("ohne Login")));
        assert!(!format!("{r:?}").contains("TOTPSECRET"));
    }

    #[test]
    fn bitwarden_json() {
        let json = r#"{"encrypted":false,"folders":[],"items":[
          {"type":1,"name":"Bw","revisionDate":"2023-05-01T10:00:00.000Z","login":{"username":"gina","password":"SECRET","uris":[{"uri":"https://bw.example.com/x"},{"uri":"https://bw2.example.com"}]}},
          {"type":2,"name":"Note","secureNote":{}},
          {"type":1,"name":"NoUri","login":{"username":"x"}}]}"#;
        let r = import_bitwarden_json(json).unwrap();
        assert_eq!(r.findings.len(), 2);
        assert!(r.findings[0].evidence.date.is_some());
        assert!(!format!("{r:?}").contains("SECRET"));
        assert!(import_bitwarden_json(r#"{"encrypted":true,"data":"x"}"#).is_err());
    }

    #[test]
    fn keepass() {
        let csv = "\"Group\",\"Title\",\"Username\",\"Password\",\"URL\",\"Notes\",\"TOTP\",\"Icon\",\"Last Modified\",\"Created\"\n\"Root\",\"Kp\",\"hank\",\"pw\",\"https://kp.example.com\",\"\",\"\",\"0\",\"2023-02-03T04:05:06Z\",\"\"\n\"Recycle Bin\",\"Old\",\"x\",\"pw\",\"https://old.example.com\",\"\",\"\",\"0\",\"\",\"\"\n";
        let r = pw(csv);
        assert_eq!(r.findings.len(), 1);
        assert!(r.findings[0].evidence.date.is_some());
    }

    #[test]
    fn lastpass_skips_notes() {
        let csv = "url,username,password,totp,extra,name,grouping,fav\nhttps://lp.example.com,ida,pw,,,LP,,0\nhttp://sn,,,,secret,Note,,0\n";
        let r = pw(csv);
        assert_eq!(r.findings.len(), 1);
        assert_eq!(r.findings[0].name.as_deref(), Some("LP"));
    }

    #[test]
    fn proton_and_dashlane() {
        let p = pw("type,name,url,email,username,password,note,totp,createTime,modifyTime,vault\nlogin,Pr,https://pr.example.com,jan@example.org,,pw,,,1600000000000,1700000000000,Personal\nalias,Al,,x@y.z,,,,,1,1,Personal\n");
        assert_eq!(p.findings.len(), 1);
        assert_eq!(p.findings[0].identifier.as_deref(), Some("jan@example.org"));
        assert_eq!(
            p.findings[0].evidence.date.unwrap().timestamp(),
            1_700_000_000
        );
        let d = pw("username,username2,username3,title,password,note,url,category,otpSecret\nkim,,,Da,pw,,https://da.example.com,Shopping,\n");
        assert_eq!(
            d.source,
            SourceKind::PasswordExport {
                format: PasswordFormat::Dashlane
            }
        );
        assert_eq!(d.findings[0].identifier.as_deref(), Some("kim"));
    }

    #[test]
    fn generic_fallback_and_list() {
        let g = pw("Site;Login;Pass\nhttps://g.example.com;lea;pw\n");
        assert_eq!(
            g.source,
            SourceKind::PasswordExport {
                format: PasswordFormat::Generic
            }
        );
        assert_eq!(g.findings[0].identifier.as_deref(), Some("lea"));
        let l = import_list_csv("domain,comment\nlist.example.com,x\n").unwrap();
        assert_eq!(l.source, SourceKind::GenericList);
        assert_eq!(l.findings[0].evidence.kind, EvidenceKind::DataExport);
    }

    #[test]
    fn unknown_header() {
        assert!(matches!(
            import_passwords("a,b,c\n1,2,3\n", None),
            Err(ImportError::UnknownFormat)
        ));
    }
}
