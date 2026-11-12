//! Encrypted vault file.
//!
//! Format (little endian):
//! ```text
//! magic       8   b"SPURLOS\0"
//! version     1   = 1
//! m_cost_kib  4   Argon2id memory (default 65536)
//! t_cost      4   Argon2id iterations (default 3)
//! p_cost      4   Argon2id lanes (default 1)
//! salt        16  random
//! nonce       24  random, fresh on every save (XChaCha20-Poly1305)
//! ciphertext  ..  AEAD(key, nonce, serde_json(VaultData)), AAD = all header bytes above
//! ```
//! Key = Argon2id(password, salt, params) -> 32 bytes. Pure-Rust crypto only
//! (`argon2`, `chacha20poly1305`, `rand`/`getrandom`, `zeroize`), so it builds on every OS.
//! Writes are atomic (temp file in the same dir, fsync, rename) and keep one `<file>.bak`
//! of the previous version. Key material is zeroized on drop.

use std::path::{Path, PathBuf};

use crate::model::VaultData;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("Falsches Passwort oder beschädigte Datei")]
    WrongPasswordOrCorrupt,
    #[error("Das ist keine Spurlos-Tresordatei")]
    NotAVault,
    #[error("Tresor-Version {0} wird nicht unterstützt")]
    UnsupportedVersion(u8),
    #[error("Datei existiert bereits")]
    AlreadyExists,
    #[error("Ein-/Ausgabefehler: {0}")]
    Io(#[from] std::io::Error),
    #[error("Datenfehler: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Kryptofehler: {0}")]
    Crypto(String),
}

/// An unlocked vault: holds the derived key (never the password) and the data.
pub struct Vault {
    pub path: PathBuf,
    pub data: VaultData,
    // private fields: derived key (zeroizing), salt, kdf params
}

impl Vault {
    /// Create a new vault file at `path` protected by `password` and write it once.
    /// Fails with `AlreadyExists` if the file exists. Creates parent dirs.
    pub fn create(path: &Path, password: &str) -> Result<Self, VaultError> {
        let _ = (path, password);
        todo!("implemented by core agent")
    }

    /// Open and decrypt an existing vault. Slow by design (Argon2id): call off the UI thread.
    pub fn open(path: &Path, password: &str) -> Result<Self, VaultError> {
        let _ = (path, password);
        todo!("implemented by core agent")
    }

    /// Encrypt and atomically write `self.data` to `self.path`. Fast (key is cached).
    pub fn save(&self) -> Result<(), VaultError> {
        todo!("implemented by core agent")
    }

    /// Re-encrypt with a new password (new salt) and save.
    pub fn change_password(&mut self, new_password: &str) -> Result<(), VaultError> {
        let _ = new_password;
        todo!("implemented by core agent")
    }

    /// Default vault location: `<platform data dir>/Spurlos/vault.spurlos`
    /// (`directories` crate: Windows %APPDATA%, macOS ~/Library/Application Support, Linux XDG).
    pub fn default_path() -> PathBuf {
        todo!("implemented by core agent")
    }

    /// Export a plain-JSON copy (user-initiated backup / portability).
    pub fn export_json(&self, path: &Path) -> Result<(), VaultError> {
        let _ = path;
        todo!("implemented by core agent")
    }
}
