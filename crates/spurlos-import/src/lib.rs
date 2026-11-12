//! Importers that turn local files into [`Finding`]s.
//!
//! Privacy rule: importers NEVER keep passwords, notes, TOTP secrets or card data.
//! Only domain/URL, entry title and username/e-mail leave this crate.

use std::path::Path;

use spurlos_core::{Finding, PasswordFormat, SourceKind};

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

/// Look at a file (CSV, JSON, ZIP or folder) and guess what it is without importing.
pub fn detect(path: &Path) -> Result<DetectedKind, ImportError> {
    let _ = path;
    todo!("implemented by import agent")
}

/// Import any supported file or folder; auto-detects the format.
pub fn import_path(path: &Path) -> Result<ImportResult, ImportError> {
    let _ = path;
    todo!("implemented by import agent")
}

/// Import a password-manager / browser CSV export from memory (auto-detected format).
pub fn import_password_csv(csv: &str) -> Result<ImportResult, ImportError> {
    let _ = csv;
    todo!("implemented by import agent")
}

/// Human readable help: how to create an export for each supported source (German).
pub fn export_instructions() -> Vec<(&'static str, &'static str)> {
    todo!("implemented by import agent")
}
