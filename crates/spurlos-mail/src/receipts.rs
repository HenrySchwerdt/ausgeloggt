//! Receipt / renewal / price-change extraction (pure, offline, heavily tested).
//!
//! Privacy: from bodies only amount, currency, plan name, billing period and the
//! payment method (card brand + last 4 digits, IBAN last 4, wallet name) are kept.
//! Full card numbers, IBANs, addresses and names are never extracted or stored.

use spurlos_core::{PaymentKind, PaymentMethod};

use crate::classify::MailHeader;

/// One extracted payment line. Aggregator receipts (Apple, Google Play, PayPal,
/// Amazon digital) yield one line per item with its own merchant.
#[derive(Debug, Clone, PartialEq)]
pub struct ReceiptLine {
    pub kind: PaymentKind,
    /// Merchant domain if known, else `unresolved:<slug>` of `merchant_name`.
    pub merchant_domain: Option<String>,
    pub merchant_name: Option<String>,
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub plan: Option<String>,
    pub method: Option<PaymentMethod>,
    /// Aggregator that sent the mail (`apple.com`, `google.com`, `paypal.com`, `amazon.de` ...).
    pub via: Option<String>,
}

/// Does this mail look like a receipt / renewal / price change / cancellation /
/// failed payment? Header-only (subject keywords de/en/fr/es/it/nl + known billing senders).
pub fn payment_kind(header: &MailHeader) -> Option<PaymentKind> {
    let _ = header;
    todo!("implemented by mail agent")
}

/// Extract from the subject only (e.g. "Deine Rechnung über 13,99 €", "Your receipt
/// from Spotify – €10.99"). Amount formats: `13,99 €`, `€13.99`, `EUR 13,99`,
/// `1.299,00 €`, `$9.99`, `CHF 12.–`.
pub fn extract_from_subject(header: &MailHeader) -> Vec<ReceiptLine> {
    let _ = header;
    todo!("implemented by mail agent")
}

/// Extract from a decoded text/plain or text/html body (HTML stripped to text first).
/// Handles aggregator receipts: Apple ("Deine Rechnung von Apple" / "Your receipt
/// from Apple": item lines with app name, plan, renewal period, price), Google Play,
/// PayPal ("Sie haben eine Zahlung an X gesendet", automatic payments), Amazon.
/// Payment method: "Visa •••• 1234", "Mastercard endet auf 1234", "ending in 1234",
/// "PayPal", "Lastschrift", "Apple Pay", "Klarna" ...
pub fn extract_from_body(header: &MailHeader, body_text: &str) -> Vec<ReceiptLine> {
    let _ = (header, body_text);
    todo!("implemented by mail agent")
}

/// Parse an amount like `13,99 €` / `€ 1.299,00` / `$9.99` into (minor units, ISO currency).
pub fn parse_amount(s: &str) -> Option<(i64, String)> {
    let _ = s;
    todo!("implemented by mail agent")
}
