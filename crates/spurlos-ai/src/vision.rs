//! Vision + extraction helpers (wave 2).
//!
//! - Screenshot import: read app / service names from screenshots of SSO lists that
//!   have no export (Apple "Mit Apple anmelden", Hide-My-Email list, Google third-party
//!   connections, Microsoft consents, GitHub authorized apps). Ollama vision models
//!   (qwen2.5vl, llama3.2-vision, gemma3, minicpm-v, llava) via `images: [base64]` in
//!   /api/chat; Anthropic via an `image` content block. Only the image is sent.
//! - Receipt fallback: when rules can't parse a receipt, extract amount/plan/period/
//!   payment method (brand + last 4 only) from a *redacted* text (the caller strips
//!   long digit runs / IBANs before sending).

use spurlos_core::{AiSettings, PaymentKind, SsoProvider};

use crate::AiError;

#[derive(Debug, Clone, PartialEq)]
pub struct ScreenshotEntry {
    /// App / service name as shown.
    pub name: String,
    /// Domain if visible or confidently known, else None.
    pub domain: Option<String>,
    /// Relay address if visible (Hide My Email lists).
    pub relay_address: Option<String>,
}

/// Which provider list a screenshot most likely shows + the entries on it.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenshotResult {
    pub provider: Option<SsoProvider>,
    pub entries: Vec<ScreenshotEntry>,
}

/// Installed Ollama models that can take images (by family / capabilities from /api/show).
pub fn ollama_vision_models(base_url: &str) -> Result<Vec<String>, AiError> {
    let _ = base_url;
    todo!("implemented by ai agent")
}

/// Read entries from one screenshot (PNG/JPEG bytes). Strict JSON output, validated;
/// UI chrome / headings / buttons must not become entries.
pub fn read_sso_screenshot(settings: &AiSettings, image: &[u8], mime: &str) -> Result<ScreenshotResult, AiError> {
    let _ = (settings, image, mime);
    todo!("implemented by ai agent")
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedPayment {
    pub kind: PaymentKind,
    pub merchant: Option<String>,
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub plan: Option<String>,
    /// "monthly" | "yearly" | "weekly" | "quarterly" | None
    pub period: Option<String>,
    /// e.g. "Visa 1234", "PayPal" — validated to brand + exactly 4 digits.
    pub method: Option<String>,
}

/// Fallback receipt extraction from redacted text (subject + body excerpt ≤ 4000 chars).
pub fn extract_payment(settings: &AiSettings, subject: &str, redacted_text: &str) -> Result<Vec<ExtractedPayment>, AiError> {
    let _ = (settings, subject, redacted_text);
    todo!("implemented by ai agent")
}
