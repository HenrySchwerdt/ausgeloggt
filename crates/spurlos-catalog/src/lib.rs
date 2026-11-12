//! Deletion catalog: for a service domain, how do I delete my account?
//!
//! Bundled snapshot (works offline) + optional refresh from upstream open data
//! (e.g. JustDeleteMe), cached in the app's cache dir. Licenses/attribution of
//! bundled data are listed in `data/ATTRIBUTION.md`.

pub mod gdpr;

use std::path::Path;

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
}

pub struct Catalog {
    // private
}

impl Catalog {
    /// Catalog from the snapshot compiled into the binary.
    pub fn bundled() -> Self {
        todo!("implemented by catalog agent")
    }

    /// Bundled snapshot overlaid with a cached refresh from `cache_dir`, if present.
    pub fn load(cache_dir: &Path) -> Self {
        let _ = cache_dir;
        todo!("implemented by catalog agent")
    }

    /// Download the latest upstream data, write it to `cache_dir`, return the new catalog.
    /// Blocking; call from a worker thread.
    pub fn refresh(cache_dir: &Path) -> Result<Self, CatalogError> {
        let _ = cache_dir;
        todo!("implemented by catalog agent")
    }

    /// Lookup by canonical registrable domain (as stored in `Account.domain`).
    pub fn lookup(&self, domain: &str) -> Option<&DeletionInfo> {
        let _ = domain;
        todo!("implemented by catalog agent")
    }

    /// Fuzzy lookup by service name (for `unresolved:` accounts).
    pub fn lookup_name(&self, name: &str) -> Option<&DeletionInfo> {
        let _ = name;
        todo!("implemented by catalog agent")
    }

    pub fn len(&self) -> usize {
        todo!("implemented by catalog agent")
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
