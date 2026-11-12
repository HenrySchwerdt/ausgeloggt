//! Facebook / Meta "Download your information" export (JSON format).
//!
//! Reads every `.json` file inside an `apps_and_websites_off_of_facebook` folder and walks
//! it for objects that carry a `name` (apps / websites the user logged into with Facebook).
//!
//! Limitations: HTML exports are not parsed (the user must choose JSON). Files about
//! off-Facebook *tracking* (`your_off-facebook_activity.json`) and posts from apps are
//! skipped on purpose: they list advertisers, not accounts. Facebook writes UTF-8 as
//! Latin-1 escapes ("mojibake"); that is repaired for every name.

use std::collections::HashSet;

use serde_json::Value;
use spurlos_core::merge::UNRESOLVED_PREFIX;
use spurlos_core::{Evidence, EvidenceKind, Finding, SourceKind, SsoProvider};

use crate::archive::Archive;
use crate::util::{decode_text, epoch, fix_mojibake, host_of, slug, source_ref};
use crate::{ImportError, ImportResult};

pub(crate) const FOLDER: &str = "apps_and_websites_off_of_facebook";

/// Does the list of entry names look like a Facebook export?
pub(crate) fn looks_like(names: &[String]) -> bool {
    names.iter().any(|n| {
        let l = n.to_lowercase();
        l.split('/').any(|seg| seg.starts_with(FOLDER))
    })
}

struct App {
    name: String,
    domain: Option<String>,
    ts: Option<i64>,
}

const TS_KEYS: &[&str] = &[
    "added_timestamp",
    "timestamp",
    "time",
    "start_timestamp",
    "removed_timestamp",
];

fn event_ts(o: &serde_json::Map<String, Value>) -> Option<i64> {
    TS_KEYS
        .iter()
        .find_map(|k| o.get(*k).and_then(Value::as_i64))
        .or_else(|| {
            o.get("events").and_then(Value::as_array).and_then(|ev| {
                ev.iter()
                    .filter_map(|e| e.get("timestamp").and_then(Value::as_i64))
                    .max()
            })
        })
}

fn walk(v: &Value, out: &mut Vec<App>, depth: usize) {
    if depth > 24 {
        return;
    }
    match v {
        Value::Array(a) => a.iter().for_each(|x| walk(x, out, depth + 1)),
        Value::Object(o) => {
            let name = ["name", "app_name"]
                .iter()
                .find_map(|k| o.get(*k).and_then(Value::as_str))
                .map(|s| fix_mojibake(s.trim()))
                .filter(|s| !s.is_empty());
            if let Some(name) = name {
                let domain = ["domain", "url", "website"]
                    .iter()
                    .find_map(|k| o.get(*k).and_then(Value::as_str))
                    .and_then(host_of);
                out.push(App {
                    name,
                    domain,
                    ts: event_ts(o),
                });
            } else {
                o.values().for_each(|x| walk(x, out, depth + 1));
            }
        }
        _ => {}
    }
}

fn relevant(name: &str) -> bool {
    let l = name.to_lowercase();
    l.ends_with(".json")
        && l.split('/').any(|seg| seg.starts_with(FOLDER))
        && !l.contains("off-facebook_activity")
        && !l.contains("off_facebook_activity")
        && !l.contains("posts")
}

pub(crate) fn import(archive: &mut Archive) -> Result<ImportResult, ImportError> {
    let names: Vec<String> = archive
        .names()
        .into_iter()
        .filter(|n| relevant(n))
        .collect();
    let mut warnings = Vec::new();
    if names.is_empty() {
        warnings.push(
            "Keine JSON-Dateien im Ordner „apps_and_websites_off_of_facebook“ gefunden. \
             Bitte den Export im Format JSON anfordern."
                .to_string(),
        );
    }
    let mut apps = Vec::new();
    let mut bad_files = 0;
    for n in &names {
        let Some(bytes) = archive.read(n)? else {
            bad_files += 1;
            continue;
        };
        let text = decode_text(&bytes);
        match serde_json::from_str::<Value>(&text) {
            Ok(v) => walk(&v, &mut apps, 0),
            Err(_) => bad_files += 1,
        }
    }
    if bad_files > 0 {
        warnings.push(format!("{bad_files} Dateien konnten nicht gelesen werden"));
    }
    let scanned = apps.len() as u64;
    let mut seen = HashSet::new();
    let mut findings = Vec::new();
    for app in apps {
        let key = app.domain.clone().unwrap_or_else(|| slug(&app.name));
        if key.is_empty() || !seen.insert(key.clone()) {
            continue;
        }
        let domain = app
            .domain
            .unwrap_or_else(|| format!("{UNRESOLVED_PREFIX}{key}"));
        findings.push(Finding {
            domain,
            name: Some(app.name.clone()),
            identifier: None,
            sso: Some(SsoProvider::Facebook),
            evidence: Evidence {
                kind: EvidenceKind::SsoGrant,
                source: SourceKind::FacebookExport,
                summary: "Mit Facebook verbunden (App/Website)".into(),
                date: app.ts.and_then(epoch),
                source_ref: Some(source_ref(&["fb", &key])),
            },
        });
    }
    Ok(ImportResult {
        source: SourceKind::FacebookExport,
        items_scanned: scanned,
        findings,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::zip_of;

    const APPS: &str = r#"{"installed_apps_v2":[
        {"name":"Spotify","added_timestamp":1600000000},
        {"name":"MÃ¼ller Shop","added_timestamp":1610000000,"domain":"https://shop.example.com"},
        {"name":"Spotify","added_timestamp":1600000001}]}"#;

    #[test]
    fn zip_with_mojibake() {
        let z = zip_of(&[
            (
                "facebook-x/apps_and_websites_off_of_facebook/connected_apps_and_websites.json",
                APPS,
            ),
            (
                "facebook-x/apps_and_websites_off_of_facebook/your_off-facebook_activity.json",
                r#"{"v":[{"name":"Tracker","events":[{"timestamp":5}]}]}"#,
            ),
        ]);
        let mut a = Archive::from_zip(std::io::Cursor::new(z)).unwrap();
        assert!(looks_like(&a.names()));
        let r = import(&mut a).unwrap();
        assert_eq!(r.findings.len(), 2);
        let s = &r.findings[0];
        assert_eq!(s.domain, "unresolved:spotify");
        assert_eq!(s.sso, Some(SsoProvider::Facebook));
        assert_eq!(s.evidence.kind, EvidenceKind::SsoGrant);
        assert_eq!(s.evidence.source, SourceKind::FacebookExport);
        assert_eq!(s.evidence.date.unwrap().timestamp(), 1_600_000_000);
        let m = &r.findings[1];
        assert_eq!(m.name.as_deref(), Some("Müller Shop"));
        assert_eq!(m.domain, "shop.example.com");
    }
}
