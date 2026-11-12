//! Google Takeout (ZIP or extracted folder).
//!
//! What Takeout really contains is limited: it does NOT include saved passwords (those
//! live in passwords.google.com / Chrome) and has no complete list of "Sign in with
//! Google" connections (that list is only at myaccount.google.com/connections). What we
//! can do, tolerantly and independent of the UI language:
//!
//! * Look at files whose *path* hints at sign-in / third-party / connected-app data
//!   (e.g. "My Activity/Sign in with Google", "Mein Aktivitätsprotokoll/Mit Google
//!   anmelden", "Connected apps", "Drittanbieter") and collect every external web host
//!   mentioned in their JSON / CSV / HTML.
//! * Skip Google-owned hosts. Each host becomes one `DataExport` finding; for
//!   "sign in with Google" paths the finding also carries `SsoProvider::Google`.
//! * Dates come from `time` / `timestamp` fields next to a URL when present.
//!
//! If nothing relevant is found the import succeeds with zero findings and a warning.
//! Chrome history / bookmarks are deliberately not mined (too noisy: every visited
//! site would become an "account").

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde_json::Value;
use spurlos_core::{Evidence, EvidenceKind, Finding, SourceKind, SsoProvider};

use crate::archive::Archive;
use crate::util::{decode_text, host_of, hosts_in_text, parse_ts, source_ref};
use crate::{ImportError, ImportResult};

/// Path fragments (lowercase) that mark "sign in with Google" activity.
const SSO_HINTS: &[&str] = &[
    "sign in with google",
    "signed in with google",
    "sign-in with google",
    "mit google anmelden",
    "anmelden mit google",
    "connexion avec google",
    "iniciar sesión con google",
    "iniciar sesion con google",
    "accedi con google",
];

/// Further path fragments for connected / third-party apps.
const APP_HINTS: &[&str] = &[
    "connected apps",
    "third-party",
    "third party",
    "drittanbieter",
    "verbundene apps",
    "apps mit zugriff",
    "applications connectées",
    "aplicaciones conectadas",
];

pub(crate) fn looks_like(names: &[String]) -> bool {
    names.iter().any(|n| {
        let l = n.to_lowercase();
        l.split('/').take(2).any(|seg| seg == "takeout") || l.ends_with("archive_browser.html")
    })
}

fn text_file(name: &str) -> bool {
    let l = name.to_lowercase();
    [".json", ".csv", ".html", ".htm", ".txt"]
        .iter()
        .any(|e| l.ends_with(e))
        && !l.ends_with("archive_browser.html")
}

fn google_owned(host: &str) -> bool {
    const OWN: &[&str] = &[
        "google.com",
        "google.de",
        "gstatic.com",
        "googleapis.com",
        "googleusercontent.com",
        "googleadservices.com",
        "googlesyndication.com",
        "gvt1.com",
        "ggpht.com",
        "withgoogle.com",
        "g.co",
        "goo.gl",
    ];
    OWN.iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

fn json_hosts(
    v: &Value,
    date: Option<DateTime<Utc>>,
    out: &mut Vec<(String, Option<DateTime<Utc>>)>,
    depth: usize,
) {
    if depth > 24 {
        return;
    }
    match v {
        Value::Array(a) => a.iter().for_each(|x| json_hosts(x, date, out, depth + 1)),
        Value::Object(o) => {
            let d = ["time", "timestamp"]
                .iter()
                .find_map(|k| o.get(*k).and_then(Value::as_str).and_then(parse_ts))
                .or(date);
            o.values().for_each(|x| json_hosts(x, d, out, depth + 1));
        }
        Value::String(s) => {
            if s.contains("://") {
                for h in hosts_in_text(s) {
                    out.push((h, date));
                }
            } else if let Some(h) = host_of(s).filter(|_| s.contains('.') && !s.contains(' ')) {
                // Bare domains only in plain strings that look like one.
                if s.trim().eq_ignore_ascii_case(&h) {
                    out.push((h, date));
                }
            }
        }
        _ => {}
    }
}

pub(crate) fn import(archive: &mut Archive) -> Result<ImportResult, ImportError> {
    let relevant: Vec<(String, bool)> = archive
        .names()
        .into_iter()
        .filter(|n| text_file(n))
        .filter_map(|n| {
            let l = n.to_lowercase();
            let sso = SSO_HINTS.iter().any(|h| l.contains(h));
            let app = APP_HINTS.iter().any(|h| l.contains(h));
            (sso || app).then_some((n, sso))
        })
        .collect();

    let mut warnings = Vec::new();
    if relevant.is_empty() {
        warnings.push(
            "Im Takeout wurden keine Dateien zu „Mit Google anmelden“ oder verbundenen Apps \
             gefunden. Takeout enthält keine gespeicherten Passwörter; die Liste der \
             verbundenen Apps findest du unter myaccount.google.com/connections."
                .to_string(),
        );
    }

    // host -> (latest date, via sign-in-with-google)
    let mut hosts: HashMap<String, (Option<DateTime<Utc>>, bool)> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for (name, sso) in &relevant {
        let Some(bytes) = archive.read(name)? else {
            warnings.push("Eine sehr große Datei wurde übersprungen".to_string());
            continue;
        };
        let text = decode_text(&bytes);
        let found: Vec<(String, Option<DateTime<Utc>>)> = if name.to_lowercase().ends_with(".json")
        {
            let mut v = Vec::new();
            if let Ok(json) = serde_json::from_str::<Value>(&text) {
                json_hosts(&json, None, &mut v, 0);
            }
            v
        } else {
            hosts_in_text(&text)
                .into_iter()
                .map(|h| (h, None))
                .collect()
        };
        for (h, d) in found {
            if google_owned(&h) {
                continue;
            }
            let e = hosts.entry(h.clone()).or_insert_with(|| {
                order.push(h.clone());
                (None, false)
            });
            e.0 = e.0.max(d);
            e.1 |= *sso;
        }
    }

    let findings: Vec<Finding> = order
        .into_iter()
        .map(|h| {
            let (date, sso) = hosts[&h];
            let r = source_ref(&["takeout", &h]);
            Finding {
                domain: h,
                name: None,
                identifier: None,
                sso: sso.then_some(SsoProvider::Google),
                evidence: Evidence {
                    kind: EvidenceKind::DataExport,
                    source: SourceKind::GoogleTakeout,
                    summary: if sso {
                        "Im Google-Takeout: Anmeldung mit Google".into()
                    } else {
                        "Im Google-Takeout: verbundene App/Website".into()
                    },
                    date,
                    source_ref: Some(r),
                },
            }
        })
        .collect();
    Ok(ImportResult {
        source: SourceKind::GoogleTakeout,
        items_scanned: relevant.len() as u64,
        findings,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::zip_of;

    #[test]
    fn takeout_zip() {
        let z = zip_of(&[
            (
                "Takeout/archive_browser.html",
                "<a href=\"https://takeout.google.com\">x</a>",
            ),
            (
                "Takeout/My Activity/Sign in with Google/MyActivity.json",
                r#"[{"header":"Sign in with Google","title":"Signed in to Foo","titleUrl":"https://www.foo.example.com/login","time":"2023-04-05T06:07:08.000Z"},
                   {"titleUrl":"https://accounts.google.com/x"}]"#,
            ),
            (
                "Takeout/Mein Aktivitätsprotokoll/Drittanbieter/apps.html",
                "<a href=\"https://bar.example.org/app\">Bar</a>",
            ),
            (
                "Takeout/Chrome/BrowserHistory.json",
                r#"{"url":"https://history.example.com"}"#,
            ),
        ]);
        let mut a = Archive::from_zip(std::io::Cursor::new(z)).unwrap();
        assert!(looks_like(&a.names()));
        let r = import(&mut a).unwrap();
        let d: Vec<_> = r.findings.iter().map(|f| f.domain.as_str()).collect();
        assert_eq!(d, ["www.foo.example.com", "bar.example.org"]);
        assert_eq!(r.findings[0].sso, Some(SsoProvider::Google));
        assert!(r.findings[0].evidence.date.is_some());
        assert_eq!(r.findings[1].sso, None);
        assert_eq!(r.findings[1].evidence.source, SourceKind::GoogleTakeout);
    }

    #[test]
    fn nothing_relevant_warns() {
        let z = zip_of(&[("Takeout/Drive/a.txt", "https://x.example.com")]);
        let mut a = Archive::from_zip(std::io::Cursor::new(z)).unwrap();
        let r = import(&mut a).unwrap();
        assert!(r.findings.is_empty());
        assert_eq!(r.warnings.len(), 1);
    }
}
