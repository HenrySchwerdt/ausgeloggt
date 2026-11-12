//! Deletion catalog: for a service domain, how do I delete my account?
//!
//! Bundled snapshot (works offline) + optional refresh from upstream open data
//! (e.g. JustDeleteMe), cached in the app's cache dir. Licenses/attribution of
//! bundled data are listed in `data/ATTRIBUTION.md`.

pub mod gdpr;
pub mod orgs;

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

/// Upstream JustDeleteMe data (MIT).
pub const JDM_SITES_URL: &str =
    "https://raw.githubusercontent.com/jdm-contrib/jdm/master/_data/sites.json";

const BUNDLED_JDM: &str = include_str!("../data/jdm-sites.json");
const BUNDLED_COMPANIES: &str = include_str!("../data/companies.json");
const CACHE_FILE: &str = "jdm-sites.json";
const MAX_DOWNLOAD: u64 = 32 * 1024 * 1024;
const MIN_PLAUSIBLE_ENTRIES: usize = 100;

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("Netzwerkfehler: {0}")]
    Network(String),
    #[error("Ungültige Katalogdaten: {0}")]
    Invalid(String),
    #[error("Ein-/Ausgabefehler: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
    /// Service offers no way to delete the account.
    Impossible,
    Unknown,
}

impl Difficulty {
    pub fn label_de(self) -> &'static str {
        match self {
            Difficulty::Easy => "Einfach",
            Difficulty::Medium => "Mittel",
            Difficulty::Hard => "Schwer",
            Difficulty::Impossible => "Nicht möglich",
            Difficulty::Unknown => "Unbekannt",
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeletionInfo {
    pub name: String,
    /// All domains of the service (registrable, lowercase).
    pub domains: Vec<String>,
    /// Direct link to the deletion page / instructions.
    pub url: Option<String>,
    pub difficulty: Difficulty,
    /// Short instructions (as provided upstream, may be English).
    pub notes: Option<String>,
    /// Address for deletion / GDPR requests if known.
    pub email: Option<String>,
    /// Full legal company name (Datenanfragen.de), for letters.
    #[serde(default)]
    pub legal_name: Option<String>,
    /// Postal address, multi-line (Datenanfragen.de), for printable letters.
    #[serde(default)]
    pub address: Option<String>,
    /// Privacy / data-protection contact if different from `email`.
    #[serde(default)]
    pub privacy_email: Option<String>,
    /// Datenanfragen.de slug.
    #[serde(default)]
    pub slug: Option<String>,
    /// Countries the company is relevant for (ISO codes or "all").
    #[serde(default)]
    pub relevant_countries: Vec<String>,
}

/// One entry of JustDeleteMe's `sites.json` (unknown fields are ignored).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct JdmSite {
    name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    domains: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    difficulty: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    email: Option<String>,
}

/// Trimmed Datenanfragen.de company record.
#[derive(Debug, Clone, serde::Deserialize)]
struct Company {
    n: String,
    d: String,
    #[serde(default)]
    s: Option<String>,
    #[serde(default)]
    e: Option<String>,
    #[serde(default)]
    a: Option<String>,
    #[serde(default)]
    c: Vec<String>,
}

fn parse_jdm(json: &str) -> Result<Vec<JdmSite>, CatalogError> {
    let sites: Vec<JdmSite> =
        serde_json::from_str(json).map_err(|e| CatalogError::Invalid(e.to_string()))?;
    if sites.len() < MIN_PLAUSIBLE_ENTRIES {
        return Err(CatalogError::Invalid(format!(
            "nur {} Einträge",
            sites.len()
        )));
    }
    Ok(sites)
}

fn parse_difficulty(s: Option<&str>) -> Difficulty {
    match s.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("easy") => Difficulty::Easy,
        Some("medium") => Difficulty::Medium,
        // "limited": deletion possible only with restrictions.
        Some("hard") | Some("limited") => Difficulty::Hard,
        Some("impossible") => Difficulty::Impossible,
        _ => Difficulty::Unknown,
    }
}

fn norm_domain(d: &str) -> String {
    let d = d.trim().trim_end_matches('.').to_lowercase();
    d.strip_prefix("www.").unwrap_or(&d).to_string()
}

fn norm_name(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn non_empty(s: Option<&str>) -> Option<String> {
    s.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub struct Catalog {
    entries: Vec<DeletionInfo>,
    by_domain: HashMap<String, Vec<usize>>,
    /// (normalised name, entry index), in entry order.
    names: Vec<(String, usize)>,
    by_name: HashMap<String, usize>,
}

impl Catalog {
    fn build(jdm: Vec<JdmSite>, companies: Vec<Company>) -> Self {
        let mut company_by_domain: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, c) in companies.iter().enumerate() {
            company_by_domain
                .entry(norm_domain(&c.d))
                .or_default()
                .push(i);
        }
        let mut used_company = vec![false; companies.len()];
        let mut entries: Vec<DeletionInfo> = Vec::with_capacity(jdm.len() + companies.len());

        for site in jdm {
            let domains: Vec<String> = site
                .domains
                .iter()
                .map(|d| norm_domain(d))
                .filter(|d| !d.is_empty())
                .collect();
            // Matching company; entries with an address preferred.
            let company = domains
                .iter()
                .filter_map(|d| company_by_domain.get(d))
                .flatten()
                .copied()
                .min_by_key(|&i| (companies[i].a.is_none(), i));
            let mut info = DeletionInfo {
                name: site.name.trim().to_string(),
                domains,
                url: non_empty(site.url.as_deref()),
                difficulty: parse_difficulty(site.difficulty.as_deref()),
                notes: non_empty(site.notes.as_deref()),
                email: non_empty(site.email.as_deref()),
                legal_name: None,
                address: None,
                privacy_email: None,
                slug: None,
                relevant_countries: Vec::new(),
            };
            if let Some(ci) = company {
                used_company[ci] = true;
                let c = &companies[ci];
                let c_email = non_empty(c.e.as_deref());
                match (&info.email, c_email) {
                    (None, e) => info.email = e,
                    (Some(have), Some(e)) if !have.eq_ignore_ascii_case(&e) => {
                        info.privacy_email = Some(e)
                    }
                    _ => {}
                }
                info.legal_name = non_empty(Some(&c.n));
                info.address = non_empty(c.a.as_deref());
                info.slug = c.s.clone();
                info.relevant_countries = c.c.clone();
            }
            entries.push(info);
        }

        // Companies that no JDM entry covers become entries of their own.
        for (i, c) in companies.iter().enumerate() {
            if used_company[i] {
                continue;
            }
            let domain = norm_domain(&c.d);
            if domain.is_empty() {
                continue;
            }
            entries.push(DeletionInfo {
                name: c.n.trim().to_string(),
                domains: vec![domain],
                url: None,
                difficulty: Difficulty::Unknown,
                notes: None,
                email: non_empty(c.e.as_deref()),
                legal_name: non_empty(Some(&c.n)),
                address: non_empty(c.a.as_deref()),
                privacy_email: None,
                slug: c.s.clone(),
                relevant_countries: c.c.clone(),
            });
        }

        let mut by_domain: HashMap<String, Vec<usize>> = HashMap::new();
        let mut names = Vec::new();
        let mut by_name = HashMap::new();
        for (i, e) in entries.iter().enumerate() {
            for d in &e.domains {
                let v = by_domain.entry(d.clone()).or_default();
                if !v.contains(&i) {
                    v.push(i);
                }
            }
            let n = norm_name(&e.name);
            if !n.is_empty() {
                by_name.entry(n.clone()).or_insert(i);
                names.push((n, i));
            }
        }
        Catalog {
            entries,
            by_domain,
            names,
            by_name,
        }
    }

    fn bundled_companies() -> Vec<Company> {
        serde_json::from_str(BUNDLED_COMPANIES).unwrap_or_else(|e| {
            log::error!("gebündelte Firmendaten ungültig: {e}");
            Vec::new()
        })
    }

    fn bundled_jdm() -> Vec<JdmSite> {
        parse_jdm(BUNDLED_JDM).unwrap_or_else(|e| {
            log::error!("gebündelter Katalog ungültig: {e}");
            Vec::new()
        })
    }

    /// Catalog from the snapshot compiled into the binary.
    pub fn bundled() -> Self {
        Self::build(Self::bundled_jdm(), Self::bundled_companies())
    }

    /// Bundled snapshot overlaid with a cached refresh from `cache_dir`, if present.
    pub fn load(cache_dir: &Path) -> Self {
        let cached = std::fs::read_to_string(cache_dir.join(CACHE_FILE))
            .map_err(|e| e.to_string())
            .and_then(|s| parse_jdm(&s).map_err(|e| e.to_string()));
        let jdm = match cached {
            Ok(sites) => sites,
            Err(e) => {
                log::debug!("kein nutzbarer Katalog-Cache: {e}");
                Self::bundled_jdm()
            }
        };
        Self::build(jdm, Self::bundled_companies())
    }

    /// Download the latest upstream data, write it to `cache_dir`, return the new catalog.
    /// Blocking; call from a worker thread.
    pub fn refresh(cache_dir: &Path) -> Result<Self, CatalogError> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .build()
            .into();
        let mut resp = agent
            .get(JDM_SITES_URL)
            .header("User-Agent", concat!("spurlos/", env!("CARGO_PKG_VERSION")))
            .call()
            .map_err(|e| CatalogError::Network(e.to_string()))?;
        let text = resp
            .body_mut()
            .with_config()
            .limit(MAX_DOWNLOAD)
            .read_to_string()
            .map_err(|e| CatalogError::Network(e.to_string()))?;
        let sites = parse_jdm(&text)?;
        // Store only the fields we use.
        let trimmed =
            serde_json::to_vec(&sites).map_err(|e| CatalogError::Invalid(e.to_string()))?;
        std::fs::create_dir_all(cache_dir)?;
        let tmp = cache_dir.join(format!("{CACHE_FILE}.tmp"));
        std::fs::write(&tmp, &trimmed)?;
        if let Err(e) = std::fs::rename(&tmp, cache_dir.join(CACHE_FILE)) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.into());
        }
        Ok(Self::build(sites, Self::bundled_companies()))
    }

    fn candidates(&self, domain: &str) -> Option<&Vec<usize>> {
        let d = norm_domain(domain);
        if let Some(v) = self.by_domain.get(&d) {
            return Some(v);
        }
        // Try parent domains, down to two labels.
        let mut rest = d.as_str();
        while let Some((_, parent)) = rest.split_once('.') {
            if !parent.contains('.') {
                break;
            }
            if let Some(v) = self.by_domain.get(parent) {
                return Some(v);
            }
            rest = parent;
        }
        None
    }

    /// Lookup by canonical registrable domain (as stored in `Account.domain`).
    /// Subdomains fall back to their parent domain.
    pub fn lookup(&self, domain: &str) -> Option<&DeletionInfo> {
        self.candidates(domain)
            .and_then(|v| v.first())
            .map(|&i| &self.entries[i])
    }

    /// Like [`Catalog::lookup`], but prefers an entry that has a postal address.
    pub fn lookup_contact(&self, domain: &str) -> Option<&DeletionInfo> {
        let v = self.candidates(domain)?;
        v.iter()
            .map(|&i| &self.entries[i])
            .find(|e| e.address.is_some())
            .or_else(|| v.first().map(|&i| &self.entries[i]))
    }

    /// Fuzzy lookup by service name (for `unresolved:` accounts).
    pub fn lookup_name(&self, name: &str) -> Option<&DeletionInfo> {
        let q = norm_name(name);
        if q.is_empty() {
            return None;
        }
        if let Some(&i) = self.by_name.get(&q) {
            return Some(&self.entries[i]);
        }
        if q.chars().count() < 4 {
            return None;
        }
        let long = |n: &str| n.chars().count() >= 4;
        // Prefix match in either direction, closest length first.
        let prefix = self
            .names
            .iter()
            .filter(|(n, _)| long(n) && (n.starts_with(&q) || q.starts_with(n.as_str())))
            .min_by_key(|(n, i)| (n.len().abs_diff(q.len()), *i));
        if let Some((_, i)) = prefix {
            return Some(&self.entries[*i]);
        }
        self.names
            .iter()
            .filter(|(n, _)| long(n) && n.contains(&q))
            .min_by_key(|(n, i)| (n.len(), *i))
            .map(|(_, i)| &self.entries[*i])
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_is_large_and_parses() {
        let jdm = parse_jdm(BUNDLED_JDM).unwrap();
        assert!(jdm.len() > 500);
        let c = Catalog::bundled();
        assert!(c.len() > 500);
    }

    #[test]
    fn lookup_domain_and_subdomain() {
        let c = Catalog::bundled();
        let a = c.lookup("spotify.com").expect("spotify");
        assert!(a.name.to_lowercase().contains("spotify"));
        assert_eq!(c.lookup("WWW.Spotify.com").map(|e| &e.name), Some(&a.name));
        assert_eq!(
            c.lookup("accounts.eu.spotify.com").map(|e| &e.name),
            Some(&a.name)
        );
        assert!(c.lookup("definitely-not-a-service.invalid").is_none());
        assert!(c.lookup("").is_none());
    }

    #[test]
    fn lookup_by_name() {
        let c = Catalog::bundled();
        assert!(c.lookup_name("Spotify").is_some());
        assert!(c.lookup_name("  spo-tify ").is_some());
        assert!(c.lookup_name("zz").is_none());
        assert!(c.lookup_name("").is_none());
    }

    fn site(name: &str, domain: &str, diff: &str) -> JdmSite {
        JdmSite {
            name: name.into(),
            domains: vec![domain.into()],
            url: Some("https://example.com/del".into()),
            difficulty: Some(diff.into()),
            notes: None,
            email: None,
        }
    }

    fn company(n: &str, d: &str, e: Option<&str>, a: Option<&str>) -> Company {
        Company {
            n: n.into(),
            d: d.into(),
            s: Some("slug".into()),
            e: e.map(Into::into),
            a: a.map(Into::into),
            c: vec!["all".into()],
        }
    }

    #[test]
    fn merge_fills_email_and_contact() {
        let c = Catalog::build(
            vec![site("Example", "example.com", "limited")],
            vec![company(
                "Example GmbH",
                "www.example.com",
                Some("privacy@example.com"),
                Some("Str. 1\n12345 Berlin"),
            )],
        );
        let e = c.lookup("example.com").unwrap();
        assert_eq!(e.difficulty, Difficulty::Hard);
        assert_eq!(e.email.as_deref(), Some("privacy@example.com"));
        assert_eq!(e.legal_name.as_deref(), Some("Example GmbH"));
        assert!(e.address.as_deref().unwrap().contains("Berlin"));
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn jdm_email_kept_and_privacy_email_separate() {
        let mut s = site("Example", "example.com", "easy");
        s.email = Some("delete@example.com".into());
        let c = Catalog::build(
            vec![s],
            vec![company(
                "Example",
                "example.com",
                Some("dpo@example.com"),
                None,
            )],
        );
        let e = c.lookup("example.com").unwrap();
        assert_eq!(e.email.as_deref(), Some("delete@example.com"));
        assert_eq!(e.privacy_email.as_deref(), Some("dpo@example.com"));
    }

    #[test]
    fn lookup_contact_prefers_address() {
        let c = Catalog::build(
            vec![],
            vec![
                company("A", "multi.com", Some("a@multi.com"), None),
                company("B", "multi.com", Some("b@multi.com"), Some("Somewhere 1")),
            ],
        );
        assert_eq!(c.lookup("multi.com").unwrap().name, "A");
        assert_eq!(c.lookup_contact("sub.multi.com").unwrap().name, "B");
    }

    #[test]
    fn name_fuzzy_rules() {
        let sites: Vec<JdmSite> = (0..120)
            .map(|i| site(&format!("Filler{i}"), &format!("filler{i}.com"), "easy"))
            .chain([site("Face Book", "fb.example", "hard")])
            .collect();
        let c = Catalog::build(sites, vec![]);
        assert_eq!(c.lookup_name("facebook").unwrap().name, "Face Book");
        assert_eq!(c.lookup_name("Facebook Inc").unwrap().name, "Face Book");
        assert_eq!(c.lookup_name("ebook").unwrap().name, "Face Book");
        assert!(c.lookup_name("fac").is_none());
    }

    #[test]
    fn cache_overlay_and_invalid_cache() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(CACHE_FILE), "not json").unwrap();
        assert_eq!(Catalog::load(dir.path()).len(), Catalog::bundled().len());
        let sites: Vec<JdmSite> = (0..150)
            .map(|i| site(&format!("Only{i}"), &format!("only{i}.test"), "easy"))
            .collect();
        std::fs::write(
            dir.path().join(CACHE_FILE),
            serde_json::to_vec(&sites).unwrap(),
        )
        .unwrap();
        let c = Catalog::load(dir.path());
        assert!(c.lookup("only7.test").is_some());
    }

    #[test]
    fn difficulty_mapping() {
        assert_eq!(parse_difficulty(Some("Impossible")), Difficulty::Impossible);
        assert_eq!(parse_difficulty(None), Difficulty::Unknown);
        assert_eq!(parse_difficulty(Some("weird")), Difficulty::Unknown);
    }
}
