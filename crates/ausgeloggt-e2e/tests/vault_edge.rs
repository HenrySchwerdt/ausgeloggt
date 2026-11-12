//! Vault: wrong passwords, corruption, unicode, concurrency, very large vaults.
//!
//! Each Argon2id derivation costs ~0.2-0.6 s (64 MiB, t=3), so tests keep the number of
//! `create` / `open` calls small.

use std::path::{Path, PathBuf};
use std::time::Instant;

use ausgeloggt_core::vault::{Vault, VaultError};
use ausgeloggt_core::{
    Account, CategorySource, Evidence, EvidenceKind, MailCategory, SourceKind, Subscription,
    SubscriptionStatus, VaultData,
};
use chrono::Utc;
use uuid::Uuid;

fn vault_path(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("vault.ausgeloggt")
}

fn sample_account(i: usize) -> Account {
    let mut a = Account::new(format!("service{i}.example.com"), format!("Service {i}"));
    a.identifiers.push(format!("user{i}@example.net"));
    a.notes = format!("Notiz äöüß 日本語 😀 {i}");
    a.confidence = 0.9;
    for k in 0..4 {
        a.evidence.push(Evidence {
            kind: EvidenceKind::Welcome,
            source: SourceKind::Mail {
                mailbox: "me@example.org".into(),
            },
            summary: format!("Willkommen bei Service {i} #{k}"),
            date: Some(Utc::now()),
            source_ref: Some(format!("<{i}.{k}@service{i}.example.com>")),
        });
    }
    a
}

fn sample_sub(i: usize) -> Subscription {
    Subscription {
        id: Uuid::new_v4(),
        mailbox: "me@example.org".into(),
        list_key: format!("news@list{i}.example.com"),
        domain: format!("list{i}.example.com"),
        name: format!("List {i}"),
        sender: format!("news@list{i}.example.com"),
        unsubscribe: vec![],
        message_count: 40,
        frequency_per_month: 4.0,
        first_seen: Some(Utc::now()),
        last_seen: Some(Utc::now()),
        sample_subjects: vec!["a".into(), "b".into()],
        category: MailCategory::Marketing,
        category_source: CategorySource::Rules,
        status: SubscriptionStatus::Active,
        history: vec![],
        post_unsubscribe_mails: vec![],
    }
}

#[test]
fn roundtrip_wrong_password_and_backup_rotation() {
    let dir = tempfile::tempdir().unwrap();
    let path = vault_path(&dir);
    let mut v = Vault::create(&path, "correct horse").unwrap();
    assert!(matches!(
        Vault::create(&path, "other"),
        Err(VaultError::AlreadyExists)
    ));
    v.data.accounts.push(sample_account(1));
    v.save().unwrap();
    let first_save_len = std::fs::metadata(&path).unwrap().len();
    v.data.accounts.push(sample_account(2));
    v.save().unwrap();
    v.data.accounts.push(sample_account(3));
    v.save().unwrap();

    assert!(matches!(
        Vault::open(&path, "wrong"),
        Err(VaultError::WrongPasswordOrCorrupt)
    ));
    assert!(matches!(
        Vault::open(&path, ""),
        Err(VaultError::WrongPasswordOrCorrupt)
    ));
    assert!(matches!(
        Vault::open(&path, "correct horse "),
        Err(VaultError::WrongPasswordOrCorrupt)
    ));
    let reopened = Vault::open(&path, "correct horse").unwrap();
    assert_eq!(reopened.data.accounts.len(), 3);
    assert_eq!(reopened.data.accounts[0].notes, sample_account(1).notes);

    // Exactly one backup, holding the previous version (2 accounts); no stray temp file.
    let bak = PathBuf::from(format!("{}.bak", path.display()));
    assert!(bak.exists());
    let old = Vault::open(&bak, "correct horse").unwrap();
    assert_eq!(old.data.accounts.len(), 2);
    assert!(!PathBuf::from(format!("{}.tmp", path.display())).exists());
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(first_save_len > 0);
}

#[test]
fn nonce_is_fresh_on_every_save_and_ciphertext_hides_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = vault_path(&dir);
    let mut v = Vault::create(&path, "pw").unwrap();
    v.data.accounts.push(sample_account(7));
    v.save().unwrap();
    let a = std::fs::read(&path).unwrap();
    v.save().unwrap();
    let b = std::fs::read(&path).unwrap();
    assert_ne!(a[37..61], b[37..61], "nonce reuse!");
    assert_eq!(a[21..37], b[21..37], "salt stays the same within a session");
    let hay = String::from_utf8_lossy(&a).to_string();
    assert!(!hay.contains("service7") && !hay.contains("Willkommen"));
}

#[test]
fn truncation_bitflips_and_version_mismatch_are_clean_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = vault_path(&dir);
    let mut v = Vault::create(&path, "pw").unwrap();
    v.data.accounts.push(sample_account(1));
    v.save().unwrap();
    let good = std::fs::read(&path).unwrap();
    let probe = dir.path().join("probe.ausgeloggt");

    // Every truncation point in the header region and a sample beyond: no panic, an error.
    // (Truncated files fail before the KDF runs, so this is fast.)
    let mut cuts: Vec<usize> = (0..70).collect();
    cuts.extend([100, 200, good.len() - 17, good.len() - 16, good.len() - 1]);
    for cut in cuts {
        std::fs::write(&probe, &good[..cut]).unwrap();
        let r = Vault::open(&probe, "pw");
        assert!(r.is_err(), "cut {cut} opened");
        match r {
            Err(
                VaultError::NotAVault
                | VaultError::WrongPasswordOrCorrupt
                | VaultError::UnsupportedVersion(_),
            ) => {}
            other => panic!("cut {cut}: unexpected {:?}", other.err()),
        }
    }

    // Magic / version bytes.
    for (idx, val, want_version) in [
        (0usize, b'X', None),
        (8, 2u8, Some(2u8)),
        (8, 0u8, Some(0u8)),
        (8, 255u8, Some(255u8)),
    ] {
        let mut b = good.clone();
        b[idx] = val;
        std::fs::write(&probe, &b).unwrap();
        match (Vault::open(&probe, "pw"), want_version) {
            (Err(VaultError::NotAVault), None) => {}
            (Err(VaultError::UnsupportedVersion(v)), Some(w)) => assert_eq!(v, w),
            (other, _) => panic!("idx {idx}: {:?}", other.err()),
        }
    }

    // Bit flips in salt, nonce, ciphertext and tag (the KDF cost fields are covered below).
    // Salt flips need a KDF run each; keep a sample.
    let mut positions: Vec<usize> = vec![
        21,
        36,
        37,
        60,
        61,
        62,
        good.len() / 2,
        good.len() - 17,
        good.len() - 16,
        good.len() - 1,
    ];
    positions.dedup();
    for pos in positions {
        for bit in [0u8, 7] {
            let mut b = good.clone();
            b[pos] ^= 1 << bit;
            std::fs::write(&probe, &b).unwrap();
            assert!(
                matches!(
                    Vault::open(&probe, "pw"),
                    Err(VaultError::WrongPasswordOrCorrupt)
                ),
                "flip at {pos}:{bit} was accepted"
            );
        }
    }

    // Garbage / non-vault files.
    for bytes in [
        vec![],
        b"AUSGLOGT".to_vec(),
        b"AUSGLOGT\x01".to_vec(),
        vec![0u8; 1000],
        b"just some text".to_vec(),
    ] {
        std::fs::write(&probe, &bytes).unwrap();
        assert!(Vault::open(&probe, "pw").is_err());
    }
    assert!(matches!(
        Vault::open(&dir.path().join("missing.ausgeloggt"), "pw"),
        Err(VaultError::Io(_))
    ));
}

#[test]
fn kdf_parameters_in_a_hostile_header_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = vault_path(&dir);
    let v = Vault::create(&path, "pw").unwrap();
    v.save().unwrap();
    let good = std::fs::read(&path).unwrap();
    let probe = dir.path().join("probe.ausgeloggt");
    // m_cost = 0, t_cost = 0, p_cost = 0, u32::MAX values: rejected before any allocation.
    for (off, val) in [
        (9usize, 0u32),
        (9, u32::MAX),
        (13, 0),
        (13, u32::MAX),
        (17, 0),
        (17, u32::MAX),
        (9, 4 * 1024 * 1024 + 1),
    ] {
        let mut b = good.clone();
        b[off..off + 4].copy_from_slice(&val.to_le_bytes());
        std::fs::write(&probe, &b).unwrap();
        let t = Instant::now();
        assert!(matches!(
            Vault::open(&probe, "pw"),
            Err(VaultError::WrongPasswordOrCorrupt)
        ));
        assert!(t.elapsed().as_secs() < 2, "slow rejection at offset {off}");
    }
}

#[test]
fn single_bit_flip_in_kdf_cost_does_not_trigger_huge_allocation() {
    let dir = tempfile::tempdir().unwrap();
    let path = vault_path(&dir);
    let v = Vault::create(&path, "pw").unwrap();
    v.save().unwrap();
    let mut b = std::fs::read(&path).unwrap();
    // m_cost is 65536 KiB = 0x0001_0000 at offset 9..13 (LE): flip bit 21 -> 2 GiB.
    b[9 + 2] ^= 1 << 5;
    let probe = dir.path().join("probe.ausgeloggt");
    std::fs::write(&probe, &b).unwrap();
    let t = Instant::now();
    let r = Vault::open(&probe, "pw");
    assert!(r.is_err());
    assert!(
        t.elapsed().as_secs_f64() < 3.0,
        "took {:?}: sane() must cap m_cost near what create() writes",
        t.elapsed()
    );
}

#[test]
fn empty_and_unicode_passwords() {
    let dir = tempfile::tempdir().unwrap();
    // Empty password: works (the UI must enforce a minimum), and is distinct from others.
    let p1 = dir.path().join("empty.ausgeloggt");
    Vault::create(&p1, "").unwrap();
    assert!(Vault::open(&p1, "").is_ok());
    assert!(Vault::open(&p1, " ").is_err());

    // Unicode, emoji, very long, NUL inside.
    let long = "x".repeat(10_000);
    for (i, pw) in ["Pässwörd日本語😀", "🔑🔑🔑", long.as_str(), "a\0b"]
        .iter()
        .enumerate()
    {
        let p = dir.path().join(format!("u{i}.ausgeloggt"));
        Vault::create(&p, pw).unwrap();
        assert!(Vault::open(&p, pw).is_ok(), "case {i}");
        assert!(Vault::open(&p, "a").is_err());
    }
}

#[test]
fn unicode_normalisation_forms_open_the_same_vault() {
    let dir = tempfile::tempdir().unwrap();
    let p = vault_path(&dir);
    let nfc = "Caf\u{e9}-Pa\u{df}wort-\u{fc}ber"; // é, ü precomposed
    let nfd = "Cafe\u{301}-Pa\u{df}wort-u\u{308}ber"; // decomposed
    assert_ne!(nfc, nfd);
    Vault::create(&p, nfc).unwrap();
    assert!(Vault::open(&p, nfd).is_ok());
}

#[test]
fn change_password_roundtrip_and_blob_invalidation() {
    let dir = tempfile::tempdir().unwrap();
    let p = vault_path(&dir);
    let mut v = Vault::create(&p, "old-pw").unwrap();
    v.data.accounts.push(sample_account(1));
    v.save().unwrap();
    let blob = v.seal_bytes("logos", b"png-bytes").unwrap();
    assert_eq!(v.open_bytes("logos", &blob).unwrap(), b"png-bytes");
    assert!(v.open_bytes("other-label", &blob).is_err());
    assert!(v.open_bytes("logos", &blob[..10]).is_err());
    let mut tampered = blob.clone();
    *tampered.last_mut().unwrap() ^= 1;
    assert!(v.open_bytes("logos", &tampered).is_err());

    v.change_password("new-pw").unwrap();
    assert!(Vault::open(&p, "old-pw").is_err());
    let v2 = Vault::open(&p, "new-pw").unwrap();
    assert_eq!(v2.data.accounts.len(), 1);
    assert!(
        v2.open_bytes("logos", &blob).is_err(),
        "cache blobs die with the old key"
    );
}

#[test]
fn change_password_does_not_leave_an_old_password_backup() {
    let dir = tempfile::tempdir().unwrap();
    let p = vault_path(&dir);
    let mut v = Vault::create(&p, "old-pw").unwrap();
    v.data.accounts.push(sample_account(1));
    v.save().unwrap();
    v.change_password("new-pw").unwrap();
    let bak = PathBuf::from(format!("{}.bak", p.display()));
    assert!(
        !bak.exists() || Vault::open(&bak, "old-pw").is_err(),
        "backup is still readable with the old password"
    );
}

#[test]
fn paths_with_spaces_umlauts_unicode_and_missing_parents() {
    let dir = tempfile::tempdir().unwrap();
    let nested: PathBuf = dir
        .path()
        .join("Mein Tresor ä ö ü ß")
        .join("日本語 Ordner 😀")
        .join("sub dir")
        .join("Tresor Datei (1).ausgeloggt");
    let v = Vault::create(&nested, "pw").unwrap();
    v.save().unwrap();
    assert!(Vault::open(&nested, "pw").is_ok());
    assert!(Path::new(&format!("{}.bak", nested.display())).exists());
    // relative file name without parent
    let cwd_free = Vault::default_path();
    assert!(cwd_free.to_string_lossy().contains("ausgeloggt"));
    assert_eq!(v.cache_dir().file_name().unwrap(), "cache");
}

#[test]
fn saving_after_the_directory_vanished_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("gone");
    let p = sub.join("v.ausgeloggt");
    let mut v = Vault::create(&p, "pw").unwrap();
    v.data.accounts.push(sample_account(1));
    std::fs::remove_dir_all(&sub).unwrap();
    let r = v.save();
    assert!(matches!(r, Err(VaultError::Io(_))), "{r:?}");
    assert_eq!(
        v.data.accounts.len(),
        1,
        "in-memory data survives a failed save"
    );
}

#[test]
fn concurrent_saves_from_two_handles_never_corrupt_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let p = vault_path(&dir);
    {
        let mut v = Vault::create(&p, "pw").unwrap();
        v.data.accounts.push(sample_account(0));
        v.save().unwrap();
    }
    let handles: Vec<_> = (0..2)
        .map(|t| {
            let p = p.clone();
            std::thread::spawn(move || {
                let mut v = Vault::open(&p, "pw").unwrap();
                let mut ok = 0;
                let mut err = 0;
                for i in 0..40 {
                    v.data.accounts.push(sample_account(1000 * (t + 1) + i));
                    match v.save() {
                        Ok(()) => ok += 1,
                        Err(VaultError::Io(_)) => err += 1,
                        Err(e) => panic!("unexpected {e:?}"),
                    }
                }
                (ok, err)
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    println!("CONCURRENT SAVES (2 writers x 40): ok/io-error = {results:?}");
    // Whatever interleaving happened, the main file must decrypt.
    let v = Vault::open(&p, "pw");
    assert!(
        v.is_ok(),
        "vault corrupted by concurrent writers: {:?}",
        v.err()
    );
    // Last-writer-wins is acceptable, losing the whole file is not.
    assert!(!v.unwrap().data.accounts.is_empty());
}

#[test]
fn large_vault_save_and_open_time() {
    let dir = tempfile::tempdir().unwrap();
    let p = vault_path(&dir);
    let mut v = Vault::create(&p, "pw").unwrap();
    let t = Instant::now();
    v.data.accounts = (0..5_000).map(sample_account).collect();
    v.data.subscriptions = (0..2_000).map(sample_sub).collect();
    let evidence: usize = v.data.accounts.iter().map(|a| a.evidence.len()).sum();
    assert_eq!(evidence, 20_000);
    println!("LARGE VAULT build: {:?}", t.elapsed());

    let t = Instant::now();
    v.save().unwrap();
    let save = t.elapsed();
    let size = std::fs::metadata(&p).unwrap().len();
    let t = Instant::now();
    let v2 = Vault::open(&p, "pw").unwrap();
    let open = t.elapsed();
    println!(
        "LARGE VAULT (5000 accounts, 2000 subscriptions, 20000 evidence): file {} KiB, save {save:?}, open {open:?} (incl. Argon2)",
        size / 1024
    );
    assert_eq!(v2.data.accounts.len(), 5_000);
    assert_eq!(v2.data.subscriptions.len(), 2_000);
    assert!(save.as_secs() < 5 && open.as_secs() < 10);

    // Second save also rotates the backup (copy of a multi-MB file).
    let t = Instant::now();
    v.save().unwrap();
    println!(
        "LARGE VAULT second save with backup copy: {:?}",
        t.elapsed()
    );
}

#[test]
fn export_json_is_plain_and_complete() {
    let dir = tempfile::tempdir().unwrap();
    let p = vault_path(&dir);
    let mut v = Vault::create(&p, "pw").unwrap();
    v.data.accounts.push(sample_account(1));
    let out = dir.path().join("export ä.json");
    v.export_json(&out).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    let parsed: VaultData = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed.accounts.len(), 1);
}

#[test]
fn newer_schema_is_rejected_instead_of_silently_downgraded() {
    let dir = tempfile::tempdir().unwrap();
    let p = vault_path(&dir);
    let mut v = Vault::create(&p, "pw").unwrap();
    v.data.schema = ausgeloggt_core::SCHEMA_VERSION + 5;
    v.save().unwrap();
    assert!(
        Vault::open(&p, "pw").is_err(),
        "a newer vault schema must not be opened by an older app"
    );
}
