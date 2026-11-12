//! Optional AI classification of newsletters and uncertain account findings.
//!
//! Privacy: only sender name/address/domain and mail subjects are sent to the model,
//! never bodies, never other identifiers. Ollama (local) is the recommended provider;
//! Anthropic is opt-in. All calls are blocking with timeouts; call from worker threads.
//! Batched (e.g. 20 items per request) with strict JSON output, validated and
//! falling back to "Unknown" on any parse problem.

use spurlos_core::{AiSettings, MailCategory, Subscription};

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("KI ist deaktiviert")]
    Disabled,
    #[error("Ollama nicht erreichbar unter {0}. Läuft Ollama?")]
    OllamaUnreachable(String),
    #[error("Kein API-Key hinterlegt")]
    MissingApiKey,
    #[error("KI-Anfrage fehlgeschlagen: {0}")]
    Request(String),
    #[error("Ungültige KI-Antwort: {0}")]
    BadResponse(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubscriptionVerdict {
    /// Index into the input slice.
    pub index: usize,
    pub category: MailCategory,
    /// 0..=1
    pub confidence: f32,
    /// One short German sentence, shown in the UI ("Rabattaktionen eines Modehändlers").
    pub reason: String,
}

/// List installed Ollama models (`GET /api/tags`), for the settings dropdown.
pub fn ollama_models(base_url: &str) -> Result<Vec<String>, AiError> {
    let _ = base_url;
    todo!("implemented by ai agent")
}

/// Quick health check of the configured provider.
pub fn test_provider(settings: &AiSettings) -> Result<String, AiError> {
    let _ = settings;
    todo!("implemented by ai agent")
}

/// Classify subscriptions into Marketing / Newsletter / Social / Transactional / Spam.
/// `on_progress(done, total)` is called after each batch.
pub fn classify_subscriptions(
    settings: &AiSettings,
    subs: &[Subscription],
    on_progress: &mut dyn FnMut(usize, usize),
) -> Result<Vec<SubscriptionVerdict>, AiError> {
    let _ = (settings, subs, on_progress);
    todo!("implemented by ai agent")
}

/// For an uncertain account (e.g. only newsletter/receipt evidence): is it likely a real
/// account? Input: service name, domain and evidence summaries (subjects).
/// Returns probability 0..=1 and a short German reason.
pub fn judge_account(
    settings: &AiSettings,
    name: &str,
    domain: &str,
    evidence_summaries: &[String],
) -> Result<(f32, String), AiError> {
    let _ = (settings, name, domain, evidence_summaries);
    todo!("implemented by ai agent")
}
