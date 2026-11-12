//! Import jobs: password managers / exports, plus the Pwned Passwords adapter.

use std::path::{Path, PathBuf};

use ausgeloggt_breach::PwnedPasswords;
use ausgeloggt_core::merge;
use ausgeloggt_core::model::SourceRun;
use ausgeloggt_import::{DetectedKind, ImportOptions, PwnedChecker};
use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{IpcError, IpcResult};
use crate::jobs::{JobCtx, JobOutcome, JobResult};
use crate::state::AppState;

/// Glue between `ausgeloggt_breach::PwnedPasswords` and `ausgeloggt_import::PwnedChecker`.
pub struct PwnedAdapter(pub PwnedPasswords);

impl PwnedChecker for PwnedAdapter {
    fn pwned_count(&self, sha1: &[u8; 20]) -> Option<u64> {
        self.0.count(sha1).ok()
    }
}

/// `import_detect`: `{ kind, label_de, format? }`.
pub fn detect(path: &Path) -> IpcResult<Value> {
    let kind = ausgeloggt_import::detect(path)?;
    Ok(match kind {
        DetectedKind::Passwords(format) => json!({
            "kind": "passwords",
            "label_de": format!("Passwort-Export ({})", format.label()),
            "format": format,
        }),
        DetectedKind::GoogleTakeout => {
            json!({ "kind": "google_takeout", "label_de": "Google Takeout" })
        }
        DetectedKind::FacebookExport => {
            json!({ "kind": "facebook_export", "label_de": "Facebook-Export" })
        }
        DetectedKind::GenericList => json!({ "kind": "generic_list", "label_de": "Liste" }),
    })
}

pub fn start_import(state: &AppState, paths: Vec<PathBuf>, check_pwned: bool) -> IpcResult<String> {
    if paths.is_empty() {
        return Err(IpcError::invalid("Keine Dateien ausgewählt"));
    }
    let st = state.clone();
    state.start_job("import", move |ctx| {
        if check_pwned {
            let adapter = PwnedAdapter(PwnedPasswords::new());
            import_job(ctx, &st, &paths, Some(&adapter))
        } else {
            import_job(ctx, &st, &paths, None)
        }
    })
}

pub fn import_job(
    ctx: &JobCtx,
    state: &AppState,
    paths: &[PathBuf],
    pwned: Option<&dyn PwnedChecker>,
) -> JobResult {
    let total = paths.len() as u64;
    let mut new_accounts = 0u64;
    let mut findings = 0u64;
    let mut ok_files = 0u64;
    let mut warnings: Vec<String> = Vec::new();
    let mut first_error: Option<IpcError> = None;

    for (i, path) in paths.iter().enumerate() {
        ctx.check()?;
        let done = i as u64;
        let label = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        ctx.progress(done, total, format!("Lese {label}"));
        let started = Utc::now();
        let options = ImportOptions { pwned };
        match ausgeloggt_import::import_path_with(path, &options) {
            Ok(audited) => {
                let result = audited.result;
                warnings.extend(result.warnings.iter().cloned());
                let source = result.source.clone();
                let items = result.items_scanned;
                let report = state.mutate(|d| {
                    let report = merge::merge_findings(d, result.findings);
                    merge::merge_security(d, audited.security);
                    d.runs.push(SourceRun {
                        id: Uuid::new_v4(),
                        source,
                        started_at: started,
                        finished_at: Some(Utc::now()),
                        items_scanned: items,
                        findings: report.findings,
                        new_accounts: report.new_accounts,
                        error: None,
                    });
                    report
                })?;
                new_accounts += report.new_accounts;
                findings += report.findings;
                ok_files += 1;
                ctx.item(
                    done + 1,
                    total,
                    label.clone(),
                    true,
                    format!(
                        "{label}: {} Einträge, {} neue Accounts",
                        items, report.new_accounts
                    ),
                );
            }
            Err(e) => {
                let err = IpcError::from(e);
                ctx.item(
                    done + 1,
                    total,
                    label.clone(),
                    false,
                    format!("{label}: {}", err.message),
                );
                first_error.get_or_insert(err);
            }
        }
    }

    let summary =
        format!("{ok_files} von {total} Dateien importiert, {new_accounts} neue Accounts");
    let payload = json!({
        "files": total, "imported": ok_files, "findings": findings,
        "newAccounts": new_accounts, "warnings": warnings,
    });
    match first_error {
        None => Ok(JobOutcome::ok(summary).with_payload(payload)),
        Some(e) => {
            let mut o = JobOutcome::failed(summary, e);
            o.payload = Some(payload);
            Ok(o)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::unlocked;

    const CSV: &str = "name,url,username,password\n\
        Spotify,https://accounts.spotify.com/login,erika@example.org,hunter2hunter2\n\
        Netflix,https://www.netflix.com/login,erika@example.org,hunter2hunter2\n\
        Github,https://github.com/login,erika,anderes-passwort-xyz\n";

    struct Pwned;
    impl PwnedChecker for Pwned {
        fn pwned_count(&self, _sha1: &[u8; 20]) -> Option<u64> {
            Some(42)
        }
    }

    fn write_csv(dir: &Path) -> PathBuf {
        let p = dir.join("passwords.csv");
        std::fs::write(&p, CSV).unwrap();
        p
    }

    #[test]
    fn detect_labels_password_csv() {
        let dir = tempfile::tempdir().unwrap();
        let p = write_csv(dir.path());
        let v = detect(&p).unwrap();
        assert_eq!(v["kind"], "passwords");
        assert!(v["label_de"]
            .as_str()
            .unwrap()
            .starts_with("Passwort-Export"));
        assert!(detect(&dir.path().join("fehlt.csv")).is_err());
    }

    #[test]
    fn import_merges_findings_security_and_run_without_passwords() {
        let f = unlocked();
        let dir = tempfile::tempdir().unwrap();
        let p = write_csv(dir.path());
        let st = f.state.clone();
        let paths = vec![p];
        f.state
            .start_job("import", move |ctx| {
                import_job(ctx, &st, &paths, Some(&Pwned))
            })
            .unwrap();
        let done = f.host.wait_for("job://done", 10).unwrap();
        assert_eq!(done["ok"], true, "{done}");
        assert_eq!(done["payload"]["newAccounts"], 3);
        f.state
            .read(|d| {
                assert_eq!(d.accounts.len(), 3);
                assert_eq!(d.runs.len(), 1);
                // reuse (two accounts share a password) and pwned flags were merged
                let spotify = d
                    .accounts
                    .iter()
                    .find(|a| a.domain == "spotify.com")
                    .unwrap();
                assert_eq!(spotify.security.password_reuse_count, Some(1));
                assert_eq!(spotify.security.password_pwned, Some(true));
                let dump = serde_json::to_string(d).unwrap();
                assert!(!dump.contains("hunter2hunter2"));
                assert!(!dump.contains("anderes-passwort-xyz"));
            })
            .unwrap();
    }

    #[test]
    fn reimport_is_idempotent_and_bad_file_is_reported() {
        let f = unlocked();
        let dir = tempfile::tempdir().unwrap();
        let good = write_csv(dir.path());
        let bad = dir.path().join("kaputt.bin");
        std::fs::write(&bad, [0u8, 1, 2, 3]).unwrap();
        for _ in 0..2 {
            f.host.events.lock().unwrap().clear();
            let st = f.state.clone();
            let paths = vec![good.clone(), bad.clone()];
            f.state
                .start_job("import", move |ctx| import_job(ctx, &st, &paths, None))
                .unwrap();
            let done = f.host.wait_for("job://done", 10).unwrap();
            assert_eq!(done["ok"], false);
            assert_eq!(done["payload"]["imported"], 1);
            assert!(done["error"]["message"].is_string());
        }
        f.state.read(|d| assert_eq!(d.accounts.len(), 3)).unwrap();
    }

    #[test]
    fn empty_path_list_is_invalid() {
        let f = unlocked();
        assert!(start_import(&f.state, vec![], false).is_err());
    }
}
