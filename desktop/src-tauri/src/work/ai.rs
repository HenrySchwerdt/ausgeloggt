//! AI jobs (newsletter classification, SSO screenshot reading) and screenshot commit.

use std::path::{Path, PathBuf};

use ausgeloggt_ai::vision::{ScreenshotEntry, ScreenshotResult, MAX_IMAGE_BYTES};
use ausgeloggt_ai::{SubscriptionVerdict, BATCH_SIZE};
use ausgeloggt_core::merge::{self, UNRESOLVED_PREFIX};
use ausgeloggt_core::model::{
    AiProvider, AiSettings, CategorySource, Evidence, EvidenceKind, Finding, MailCategory,
    SourceKind, SourceRun, SsoProvider, Subscription,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{IpcError, IpcResult};
use crate::jobs::{JobCtx, JobFail, JobOutcome, JobResult};
use crate::state::AppState;

// ---- classification ---------------------------------------------------------------

pub type Classifier = dyn Fn(&AiSettings, &[Subscription]) -> Result<Vec<SubscriptionVerdict>, ausgeloggt_ai::AiError>
    + Send
    + Sync;

pub fn real_classifier() -> &'static Classifier {
    &|settings, subs| ausgeloggt_ai::classify_subscriptions(settings, subs, &mut |_, _| {})
}

pub fn start_classify(state: &AppState, classify: &'static Classifier) -> IpcResult<String> {
    let st = state.clone();
    state.start_job("ai_classify", move |ctx| classify_job(ctx, &st, classify))
}

pub fn classify_job(ctx: &JobCtx, state: &AppState, classify: &Classifier) -> JobResult {
    let (ai, subs) = state.read(|d| {
        let subs: Vec<Subscription> = d
            .subscriptions
            .iter()
            .filter(|s| s.category_source != CategorySource::User)
            .cloned()
            .collect();
        (d.settings.ai.clone(), subs)
    })?;
    if ai.provider == AiProvider::Off {
        return Err(IpcError::invalid(
            "KI ist deaktiviert. Bitte in den Einstellungen einschalten",
        )
        .into());
    }
    let total = subs.len() as u64;
    let mut changed = 0u64;
    let mut done = 0u64;
    for chunk in subs.chunks(BATCH_SIZE) {
        ctx.check()?;
        ctx.progress(done, total, "KI sortiert Newsletter");
        let verdicts = classify(&ai, chunk).map_err(IpcError::from)?;
        let updates: Vec<(Uuid, MailCategory)> = verdicts
            .iter()
            // Unknown with no confidence = "could not tell": keep the rule-based guess.
            .filter(|v| v.category != MailCategory::Unknown)
            .filter_map(|v| chunk.get(v.index).map(|s| (s.id, v.category)))
            .collect();
        changed += state.mutate(|d| {
            let mut n = 0;
            for (id, cat) in updates {
                if let Some(s) = d.subscriptions.iter_mut().find(|s| s.id == id) {
                    // The user may have decided in the meantime.
                    if s.category_source != CategorySource::User {
                        if s.category != cat {
                            n += 1;
                        }
                        s.category = cat;
                        s.category_source = CategorySource::Ai;
                    }
                }
            }
            n
        })?;
        done += chunk.len() as u64;
        ctx.progress(done, total, "KI sortiert Newsletter");
    }
    Ok(JobOutcome::ok(format!(
        "{total} Newsletter geprüft, {changed} neu eingeordnet"
    ))
    .with_payload(json!({ "checked": total, "changed": changed })))
}

// ---- screenshots ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenshotEntryDto {
    pub name: String,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub relay_address: Option<String>,
}

impl From<ScreenshotEntry> for ScreenshotEntryDto {
    fn from(e: ScreenshotEntry) -> Self {
        Self {
            name: e.name,
            domain: e.domain,
            relay_address: e.relay_address,
        }
    }
}

pub type VisionReader = dyn Fn(&AiSettings, &[u8], &str) -> Result<ScreenshotResult, ausgeloggt_ai::AiError>
    + Send
    + Sync;

pub fn real_vision() -> &'static VisionReader {
    &ausgeloggt_ai::vision::read_sso_screenshot
}

fn mime_for(path: &Path) -> IpcResult<&'static str> {
    match path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .as_deref()
    {
        Some("png") => Ok("image/png"),
        Some("jpg") | Some("jpeg") => Ok("image/jpeg"),
        Some("webp") => Ok("image/webp"),
        _ => Err(IpcError::invalid(
            "Nur PNG, JPEG und WebP Bilder werden unterstützt",
        )),
    }
}

pub fn start_screenshot_read(
    state: &AppState,
    paths: Vec<PathBuf>,
    reader: &'static VisionReader,
) -> IpcResult<String> {
    if paths.is_empty() {
        return Err(IpcError::invalid("Keine Bilder ausgewählt"));
    }
    let st = state.clone();
    state.start_job("screenshot_read", move |ctx| {
        screenshot_read_job(ctx, &st, &paths, reader)
    })
}

pub fn screenshot_read_job(
    ctx: &JobCtx,
    state: &AppState,
    paths: &[PathBuf],
    reader: &VisionReader,
) -> JobResult {
    let ai = state.read(|d| d.settings.ai.clone())?;
    if ai.provider == AiProvider::Off {
        return Err(IpcError::invalid(
            "KI ist deaktiviert. Bitte in den Einstellungen einschalten",
        )
        .into());
    }
    let total = paths.len() as u64;
    let mut provider: Option<SsoProvider> = None;
    let mut entries: Vec<ScreenshotEntryDto> = Vec::new();
    let mut first_error: Option<IpcError> = None;
    for (i, path) in paths.iter().enumerate() {
        ctx.check()?;
        let done = i as u64 + 1;
        let label = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        ctx.progress(done - 1, total, format!("Lese {label}"));
        let one = (|| -> IpcResult<ScreenshotResult> {
            let mime = mime_for(path)?;
            let meta = std::fs::metadata(path)?;
            if meta.len() as usize > MAX_IMAGE_BYTES {
                return Err(IpcError::invalid("Das Bild ist zu groß (maximal 8 MB)"));
            }
            let bytes = std::fs::read(path)?;
            Ok(reader(&ai, &bytes, mime)?)
        })();
        match one {
            Ok(r) => {
                if provider.is_none() {
                    provider = r.provider;
                }
                let n = r.entries.len();
                for e in r.entries {
                    let dto = ScreenshotEntryDto::from(e);
                    let dup = entries
                        .iter()
                        .any(|x| x.name.eq_ignore_ascii_case(&dto.name) && x.domain == dto.domain);
                    if !dup {
                        entries.push(dto);
                    }
                }
                ctx.item(
                    done,
                    total,
                    label.clone(),
                    true,
                    format!("{label}: {n} Einträge"),
                );
            }
            Err(e) => {
                ctx.item(
                    done,
                    total,
                    label.clone(),
                    false,
                    format!("{label}: {}", e.message),
                );
                first_error.get_or_insert(e);
            }
        }
    }
    if entries.is_empty() {
        if let Some(e) = first_error {
            return Err(JobFail::Error(e));
        }
    }
    let summary = format!("{} Einträge erkannt", entries.len());
    Ok(JobOutcome::ok(summary).with_payload(json!({ "provider": provider, "entries": entries })))
}

/// `unresolved:<slug>` key for entries without a domain.
fn unresolved_key(name: &str) -> String {
    let slug: String = name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    format!("{UNRESOLVED_PREFIX}{slug}")
}

/// `screenshot_commit`: confirmed entries become findings. Returns `(new, updated)`.
pub fn commit_screenshot(
    state: &AppState,
    provider: Option<SsoProvider>,
    entries: Vec<ScreenshotEntryDto>,
) -> IpcResult<(u64, u64)> {
    let now = Utc::now();
    let source = SourceKind::Screenshot {
        provider: provider.clone(),
    };
    let provider_label = provider
        .as_ref()
        .map(|p| p.label().to_string())
        .unwrap_or_else(|| "unbekannt".into());
    let findings: Vec<Finding> = entries
        .into_iter()
        .filter(|e| !e.name.trim().is_empty())
        .map(|e| {
            let name = e.name.trim().to_string();
            let domain = e
                .domain
                .as_deref()
                .map(str::trim)
                .filter(|d| !d.is_empty())
                .map(String::from)
                .unwrap_or_else(|| unresolved_key(&name));
            Finding {
                domain: domain.clone(),
                name: Some(name.clone()),
                identifier: e
                    .relay_address
                    .map(|r| r.trim().to_string())
                    .filter(|r| !r.is_empty()),
                sso: provider.clone(),
                evidence: Evidence {
                    kind: EvidenceKind::Screenshot,
                    source: source.clone(),
                    summary: format!("Screenshot der {provider_label} Verknüpfungen: {name}"),
                    date: Some(now),
                    source_ref: Some(format!(
                        "screenshot:{}:{}",
                        provider_label.to_lowercase(),
                        domain
                    )),
                },
            }
        })
        .collect();
    if findings.is_empty() {
        return Err(IpcError::invalid("Keine Einträge zum Übernehmen"));
    }
    state.mutate(|d| {
        let report = merge::merge_findings(d, findings);
        d.runs.push(SourceRun {
            id: Uuid::new_v4(),
            source,
            started_at: now,
            finished_at: Some(Utc::now()),
            items_scanned: report.findings + report.skipped,
            findings: report.findings,
            new_accounts: report.new_accounts,
            error: None,
        });
        (report.new_accounts, report.updated_accounts)
    })
}

/// `ai_models`: `{ text, vision }` for the settings dropdowns.
pub fn list_models(ai: &AiSettings) -> IpcResult<Value> {
    match ai.provider {
        AiProvider::Off => Ok(json!({ "text": [], "vision": [] })),
        AiProvider::Ollama => {
            let text = ausgeloggt_ai::ollama_models(&ai.ollama_url)?;
            let vision = ausgeloggt_ai::vision::ollama_vision_models(&ai.ollama_url)?;
            Ok(json!({ "text": text, "vision": vision }))
        }
        AiProvider::ClaudeCode => Ok(json!({
            "text": ausgeloggt_ai::cli::CLAUDE_MODELS, "vision": ausgeloggt_ai::cli::CLAUDE_MODELS,
        })),
        // Codex picks its model from the ChatGPT plan; empty list = "Standard".
        AiProvider::Codex => Ok(json!({ "text": [], "vision": [] })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::unlocked;
    use ausgeloggt_core::model::SubscriptionStatus;

    fn sub(name: &str, source: CategorySource) -> Subscription {
        Subscription {
            id: Uuid::new_v4(),
            mailbox: "me@example.org".into(),
            list_key: format!("{name}@x.example"),
            domain: "x.example".into(),
            name: name.into(),
            sender: format!("{name}@x.example"),
            unsubscribe: vec![],
            message_count: 2,
            frequency_per_month: 1.0,
            first_seen: None,
            last_seen: None,
            sample_subjects: vec!["Angebot".into()],
            category: MailCategory::Unknown,
            category_source: source,
            status: SubscriptionStatus::Active,
            history: vec![],
            post_unsubscribe_mails: vec![],
            last_message_id: None,
        }
    }

    fn enable_ai(f: &crate::state::testing::Fixture) {
        f.state
            .mutate(|d| d.settings.ai.provider = AiProvider::Ollama)
            .unwrap();
    }

    #[test]
    fn classify_updates_only_non_user_subscriptions() {
        let f = unlocked();
        enable_ai(&f);
        let a = sub("a", CategorySource::Rules);
        let b = sub("b", CategorySource::User);
        let c = sub("c", CategorySource::Rules);
        f.state
            .mutate(|d| d.subscriptions.extend([a, b, c]))
            .unwrap();
        let classifier: &'static Classifier =
            Box::leak(Box::new(|_ai: &AiSettings, subs: &[Subscription]| {
                // the user-decided one must never be sent
                assert!(subs.iter().all(|s| s.name != "b"));
                Ok(subs
                    .iter()
                    .enumerate()
                    .map(|(i, s)| SubscriptionVerdict {
                        index: i,
                        category: if s.name == "a" {
                            MailCategory::Marketing
                        } else {
                            MailCategory::Unknown
                        },
                        confidence: 0.9,
                        reason: "x".into(),
                    })
                    .collect())
            }));
        let st = f.state.clone();
        f.state
            .start_job("ai_classify", move |ctx| classify_job(ctx, &st, classifier))
            .unwrap();
        let done = f.host.wait_for("job://done", 10).unwrap();
        assert_eq!(done["ok"], true, "{done}");
        assert_eq!(done["payload"]["checked"], 2);
        f.state
            .read(|d| {
                let by = |n: &str| d.subscriptions.iter().find(|s| s.name == n).unwrap();
                assert_eq!(by("a").category, MailCategory::Marketing);
                assert_eq!(by("a").category_source, CategorySource::Ai);
                // unknown verdict: untouched
                assert_eq!(by("c").category_source, CategorySource::Rules);
                assert_eq!(by("b").category, MailCategory::Unknown);
                assert_eq!(by("b").category_source, CategorySource::User);
            })
            .unwrap();
    }

    #[test]
    fn classify_with_ai_off_is_invalid() {
        let f = unlocked();
        let st = f.state.clone();
        f.state
            .start_job("ai_classify", move |ctx| {
                classify_job(ctx, &st, real_classifier())
            })
            .unwrap();
        let done = f.host.wait_for("job://done", 10).unwrap();
        assert_eq!(done["error"]["code"], "Invalid");
    }

    #[test]
    fn screenshot_read_dedups_entries_across_images() {
        let f = unlocked();
        enable_ai(&f);
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("a.png");
        let p2 = dir.path().join("b.png");
        std::fs::write(&p1, b"png1").unwrap();
        std::fs::write(&p2, b"png2").unwrap();
        let reader: &'static VisionReader =
            Box::leak(Box::new(|_ai: &AiSettings, bytes: &[u8], mime: &str| {
                assert_eq!(mime, "image/png");
                let mut entries = vec![ScreenshotEntry {
                    name: "Notion".into(),
                    domain: Some("notion.so".into()),
                    relay_address: None,
                }];
                if bytes == b"png2" {
                    entries.push(ScreenshotEntry {
                        name: "Spotify".into(),
                        domain: None,
                        relay_address: Some("abc@privaterelay.appleid.com".into()),
                    });
                }
                Ok(ScreenshotResult {
                    provider: Some(SsoProvider::Apple),
                    entries,
                })
            }));
        let st = f.state.clone();
        let paths = vec![p1, p2];
        f.state
            .start_job("screenshot_read", move |ctx| {
                screenshot_read_job(ctx, &st, &paths, reader)
            })
            .unwrap();
        let done = f.host.wait_for("job://done", 10).unwrap();
        assert_eq!(done["ok"], true, "{done}");
        assert_eq!(done["payload"]["provider"], "Apple");
        assert_eq!(done["payload"]["entries"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn screenshot_rejects_unsupported_files() {
        let f = unlocked();
        enable_ai(&f);
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.gif");
        std::fs::write(&p, b"gif").unwrap();
        let st = f.state.clone();
        let paths = vec![p];
        f.state
            .start_job("screenshot_read", move |ctx| {
                screenshot_read_job(ctx, &st, &paths, real_vision())
            })
            .unwrap();
        let done = f.host.wait_for("job://done", 10).unwrap();
        assert_eq!(done["ok"], false);
        assert_eq!(done["error"]["code"], "Invalid");
    }

    #[test]
    fn commit_creates_screenshot_findings_with_relay_identifier() {
        let f = unlocked();
        let (new, updated) = commit_screenshot(
            &f.state,
            Some(SsoProvider::Apple),
            vec![
                ScreenshotEntryDto {
                    name: "Notion".into(),
                    domain: Some("notion.so".into()),
                    relay_address: Some("xyz@privaterelay.appleid.com".into()),
                },
                ScreenshotEntryDto {
                    name: "Kleine App".into(),
                    domain: None,
                    relay_address: None,
                },
                ScreenshotEntryDto {
                    name: "  ".into(),
                    domain: None,
                    relay_address: None,
                },
            ],
        )
        .unwrap();
        assert_eq!((new, updated), (2, 0));
        f.state
            .read(|d| {
                assert_eq!(d.accounts.len(), 2);
                let n = d.accounts.iter().find(|a| a.domain == "notion.so").unwrap();
                assert_eq!(n.identifiers, vec!["xyz@privaterelay.appleid.com"]);
                assert_eq!(n.sso, vec![SsoProvider::Apple]);
                assert_eq!(n.evidence[0].kind, EvidenceKind::Screenshot);
                assert_eq!(
                    n.evidence[0].source,
                    SourceKind::Screenshot {
                        provider: Some(SsoProvider::Apple)
                    }
                );
                assert!(d
                    .accounts
                    .iter()
                    .any(|a| a.domain == "unresolved:kleineapp"));
                assert_eq!(d.runs.len(), 1);
            })
            .unwrap();
        // committing again is idempotent on evidence
        let (new2, _) = commit_screenshot(
            &f.state,
            Some(SsoProvider::Apple),
            vec![ScreenshotEntryDto {
                name: "Notion".into(),
                domain: Some("notion.so".into()),
                relay_address: None,
            }],
        )
        .unwrap();
        assert_eq!(new2, 0);
        f.state
            .read(|d| assert_eq!(d.accounts[0].evidence.len(), 1))
            .unwrap();
    }

    #[test]
    fn commit_without_entries_is_invalid() {
        let f = unlocked();
        assert!(commit_screenshot(&f.state, None, vec![]).is_err());
    }

    #[test]
    fn models_for_providers() {
        let mut ai = AiSettings::default();
        assert_eq!(
            list_models(&ai).unwrap()["text"].as_array().unwrap().len(),
            0
        );
        ai.provider = AiProvider::ClaudeCode;
        assert!(
            list_models(&ai).unwrap()["vision"]
                .as_array()
                .unwrap()
                .len()
                >= 2
        );
    }
}
