//! Password audit: reuse groups, pwned lookups, 2FA presence.
//!
//! Passwords arrive as zeroizing buffers ([`PwRow`]). Equal passwords are grouped by a
//! keyed hash (SHA-256 over a random per-run key + password); the key and the digests
//! never leave this module. Output carries only counts and booleans.

use std::collections::HashMap;

use sha1::Sha1;
use sha2::{Digest, Sha256};
use spurlos_core::SecurityObservation;
use zeroize::{Zeroize, Zeroizing};

use crate::passwords::PwRow;
use crate::PwnedChecker;

/// Per-run keyed hash of a password (only used for in-memory grouping).
fn keyed(key: &[u8; 32], password: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(key);
    h.update((password.len() as u64).to_le_bytes());
    h.update(password.as_bytes());
    h.finalize().into()
}

pub(crate) fn audit(
    rows: Vec<PwRow>,
    pwned: Option<&dyn PwnedChecker>,
) -> Vec<SecurityObservation> {
    let mut key = Zeroizing::new([0u8; 32]);
    if getrandom::fill(key.as_mut_slice()).is_err() {
        // No randomness available: fall back to a time-derived key. It only has to be
        // unpredictable to nobody but this process; the digests never leave memory.
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        key[..16].copy_from_slice(&t.to_le_bytes());
    }

    // Group id per row; a representative row index per group.
    let mut groups: HashMap<[u8; 32], usize> = HashMap::new();
    let mut group_of: Vec<usize> = Vec::with_capacity(rows.len());
    let mut sizes: Vec<u32> = Vec::new();
    let mut first_row: Vec<usize> = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let d = keyed(&key, &row.password);
        let g = *groups.entry(d).or_insert_with(|| {
            sizes.push(0);
            first_row.push(i);
            sizes.len() - 1
        });
        sizes[g] += 1;
        group_of.push(g);
    }

    // One pwned lookup per distinct password.
    let pwned_of: Vec<Option<bool>> = match pwned {
        None => vec![None; sizes.len()],
        Some(checker) => first_row
            .iter()
            .map(|&i| {
                let mut digest: [u8; 20] = Sha1::digest(rows[i].password.as_bytes()).into();
                let r = checker.pwned_count(&digest).map(|n| n > 0);
                digest.zeroize();
                r
            })
            .collect(),
    };

    rows.into_iter()
        .zip(group_of)
        .map(|(row, g)| SecurityObservation {
            domain: row.domain,
            identifier: row.identifier,
            password_reuse_count: Some(sizes[g] - 1),
            password_pwned: pwned_of[g],
            has_2fa: row.has_totp.then_some(true),
        })
        .collect()
}
