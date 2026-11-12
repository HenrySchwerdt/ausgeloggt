//! Importers that turn local files into [`Finding`]s.
//!
//! Privacy rule: importers NEVER keep passwords, notes, TOTP secrets or card data.
//! Only domain/URL, entry title and username/e-mail leave this crate.

mod archive;
mod audit;
mod facebook;
mod generic;
mod instructions;
mod passwords;
mod takeout;
#[cfg(test)]
mod testutil;
mod util;

use std::fs::File;
use std::io::Read;
use std::path::Path;

use ausgeloggt_core::{Finding, PasswordFormat, SourceKind};

use crate::archive::{Archive, MAX_FILE_BYTES};
use crate::passwords::{CsvKind, PwRow};
use crate::util::{decode_text, host_of};

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("Dateiformat nicht erkannt")]
    UnknownFormat,
    #[error("Datei konnte nicht gelesen werden: {0}")]
    Io(#[from] std::io::Error),
    #[error("Ungültige Daten: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone)]
pub struct ImportResult {
    pub source: SourceKind,
    /// Rows / entries looked at.
    pub items_scanned: u64,
    pub findings: Vec<Finding>,
    /// Non-fatal problems, shown to the user (e.g. "12 Zeilen ohne URL übersprungen").
    pub warnings: Vec<String>,
}

/// What kind of file the user dropped in. Used by the UI for a nice label before import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectedKind {
    Passwords(PasswordFormat),
    GoogleTakeout,
    FacebookExport,
    GenericList,
}

/// Bytes sniffed from a plain file by [`detect`].
const SNIFF_BYTES: u64 = 256 * 1024;

fn ext_of(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Edge exports use the Chrome layout; the file name is the only hint.
fn edge_hint(path: &Path) -> Option<PasswordFormat> {
    let stem = path.file_stem()?.to_string_lossy().to_lowercase();
    stem.contains("edge").then_some(PasswordFormat::Edge)
}

fn is_zip_magic(head: &[u8]) -> bool {
    head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06")
}

fn looks_like_bitwarden_json(text: &str) -> bool {
    text.contains("\"items\"") && (text.contains("\"encrypted\"") || text.contains("\"folders\""))
}

/// Classify text content (CSV / JSON / TXT) without importing it.
fn detect_text(ext: &str, text: &str) -> Option<DetectedKind> {
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Some(if looks_like_bitwarden_json(trimmed) {
            DetectedKind::Passwords(PasswordFormat::Bitwarden)
        } else {
            DetectedKind::GenericList
        });
    }
    if ext == "txt" {
        return Some(DetectedKind::GenericList);
    }
    match passwords::sniff_csv(text) {
        Some(CsvKind::Passwords(f)) => return Some(DetectedKind::Passwords(f)),
        Some(CsvKind::List) => return Some(DetectedKind::GenericList),
        None => {}
    }
    // Headerless list with one domain per line.
    let first = trimmed.lines().find(|l| !l.trim().is_empty())?;
    host_of(first.trim()).map(|_| DetectedKind::GenericList)
}

fn classify_archive(names: &[String]) -> Option<DetectedKind> {
    if facebook::looks_like(names) {
        Some(DetectedKind::FacebookExport)
    } else if takeout::looks_like(names) {
        Some(DetectedKind::GoogleTakeout)
    } else {
        None
    }
}

fn read_head(path: &Path, max: u64) -> Result<Vec<u8>, ImportError> {
    let mut buf = Vec::new();
    File::open(path)?.take(max).read_to_end(&mut buf)?;
    Ok(buf)
}

/// Look at a file (CSV, JSON, ZIP or folder) and guess what it is without importing.
pub fn detect(path: &Path) -> Result<DetectedKind, ImportError> {
    if path.is_dir() {
        let archive = Archive::from_dir(path)?;
        return classify_archive(&archive.names()).ok_or(ImportError::UnknownFormat);
    }
    let head = read_head(path, SNIFF_BYTES)?;
    if is_zip_magic(&head) {
        let archive = Archive::from_zip(File::open(path)?)?;
        return classify_archive(&archive.names()).ok_or(ImportError::UnknownFormat);
    }
    let kind = detect_text(&ext_of(path), &decode_text(&head)).ok_or(ImportError::UnknownFormat)?;
    Ok(match (kind, edge_hint(path)) {
        (DetectedKind::Passwords(PasswordFormat::Chrome), Some(edge)) => {
            DetectedKind::Passwords(edge)
        }
        (k, _) => k,
    })
}

fn import_archive(mut archive: Archive) -> Result<ImportResult, ImportError> {
    match classify_archive(&archive.names()) {
        Some(DetectedKind::FacebookExport) => facebook::import(&mut archive),
        Some(DetectedKind::GoogleTakeout) => takeout::import(&mut archive),
        _ => Err(ImportError::UnknownFormat),
    }
}

/// Import any supported file or folder; auto-detects the format.
pub fn import_path(path: &Path) -> Result<ImportResult, ImportError> {
    import_path_inner(path, None)
}

fn import_path_inner(
    path: &Path,
    audit: Option<&mut Vec<PwRow>>,
) -> Result<ImportResult, ImportError> {
    if path.is_dir() {
        return import_archive(Archive::from_dir(path)?);
    }
    if std::fs::metadata(path)?.len() > MAX_FILE_BYTES * 5 {
        return Err(ImportError::Invalid("Datei ist zu groß".into()));
    }
    let head = read_head(path, 4)?;
    if is_zip_magic(&head) {
        return import_archive(Archive::from_zip(File::open(path)?)?);
    }
    let text = decode_text(&read_head(path, MAX_FILE_BYTES)?);
    match detect_text(&ext_of(path), &text).ok_or(ImportError::UnknownFormat)? {
        DetectedKind::Passwords(PasswordFormat::Bitwarden)
            if text
                .trim_start_matches('\u{feff}')
                .trim_start()
                .starts_with('{') =>
        {
            passwords::import_bitwarden_json_audit(&text, audit)
        }
        DetectedKind::Passwords(_) => {
            passwords::import_passwords_audit(&text, edge_hint(path), audit)
        }
        DetectedKind::GenericList => import_list_text(&ext_of(path), &text),
        DetectedKind::GoogleTakeout | DetectedKind::FacebookExport => {
            Err(ImportError::UnknownFormat)
        }
    }
}

fn import_list_text(ext: &str, text: &str) -> Result<ImportResult, ImportError> {
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        generic::import_json(text)
    } else if ext == "txt" {
        Ok(generic::import_txt(text))
    } else {
        // CSV with a header, or a headerless one-domain-per-line file.
        generic::import_csv(text).or_else(|_| Ok(generic::import_txt(text)))
    }
}

/// Import a password-manager / browser CSV export from memory (auto-detected format).
pub fn import_password_csv(csv: &str) -> Result<ImportResult, ImportError> {
    passwords::import_passwords(csv, None)
}

/// Import an unencrypted Bitwarden JSON export from memory.
pub fn import_bitwarden_json(json: &str) -> Result<ImportResult, ImportError> {
    passwords::import_bitwarden_json(json)
}

/// Import a Google Takeout / Facebook export ZIP held in memory.
pub fn import_zip_bytes(bytes: Vec<u8>) -> Result<ImportResult, ImportError> {
    import_archive(Archive::from_zip(std::io::Cursor::new(bytes))?)
}

/// Checks a password SHA-1 against a breach corpus (implemented by `ausgeloggt-breach`
/// via the HIBP k-anonymity range API). Returns how often it was seen, `None` on error.
pub trait PwnedChecker {
    fn pwned_count(&self, sha1: &[u8; 20]) -> Option<u64>;
}

#[derive(Default)]
pub struct ImportOptions<'a> {
    /// If set, every distinct password is checked once (SHA-1 computed in memory,
    /// zeroized right after). `None` = no network, reuse analysis only.
    pub pwned: Option<&'a dyn PwnedChecker>,
}

/// Result of [`import_path_with`]: the normal import plus a password audit.
#[derive(Debug, Clone)]
pub struct AuditedImport {
    pub result: ImportResult,
    /// Per (domain, identifier): reuse count, pwned flag, TOTP present -> has_2fa.
    /// Contains NO passwords or hashes. Empty for non-password sources.
    pub security: Vec<ausgeloggt_core::SecurityObservation>,
}

/// Import with password audit. Reuse detection compares passwords in memory only
/// (keyed by a per-run random salted hash, all buffers zeroized); nothing about the
/// password itself leaves this function except counts / booleans.
pub fn import_path_with(
    path: &Path,
    options: &ImportOptions<'_>,
) -> Result<AuditedImport, ImportError> {
    let mut rows: Vec<PwRow> = Vec::new();
    let result = import_path_inner(path, Some(&mut rows))?;
    let security = audit::audit(rows, options.pwned);
    Ok(AuditedImport { result, security })
}

/// Human readable help: how to create an export for each supported source (German).
pub fn export_instructions() -> Vec<(&'static str, &'static str)> {
    instructions::all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::zip_of;
    use ausgeloggt_core::EvidenceKind;

    #[test]
    fn instructions_cover_sources() {
        let i = export_instructions();
        for t in [
            "Chrome",
            "Firefox",
            "Safari",
            "Edge",
            "Bitwarden",
            "1Password",
            "KeePass",
            "Proton",
            "Takeout",
            "Facebook",
        ] {
            assert!(i.iter().any(|(title, _)| title.contains(t)), "{t}");
        }
        assert!(i.iter().all(|(a, b)| !a.is_empty() && !b.is_empty()));
    }

    #[test]
    fn file_detection_and_import() {
        let dir = tempfile::tempdir().unwrap();
        let chrome = dir.path().join("Chrome Passwords.csv");
        std::fs::write(
            &chrome,
            "name,url,username,password\nA,https://a.example.com,u,p\n",
        )
        .unwrap();
        assert_eq!(
            detect(&chrome).unwrap(),
            DetectedKind::Passwords(PasswordFormat::Chrome)
        );
        let edge = dir.path().join("Edge Passwords.csv");
        std::fs::write(
            &edge,
            "name,url,username,password,note\nA,https://a.example.com,u,p,\n",
        )
        .unwrap();
        assert_eq!(
            detect(&edge).unwrap(),
            DetectedKind::Passwords(PasswordFormat::Edge)
        );
        let r = import_path(&edge).unwrap();
        assert_eq!(
            r.source,
            SourceKind::PasswordExport {
                format: PasswordFormat::Edge
            }
        );

        let bw = dir.path().join("bw.json");
        std::fs::write(&bw, r#"{"encrypted":false,"items":[{"type":1,"name":"X","login":{"username":"u","uris":[{"uri":"https://x.example.com"}]}}]}"#).unwrap();
        assert_eq!(
            detect(&bw).unwrap(),
            DetectedKind::Passwords(PasswordFormat::Bitwarden)
        );
        assert_eq!(import_path(&bw).unwrap().findings.len(), 1);

        let txt = dir.path().join("list.txt");
        std::fs::write(&txt, "# c\nfoo.example.com\n").unwrap();
        assert_eq!(detect(&txt).unwrap(), DetectedKind::GenericList);
        let r = import_path(&txt).unwrap();
        assert_eq!(r.findings[0].evidence.kind, EvidenceKind::DataExport);

        let lst = dir.path().join("list.csv");
        std::fs::write(&lst, "url,comment\nhttps://l.example.com,x\n").unwrap();
        assert_eq!(detect(&lst).unwrap(), DetectedKind::GenericList);
        assert_eq!(import_path(&lst).unwrap().source, SourceKind::GenericList);

        let junk = dir.path().join("junk.bin");
        std::fs::write(&junk, "\u{1}\u{2}\u{3}").unwrap();
        assert!(matches!(detect(&junk), Err(ImportError::UnknownFormat)));
    }

    #[test]
    fn zip_file_and_folder() {
        let dir = tempfile::tempdir().unwrap();
        let apps = r#"{"installed_apps_v2":[{"name":"Foo App","added_timestamp":1600000000}]}"#;
        let rel =
            "your_activity/apps_and_websites_off_of_facebook/connected_apps_and_websites.json";
        let zip = dir.path().join("fb.zip");
        std::fs::write(&zip, zip_of(&[(rel, apps)])).unwrap();
        assert_eq!(detect(&zip).unwrap(), DetectedKind::FacebookExport);
        let r = import_path(&zip).unwrap();
        assert_eq!(r.findings[0].domain, "unresolved:foo-app");

        let folder = dir.path().join("fbdir");
        let full = folder.join(rel);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(&full, apps).unwrap();
        assert_eq!(detect(&folder).unwrap(), DetectedKind::FacebookExport);
        assert_eq!(import_path(&folder).unwrap().findings.len(), 1);

        let tk = zip_of(&[(
            "Takeout/My Activity/Sign in with Google/a.json",
            r#"[{"titleUrl":"https://t.example.com"}]"#,
        )]);
        assert_eq!(import_zip_bytes(tk).unwrap().findings.len(), 1);
    }

    struct Fake;
    impl PwnedChecker for Fake {
        fn pwned_count(&self, sha1: &[u8; 20]) -> Option<u64> {
            use sha1::Digest;
            let h = |s: &str| -> [u8; 20] { sha1::Sha1::digest(s.as_bytes()).into() };
            if *sha1 == h("hunter2") {
                Some(42)
            } else if *sha1 == h("boom") {
                None
            } else {
                Some(0)
            }
        }
    }

    fn audited(name: &str, content: &str, pwned: bool) -> AuditedImport {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(name);
        std::fs::write(&p, content).unwrap();
        let fake = Fake;
        let opts = ImportOptions {
            pwned: pwned.then_some(&fake as &dyn PwnedChecker),
        };
        import_path_with(&p, &opts).unwrap()
    }

    #[test]
    fn audit_reuse_pwned_and_errors() {
        let csv = "name,url,username,password
A,https://a.example.com,u1,hunter2
B,https://b.example.com,u2,hunter2
C,https://c.example.com,u3,hunter2
D,https://d.example.com,u4,Unique-Pass-9
E,https://e.example.com,u5,boom
";
        let a = audited("c.csv", csv, true);
        assert_eq!(a.security.len(), 5);
        let by = |d: &str| a.security.iter().find(|s| s.domain == d).unwrap();
        assert_eq!(by("a.example.com").password_reuse_count, Some(2));
        assert_eq!(by("c.example.com").password_reuse_count, Some(2));
        assert_eq!(by("a.example.com").password_pwned, Some(true));
        assert_eq!(by("a.example.com").identifier.as_deref(), Some("u1"));
        assert_eq!(by("d.example.com").password_reuse_count, Some(0));
        assert_eq!(by("d.example.com").password_pwned, Some(false));
        assert_eq!(by("e.example.com").password_pwned, None);
        assert_eq!(by("d.example.com").has_2fa, None);
        let dump = format!("{a:?}");
        for p in ["hunter2", "Unique-Pass-9", "boom"] {
            assert!(!dump.contains(p), "{p}");
        }
        let b = audited("c.csv", csv, false);
        assert!(b.security.iter().all(|s| s.password_pwned.is_none()));
        assert_eq!(b.result.findings.len(), 5);
    }

    #[test]
    fn audit_totp_per_format() {
        let bw = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp
,0,login,A,,,0,https://t.example.com,u,pw1,TOTPSECRET
,0,login,B,,,0,https://n.example.com,u,pw2,
";
        let a = audited("bw.csv", bw, false);
        let by = |d: &str| a.security.iter().find(|s| s.domain == d).unwrap();
        assert_eq!(by("t.example.com").has_2fa, Some(true));
        assert_eq!(by("n.example.com").has_2fa, None);
        assert!(!format!("{a:?}").contains("TOTPSECRET"));

        let safari = "Title,URL,Username,Password,Notes,OTPAuth
S,https://s.example.com,u,pw,,otpauth://totp/x?secret=ABC
";
        assert_eq!(
            audited("s.csv", safari, false).security[0].has_2fa,
            Some(true)
        );
        let op = "Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes
S,https://o.example.com,u,pw,otpauth://x,0,0,,
";
        assert_eq!(audited("o.csv", op, false).security[0].has_2fa, Some(true));
        let lp = "url,username,password,totp,extra,name,grouping,fav
https://l.example.com,u,pw,123456,,L,,0
";
        assert_eq!(audited("l.csv", lp, false).security[0].has_2fa, Some(true));
        let pr = "type,name,url,email,username,password,note,totp,createTime,modifyTime,vault
login,P,https://p.example.com,a@b.c,,pw,,SECRET,1,1,Personal
";
        assert_eq!(audited("p.csv", pr, false).security[0].has_2fa, Some(true));
        let kp = "\"Group\",\"Title\",\"Username\",\"Password\",\"URL\",\"Notes\",\"TOTP\",\"Icon\"
\"Root\",\"K\",\"h\",\"pw\",\"https://k.example.com\",\"\",\"JBSWY3DP\",\"0\"
";
        assert_eq!(audited("k.csv", kp, false).security[0].has_2fa, Some(true));
        let json = r#"{"encrypted":false,"items":[{"type":1,"name":"J","login":{"username":"u","password":"jsonpw","totp":"otpauth://totp/x","uris":[{"uri":"https://j.example.com"}]}},{"type":1,"name":"K","login":{"username":"u","password":"jsonpw","uris":[{"uri":"https://k.example.com"}]}}]}"#;
        let j = audited("b.json", json, false);
        assert_eq!(j.security[0].has_2fa, Some(true));
        assert_eq!(j.security[1].has_2fa, None);
        assert_eq!(j.security[0].password_reuse_count, Some(1));
        assert!(!format!("{j:?}").contains("jsonpw"));
    }

    #[test]
    fn audit_empty_for_non_password_sources() {
        let a = audited(
            "l.txt",
            "foo.example.com
",
            true,
        );
        assert!(a.security.is_empty());
        assert_eq!(a.result.findings.len(), 1);
    }

    #[test]
    fn unsafe_zip_names_are_ignored() {
        let z = zip_of(&[
            ("../evil.json", "{}"),
            ("/abs.json", "{}"),
            ("ok/apps_and_websites_off_of_facebook/a.json", "{}"),
        ]);
        let a = Archive::from_zip(std::io::Cursor::new(z)).unwrap();
        assert_eq!(
            a.names(),
            vec!["ok/apps_and_websites_off_of_facebook/a.json"]
        );
    }
}
