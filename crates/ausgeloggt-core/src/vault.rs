//! Encrypted vault file.
//!
//! Format (little endian):
//! ```text
//! magic       8   b"AUSGLOGT"
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

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

use crate::model::{VaultData, SCHEMA_VERSION};

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("Falsches Passwort oder beschädigte Datei")]
    WrongPasswordOrCorrupt,
    #[error("Das ist keine ausgeloggt-Tresordatei")]
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

const MAGIC: &[u8; 8] = b"AUSGLOGT";
const FORMAT_VERSION: u8 = 1;
const HEADER_LEN: usize = 8 + 1 + 4 + 4 + 4 + 16 + 24;
const TAG_LEN: usize = 16;

/// Argon2id cost parameters (stored in the file header).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KdfParams {
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl KdfParams {
    pub(crate) const DEFAULT: KdfParams = KdfParams {
        m_cost_kib: 65536,
        t_cost: 3,
        p_cost: 1,
    };

    /// Reject absurd values from a (possibly hostile) header before allocating memory.
    fn sane(&self) -> bool {
        (8..=4 * 1024 * 1024).contains(&self.m_cost_kib)
            && (1..=64).contains(&self.t_cost)
            && (1..=64).contains(&self.p_cost)
    }
}

/// An unlocked vault: holds the derived key (never the password) and the data.
pub struct Vault {
    pub path: PathBuf,
    pub data: VaultData,
    key: Zeroizing<[u8; 32]>,
    salt: [u8; 16],
    params: KdfParams,
}

fn random<const N: usize>() -> Result<[u8; N], VaultError> {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).map_err(|e| VaultError::Crypto(format!("Zufallszahlen: {e}")))?;
    Ok(buf)
}

fn derive_key(
    password: &str,
    salt: &[u8; 16],
    p: KdfParams,
) -> Result<Zeroizing<[u8; 32]>, VaultError> {
    let params = Params::new(p.m_cost_kib, p.t_cost, p.p_cost, Some(32))
        .map_err(|e| VaultError::Crypto(e.to_string()))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|e| VaultError::Crypto(e.to_string()))?;
    Ok(key)
}

fn cipher(key: &[u8; 32]) -> Result<XChaCha20Poly1305, VaultError> {
    XChaCha20Poly1305::new_from_slice(key).map_err(|e| VaultError::Crypto(e.to_string()))
}

impl Vault {
    /// Create a new vault file at `path` protected by `password` and write it once.
    /// Fails with `AlreadyExists` if the file exists. Creates parent dirs.
    pub fn create(path: &Path, password: &str) -> Result<Self, VaultError> {
        Self::create_with_params(path, password, KdfParams::DEFAULT)
    }

    /// Cheap-KDF variant for tests of dependent crates (never use for real vaults).
    #[doc(hidden)]
    pub fn create_for_tests(path: &Path, password: &str) -> Result<Self, VaultError> {
        Self::create_with_params(
            path,
            password,
            KdfParams {
                m_cost_kib: 64,
                t_cost: 1,
                p_cost: 1,
            },
        )
    }

    pub(crate) fn create_with_params(
        path: &Path,
        password: &str,
        params: KdfParams,
    ) -> Result<Self, VaultError> {
        if path.exists() {
            return Err(VaultError::AlreadyExists);
        }
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                fs::create_dir_all(dir)?;
            }
        }
        let salt = random::<16>()?;
        let key = derive_key(password, &salt, params)?;
        let vault = Vault {
            path: path.to_path_buf(),
            data: VaultData {
                schema: SCHEMA_VERSION,
                ..VaultData::default()
            },
            key,
            salt,
            params,
        };
        vault.save()?;
        Ok(vault)
    }

    /// Open and decrypt an existing vault. Slow by design (Argon2id): call off the UI thread.
    pub fn open(path: &Path, password: &str) -> Result<Self, VaultError> {
        let bytes = fs::read(path)?;
        if bytes.len() < MAGIC.len() + 1 || &bytes[..MAGIC.len()] != MAGIC {
            return Err(VaultError::NotAVault);
        }
        let version = bytes[8];
        if version != FORMAT_VERSION {
            return Err(VaultError::UnsupportedVersion(version));
        }
        if bytes.len() < HEADER_LEN + TAG_LEN {
            return Err(VaultError::WrongPasswordOrCorrupt);
        }
        let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().expect("4 bytes"));
        let params = KdfParams {
            m_cost_kib: u32_at(9),
            t_cost: u32_at(13),
            p_cost: u32_at(17),
        };
        if !params.sane() {
            return Err(VaultError::WrongPasswordOrCorrupt);
        }
        let salt: [u8; 16] = bytes[21..37].try_into().expect("16 bytes");
        let nonce: [u8; 24] = bytes[37..61].try_into().expect("24 bytes");
        let key = derive_key(password, &salt, params)?;
        let plain = cipher(&key)?
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &bytes[HEADER_LEN..],
                    aad: &bytes[..HEADER_LEN],
                },
            )
            .map_err(|_| VaultError::WrongPasswordOrCorrupt)?;
        let plain = Zeroizing::new(plain);
        let data: VaultData = serde_json::from_slice(&plain)?;
        Ok(Vault {
            path: path.to_path_buf(),
            data,
            key,
            salt,
            params,
        })
    }

    /// Encrypt and atomically write `self.data` to `self.path`. Fast (key is cached).
    pub fn save(&self) -> Result<(), VaultError> {
        let nonce = random::<24>()?;
        let mut header = Vec::with_capacity(HEADER_LEN);
        header.extend_from_slice(MAGIC);
        header.push(FORMAT_VERSION);
        header.extend_from_slice(&self.params.m_cost_kib.to_le_bytes());
        header.extend_from_slice(&self.params.t_cost.to_le_bytes());
        header.extend_from_slice(&self.params.p_cost.to_le_bytes());
        header.extend_from_slice(&self.salt);
        header.extend_from_slice(&nonce);
        debug_assert_eq!(header.len(), HEADER_LEN);

        let plain = Zeroizing::new(serde_json::to_vec(&self.data)?);
        let ct = cipher(&self.key)?
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &plain,
                    aad: &header,
                },
            )
            .map_err(|_| VaultError::Crypto("Verschlüsselung fehlgeschlagen".into()))?;
        header.extend_from_slice(&ct);

        let tmp = sibling(&self.path, "tmp");
        let result = (|| -> Result<(), VaultError> {
            write_synced(&tmp, &header)?;
            if self.path.exists() {
                // Keep the previous version; a failed backup must not block saving.
                if let Err(e) = fs::copy(&self.path, sibling(&self.path, "bak")) {
                    log::warn!("Sicherungskopie des Tresors fehlgeschlagen: {e}");
                }
            }
            fs::rename(&tmp, &self.path)?;
            sync_parent_dir(&self.path);
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    }

    /// Re-encrypt with a new password (new salt) and save.
    pub fn change_password(&mut self, new_password: &str) -> Result<(), VaultError> {
        let salt = random::<16>()?;
        let key = derive_key(new_password, &salt, self.params)?;
        let old_salt = std::mem::replace(&mut self.salt, salt);
        let old_key = std::mem::replace(&mut self.key, key);
        if let Err(e) = self.save() {
            self.salt = old_salt;
            self.key = old_key;
            return Err(e);
        }
        Ok(())
    }

    /// Default vault location: `<platform data dir>/ausgeloggt/vault.ausgeloggt`
    /// (`directories` crate: Windows %APPDATA%, macOS ~/Library/Application Support, Linux XDG).
    pub fn default_path() -> PathBuf {
        let base = directories::BaseDirs::new()
            .map(|d| d.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("ausgeloggt").join("vault.ausgeloggt")
    }

    /// Encrypt arbitrary bytes with the vault key, e.g. the logo cache, which would
    /// otherwise reveal which services the user has accounts with.
    /// Output: `nonce (24) || ciphertext`. `label` is bound as AAD, so a blob sealed
    /// for one purpose cannot be opened as another. Blobs become unreadable after
    /// [`Vault::change_password`]; callers must treat them as a disposable cache.
    pub fn seal_bytes(&self, label: &str, plain: &[u8]) -> Result<Vec<u8>, VaultError> {
        let nonce = random::<24>()?;
        let aad = blob_aad(label);
        let ct = cipher(&self.key)?
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: plain,
                    aad: &aad,
                },
            )
            .map_err(|_| VaultError::Crypto("Verschlüsselung fehlgeschlagen".into()))?;
        let mut out = Vec::with_capacity(24 + ct.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        Ok(out)
    }

    /// Decrypt a blob produced by [`Vault::seal_bytes`] with the same `label`.
    pub fn open_bytes(&self, label: &str, sealed: &[u8]) -> Result<Vec<u8>, VaultError> {
        if sealed.len() < 24 + TAG_LEN {
            return Err(VaultError::WrongPasswordOrCorrupt);
        }
        let nonce: [u8; 24] = sealed[..24].try_into().expect("24 bytes");
        let aad = blob_aad(label);
        cipher(&self.key)?
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &sealed[24..],
                    aad: &aad,
                },
            )
            .map_err(|_| VaultError::WrongPasswordOrCorrupt)
    }

    /// Directory for encrypted caches next to the vault file: `<vault dir>/cache`.
    pub fn cache_dir(&self) -> PathBuf {
        self.path
            .parent()
            .map(|p| p.join("cache"))
            .unwrap_or_else(|| PathBuf::from("cache"))
    }

    /// Export a plain-JSON copy (user-initiated backup / portability).
    pub fn export_json(&self, path: &Path) -> Result<(), VaultError> {
        let json = Zeroizing::new(serde_json::to_vec_pretty(&self.data)?);
        write_synced(path, &json)?;
        Ok(())
    }
}

fn blob_aad(label: &str) -> Vec<u8> {
    let mut aad = b"AUSGELOGGT-BLOB\0".to_vec();
    aad.extend_from_slice(label.as_bytes());
    aad
}

/// `<path>.<ext>` appended to the full file name (`vault.ausgeloggt` -> `vault.ausgeloggt.tmp`).
fn sibling(path: &Path, ext: &str) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".");
    s.push(ext);
    PathBuf::from(s)
}

fn write_synced(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}

/// Flush the directory entry after the rename (no-op where unsupported).
fn sync_parent_dir(path: &Path) {
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        let dir = if dir.as_os_str().is_empty() {
            Path::new(".")
        } else {
            dir
        };
        if let Ok(d) = fs::File::open(dir) {
            let _ = d.sync_all();
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Account;

    const FAST: KdfParams = KdfParams {
        m_cost_kib: 64,
        t_cost: 1,
        p_cost: 1,
    };

    fn account(domain: &str) -> Account {
        let mut a = Account::new(domain, "Test äöü");
        a.identifiers = vec!["a@b.de".into()];
        a.confidence = 0.5;
        a
    }

    fn new_vault(dir: &tempfile::TempDir) -> (PathBuf, Vault) {
        let p = dir.path().join("sub").join("vault.ausgeloggt");
        let v = Vault::create_with_params(&p, "geheim", FAST).unwrap();
        (p, v)
    }

    #[test]
    fn roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let (p, mut v) = new_vault(&dir);
        assert_eq!(v.data.schema, SCHEMA_VERSION);
        v.data.accounts.push(account("spotify.com"));
        v.save().unwrap();
        let o = Vault::open(&p, "geheim").unwrap();
        assert_eq!(o.data.accounts.len(), 1);
        assert_eq!(o.data.accounts[0].domain, "spotify.com");
        assert_eq!(o.data.accounts[0].name, "Test äöü");
        assert_eq!(o.params, FAST);
        // the file does not contain plaintext
        let raw = fs::read(&p).unwrap();
        assert!(!raw.windows(7).any(|w| w == b"spotify"));
    }

    #[test]
    fn wrong_password() {
        let dir = tempfile::tempdir().unwrap();
        let (p, _v) = new_vault(&dir);
        assert!(matches!(
            Vault::open(&p, "falsch"),
            Err(VaultError::WrongPasswordOrCorrupt)
        ));
    }

    #[test]
    fn tampered_ciphertext_and_header() {
        let dir = tempfile::tempdir().unwrap();
        let (p, _v) = new_vault(&dir);
        let orig = fs::read(&p).unwrap();
        // ciphertext, tag (last byte), salt, nonce, t_cost (AAD only)
        for idx in [HEADER_LEN + 2, orig.len() - 1, 25, 40, 13] {
            let mut b = orig.clone();
            b[idx] ^= 0x01;
            fs::write(&p, &b).unwrap();
            assert!(
                matches!(
                    Vault::open(&p, "geheim"),
                    Err(VaultError::WrongPasswordOrCorrupt)
                ),
                "byte {idx}"
            );
        }
        fs::write(&p, &orig[..HEADER_LEN + 4]).unwrap();
        assert!(Vault::open(&p, "geheim").is_err());
        fs::write(&p, &orig).unwrap();
        assert!(Vault::open(&p, "geheim").is_ok());
    }

    #[test]
    fn not_a_vault_and_version() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x");
        fs::write(&p, b"hello world, definitely not a vault file at all").unwrap();
        assert!(matches!(Vault::open(&p, "x"), Err(VaultError::NotAVault)));
        fs::write(&p, b"").unwrap();
        assert!(matches!(Vault::open(&p, "x"), Err(VaultError::NotAVault)));
        let mut b = MAGIC.to_vec();
        b.push(9);
        b.extend_from_slice(&[0u8; 80]);
        fs::write(&p, &b).unwrap();
        assert!(matches!(
            Vault::open(&p, "x"),
            Err(VaultError::UnsupportedVersion(9))
        ));
        assert!(matches!(
            Vault::open(&dir.path().join("missing"), "x"),
            Err(VaultError::Io(_))
        ));
    }

    #[test]
    fn insane_params_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let (p, _v) = new_vault(&dir);
        let mut b = fs::read(&p).unwrap();
        b[9..13].copy_from_slice(&u32::MAX.to_le_bytes());
        fs::write(&p, &b).unwrap();
        assert!(matches!(
            Vault::open(&p, "geheim"),
            Err(VaultError::WrongPasswordOrCorrupt)
        ));
    }

    #[test]
    fn create_existing_fails() {
        let dir = tempfile::tempdir().unwrap();
        let (p, _v) = new_vault(&dir);
        assert!(matches!(
            Vault::create_with_params(&p, "other", FAST),
            Err(VaultError::AlreadyExists)
        ));
        assert!(Vault::open(&p, "geheim").is_ok());
    }

    #[test]
    fn change_password_works() {
        let dir = tempfile::tempdir().unwrap();
        let (p, mut v) = new_vault(&dir);
        v.data.accounts.push(account("netflix.com"));
        let old_salt = v.salt;
        v.change_password("neu").unwrap();
        assert_ne!(old_salt, v.salt);
        assert!(Vault::open(&p, "geheim").is_err());
        let o = Vault::open(&p, "neu").unwrap();
        assert_eq!(o.data.accounts[0].domain, "netflix.com");
    }

    #[test]
    fn save_rotates_nonce_keeps_backup_leaves_no_tmp() {
        let dir = tempfile::tempdir().unwrap();
        let (p, mut v) = new_vault(&dir);
        assert!(!sibling(&p, "bak").exists(), "no backup on first write");
        let first = fs::read(&p).unwrap();
        v.data.accounts.push(account("a.com"));
        v.save().unwrap();
        let second = fs::read(&p).unwrap();
        assert_ne!(
            first[37..61],
            second[37..61],
            "nonce must change on every save"
        );
        assert_eq!(fs::read(sibling(&p, "bak")).unwrap(), first);
        assert!(!sibling(&p, "tmp").exists());
        let b = Vault::open(&sibling(&p, "bak"), "geheim").unwrap();
        assert!(b.data.accounts.is_empty());
        // overwriting an existing file works on every platform
        v.save().unwrap();
        assert!(Vault::open(&p, "geheim").is_ok());
    }

    #[test]
    fn sealed_blobs_roundtrip_and_bind_label() {
        let dir = tempfile::tempdir().unwrap();
        let (_, v) = new_vault(&dir);
        let sealed = v.seal_bytes("logos", b"png bytes").unwrap();
        assert_ne!(&sealed[24..], b"png bytes");
        assert_eq!(v.open_bytes("logos", &sealed).unwrap(), b"png bytes");
        assert!(v.open_bytes("other", &sealed).is_err());
        let mut bad = sealed.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(v.open_bytes("logos", &bad).is_err());
        assert!(v.open_bytes("logos", &sealed[..10]).is_err());
        assert!(v.cache_dir().ends_with("cache"));
    }

    #[test]
    fn export_json_is_plain() {
        let dir = tempfile::tempdir().unwrap();
        let (_p, mut v) = new_vault(&dir);
        v.data.accounts.push(account("spotify.com"));
        let out = dir.path().join("export.json");
        v.export_json(&out).unwrap();
        let d: VaultData = serde_json::from_slice(&fs::read(&out).unwrap()).unwrap();
        assert_eq!(d.accounts[0].domain, "spotify.com");
    }

    #[test]
    fn default_path_shape() {
        let p = Vault::default_path();
        assert!(p.ends_with("ausgeloggt/vault.ausgeloggt"));
    }
}
