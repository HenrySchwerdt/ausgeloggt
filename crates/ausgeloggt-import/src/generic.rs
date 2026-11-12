//! Generic lists of domains / URLs: `.txt`, `.csv`, `.json`.

use std::collections::HashSet;

use ausgeloggt_core::{Evidence, EvidenceKind, Finding, SourceKind};
use serde_json::Value;

use crate::passwords::import_list_csv;
use crate::util::{host_of, source_ref};
use crate::{ImportError, ImportResult};

fn finding(host: String, name: Option<String>, identifier: Option<String>) -> Finding {
    let r = source_ref(&["list", &host, identifier.as_deref().unwrap_or("")]);
    Finding {
        domain: host,
        name,
        identifier,
        sso: None,
        evidence: Evidence {
            kind: EvidenceKind::DataExport,
            source: SourceKind::GenericList,
            summary: "In einer Liste aufgeführt".into(),
            date: None,
            source_ref: Some(r),
        },
    }
}

/// One entry: a URL, a domain, or an e-mail address (the identifier is kept then).
fn entry(raw: &str, name: Option<String>) -> Option<Finding> {
    let raw = raw.trim();
    let host = host_of(raw)?;
    let identifier =
        (raw.contains('@') && !raw.contains("://") && !raw.contains('/')).then(|| raw.to_string());
    Some(finding(host, name, identifier))
}

struct Acc {
    seen: HashSet<Option<String>>,
    findings: Vec<Finding>,
    scanned: u64,
    skipped: u64,
}

impl Acc {
    fn new() -> Self {
        Self {
            seen: HashSet::new(),
            findings: Vec::new(),
            scanned: 0,
            skipped: 0,
        }
    }
    fn add(&mut self, raw: &str, name: Option<String>) {
        self.scanned += 1;
        match entry(raw, name) {
            Some(f) => {
                if self.seen.insert(f.evidence.source_ref.clone()) {
                    self.findings.push(f);
                }
            }
            None => self.skipped += 1,
        }
    }
    fn finish(self) -> ImportResult {
        let mut warnings = Vec::new();
        if self.skipped > 0 {
            warnings.push(format!(
                "{} Einträge ohne erkennbare Domain übersprungen",
                self.skipped
            ));
        }
        ImportResult {
            source: SourceKind::GenericList,
            items_scanned: self.scanned,
            findings: self.findings,
            warnings,
        }
    }
}

/// Plain text: one URL / domain / e-mail per line, `#` starts a comment.
pub(crate) fn import_txt(text: &str) -> ImportResult {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut acc = Acc::new();
    for line in text.lines() {
        let line = line.split(" #").next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        acc.add(line, None);
    }
    acc.finish()
}

/// JSON: array of strings, array of objects with `url`/`domain`/..., or an object
/// wrapping such an array.
pub(crate) fn import_json(json: &str) -> Result<ImportResult, ImportError> {
    let json = json.strip_prefix('\u{feff}').unwrap_or(json);
    let v: Value =
        serde_json::from_str(json).map_err(|e| ImportError::Invalid(format!("JSON: {e}")))?;
    let arr = match &v {
        Value::Array(a) => a,
        Value::Object(o) => o
            .values()
            .find_map(Value::as_array)
            .ok_or(ImportError::UnknownFormat)?,
        _ => return Err(ImportError::UnknownFormat),
    };
    let mut acc = Acc::new();
    for item in arr {
        match item {
            Value::String(s) => acc.add(s, None),
            Value::Object(o) => {
                let get = |keys: &[&str]| {
                    keys.iter()
                        .find_map(|k| o.get(*k).and_then(Value::as_str))
                        .map(str::to_string)
                };
                let name = get(&["name", "title"]);
                match get(&["url", "domain", "website", "host", "login_uri", "uri"]) {
                    Some(u) => acc.add(&u, name),
                    None => {
                        acc.scanned += 1;
                        acc.skipped += 1;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(acc.finish())
}

pub(crate) fn import_csv(text: &str) -> Result<ImportResult, ImportError> {
    import_list_csv(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn txt() {
        let r = import_txt("\u{feff}# my list\nhttps://a.example.com/x\n\nb.example.org # note\nme@c.example.net\nnot a domain\na.example.com\n");
        let d: Vec<_> = r.findings.iter().map(|f| f.domain.as_str()).collect();
        assert_eq!(d, ["a.example.com", "b.example.org", "c.example.net"]);
        assert_eq!(
            r.findings[2].identifier.as_deref(),
            Some("me@c.example.net")
        );
        assert!(!r.warnings.is_empty());
        assert_eq!(r.findings[0].evidence.source, SourceKind::GenericList);
    }

    #[test]
    fn json() {
        let r = import_json(r#"["a.example.com", "https://b.example.org"]"#).unwrap();
        assert_eq!(r.findings.len(), 2);
        let r = import_json(r#"[{"name":"Foo","url":"https://foo.example.com"},{"domain":"bar.example.com"},{"x":1}]"#).unwrap();
        assert_eq!(r.findings.len(), 2);
        assert_eq!(r.findings[0].name.as_deref(), Some("Foo"));
        assert_eq!(r.warnings.len(), 1);
        assert!(import_json("3").is_err());
    }

    #[test]
    fn csv() {
        let r = import_csv("name,website\nFoo,foo.example.com\n").unwrap();
        assert_eq!(r.findings[0].domain, "foo.example.com");
        assert_eq!(r.findings[0].name.as_deref(), Some("Foo"));
    }
}
