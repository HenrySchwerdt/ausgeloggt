//! Receipt / renewal / price-change extraction (pure, offline, heavily tested).
//!
//! Privacy: from bodies only amount, currency, plan name, billing period and the
//! payment method (card brand + last 4 digits, IBAN last 4, wallet name) are kept.
//! Full card numbers, IBANs, addresses and names are never extracted or stored.

use std::sync::LazyLock;

use ausgeloggt_core::{PaymentKind, PaymentMethod};
use regex::Regex;

use crate::classify::{self, decode_header, fold, parse_address, MailHeader};

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

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

/// Regex matching any keyword at a word start (keywords must be folded, see [`fold`]).
fn kw(list: &[&str]) -> Regex {
    let alts: Vec<String> = list.iter().map(|k| regex::escape(k)).collect();
    Regex::new(&format!(r"\b(?:{})", alts.join("|"))).expect("keyword regex")
}

fn rx(p: &str) -> Regex {
    Regex::new(p).expect("regex")
}

// ---------------------------------------------------------------------------------------------
// Subject keywords (folded: lowercase, umlauts transliterated, accents stripped)
// ---------------------------------------------------------------------------------------------

static FAILED: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "zahlung fehlgeschlagen",
        "zahlung konnte nicht",
        "zahlungsproblem",
        "problem mit deiner zahlung",
        "problem mit ihrer zahlung",
        "problem mit der zahlung",
        "fehlgeschlagene zahlung",
        "abbuchung fehlgeschlagen",
        "lastschrift fehlgeschlagen",
        "ruecklastschrift",
        "zahlungsmethode abgelehnt",
        "zahlung abgelehnt",
        "payment failed",
        "failed payment",
        "payment declined",
        "payment unsuccessful",
        "payment was unsuccessful",
        "payment issue",
        "problem with your payment",
        "problem processing your payment",
        "unable to process your payment",
        "couldn't process your payment",
        "could not process your payment",
        "update your payment",
        "update your billing",
        "billing problem",
        "billing issue",
        "card was declined",
        "card declined",
        "paiement refuse",
        "echec du paiement",
        "paiement echoue",
        "probleme de paiement",
        "pago fallido",
        "pago rechazado",
        "problema con tu pago",
        "problema con el pago",
        "pagamento non riuscito",
        "pagamento rifiutato",
        "problema con il pagamento",
        "betaling mislukt",
        "betaling geweigerd",
        "probleem met je betaling",
        "probleem met uw betaling",
    ])
});

static CANCEL_STANDALONE: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "kuendigungsbestaetigung",
        "kuendigung bestaetigt",
        "bestaetigung ihrer kuendigung",
        "bestaetigung deiner kuendigung",
        "ihre kuendigung",
        "deine kuendigung",
        "gekuendigt",
        "kuendigung ihres",
        "kuendigung deines",
        "cancellation confirm",
        "confirmation of your cancellation",
        "your cancellation",
        "confirmation de resiliation",
        "resiliation de votre",
        "resiliation de ton",
        "confirmacion de cancelacion",
        "conferma di disdetta",
        "conferma annullamento",
        "opzegbevestiging",
        "bevestiging van je opzegging",
        "opzegging",
    ])
});

static CANCEL_WORD: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "cancel",
        "annul",
        "resili",
        "disdett",
        "opzeg",
        "cancelacion",
        "cancelad",
        "beendet",
    ])
});

static SUB_WORD: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "subscription",
        "abo",
        "membership",
        "mitgliedschaft",
        "abonnement",
        "suscripcion",
        "abbonamento",
        "vertrag",
        "premium",
        "prime",
        "trial",
        "testphase",
    ])
});

static ORDER_WORD: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "order",
        "bestellung",
        "buchung",
        "booking",
        "commande",
        "pedido",
        "ordine",
        "bestelling",
        "reservation",
        "flug",
        "ticket",
    ])
});

static ANYTIME: LazyLock<Regex> = LazyLock::new(|| kw(&["anytime", "any time", "jederzeit"]));

static REFUND: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "erstattung",
        "rueckerstattung",
        "refund",
        "gutschrift",
        "rueckzahlung",
        "geld zurueck",
        "credit note",
        "remboursement",
        "reembolso",
        "reintegro",
        "rimborso",
        "terugbetaling",
        "terugstorting",
    ])
});

static PRICE_CHANGE: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "preisanpassung",
        "preisaenderung",
        "preiserhoehung",
        "neue preise",
        "aenderung der preise",
        "aenderung ihrer preise",
        "aenderungen unserer preise",
        "aenderung deines abo",
        "preise aendern sich",
        "tarifaenderung",
        "beitragsanpassung",
        "price change",
        "price increase",
        "price update",
        "pricing change",
        "pricing update",
        "new pricing",
        "price is changing",
        "prices are changing",
        "prices are going up",
        "price will change",
        "changes to your subscription price",
        "changes to our prices",
        "update to your subscription price",
        "changement de prix",
        "changement de tarif",
        "modification de tarif",
        "modification des tarifs",
        "hausse des prix",
        "augmentation de prix",
        "nouveaux tarifs",
        "cambio de precio",
        "cambios en los precios",
        "cambio en el precio",
        "subida de precio",
        "nuevos precios",
        "variazione di prezzo",
        "variazione dei prezzi",
        "aumento del prezzo",
        "modifica dei prezzi",
        "nuovi prezzi",
        "prijswijziging",
        "prijsverhoging",
        "nieuwe prijzen",
        "prijsaanpassing",
        "wijziging van de prijs",
    ])
});

static TRIAL_WORD: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "free trial",
        "trial",
        "testphase",
        "testzeitraum",
        "probeabo",
        "probemonat",
        "probezeitraum",
        "testmonat",
        "essai gratuit",
        "periode d'essai",
        "prueba gratuita",
        "periodo de prueba",
        "prova gratuita",
        "periodo di prova",
        "proefperiode",
        "gratis proefperiode",
    ])
});

static TRIAL_STATE: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "started",
        "has started",
        "begins",
        "begun",
        "confirmed",
        "activated",
        "now active",
        "welcome to your",
        "enjoy your",
        "gestartet",
        "beginnt",
        "begonnen",
        "aktiviert",
        "bestaetigt",
        "willkommen",
        "commence",
        "demarre",
        "ha comenzado",
        "iniciada",
        "e iniziata",
        "is gestart",
        "begint",
    ])
});

static RENEWAL: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "verlaengert",
        "verlaengerung",
        "renewed",
        "renewal",
        "renouvel",
        "renovad",
        "renovacion",
        "rinnov",
        "verlengd",
        "verlenging",
        "recurring payment",
        "wiederkehrende zahlung",
        "automatische zahlung",
        "automatic payment",
        "automatische verlaengerung",
        "auto-renew",
        "abo-abrechnung",
        "subscription payment",
    ])
});

/// Reminders about a *future* renewal are not payments.
static UPCOMING: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "wird verlaengert",
        "wird sich",
        "wird bald",
        "will renew",
        "will be renewed",
        "is about to renew",
        "renews soon",
        "upcoming renewal",
        "bevorstehende",
        "demnaechst",
        "laeuft bald",
        "verlaengert sich",
        "will be charged",
        "wird abgebucht",
    ])
});

/// Words that make a mail a receipt on their own.
static STRONG_CHARGE: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "receipt",
        "invoice",
        "rechnung",
        "quittung",
        "zahlungsbestaetigung",
        "zahlungsbeleg",
        "zahlungseingang",
        "payment confirmation",
        "payment received",
        "payment successful",
        "payment to",
        "payment for",
        "your payment",
        "ihre zahlung",
        "deine zahlung",
        "zahlung an",
        "zahlung ueber",
        "zahlung von",
        "zahlung erfolgreich",
        "eine zahlung",
        "bezahlung",
        "abrechnung",
        "beleg fuer",
        "transaction receipt",
        "you paid",
        "du hast bezahlt",
        "sie haben bezahlt",
        "you sent",
        "facture",
        "recu de",
        "recu pour",
        "votre recu",
        "confirmation de paiement",
        "paiement recu",
        "factura",
        "recibo",
        "comprobante",
        "confirmacion de pago",
        "fattura",
        "ricevuta",
        "conferma di pagamento",
        "pagamento ricevuto",
        "factuur",
        "betaalbewijs",
        "betalingsbevestiging",
        "betaling ontvangen",
    ])
});

/// Order confirmations: only count if the mail is not about shipping.
static WEAK_CHARGE: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "order confirmation",
        "your order",
        "order #",
        "order no",
        "bestellbestaetigung",
        "bestellung",
        "auftragsbestaetigung",
        "kaufbestaetigung",
        "your purchase",
        "thanks for your purchase",
        "thank you for your purchase",
        "purchase confirmation",
        "purchase receipt",
        "einkauf",
        "commande",
        "confirmation de commande",
        "pedido",
        "confirmacion de compra",
        "confirmacion de pedido",
        "ordine",
        "conferma d'ordine",
        "orderbevestiging",
        "bestelling",
        "aankoopbevestiging",
    ])
});

static SHIPPING: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "shipped",
        "has shipped",
        "versandt",
        "versendet",
        "is on its way",
        "unterwegs",
        "zugestellt",
        "delivered",
        "out for delivery",
        "delivery",
        "lieferung",
        "expedie",
        "livre",
        "enviado",
        "entregado",
        "spedito",
        "consegnato",
        "verzonden",
        "bezorgd",
        "tracking",
        "sendungsverfolgung",
        "paket",
    ])
});

static BILLING_WORD: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "payment",
        "zahlung",
        "order",
        "bestell",
        "receipt",
        "invoice",
        "rechnung",
        "billing",
        "abrechnung",
        "beleg",
        "quittung",
        "subscription",
        "abo",
        "purchase",
        "kauf",
        "facture",
        "recibo",
        "fattura",
        "factuur",
    ])
});

/// Billing mailboxes of known services: any billing-ish subject from them is a charge.
const KNOWN_BILLING_HOSTS: &[&str] = &[
    "apple.com",
    "paypal.com",
    "paypal.de",
    "spotify.com",
    "netflix.com",
    "microsoft.com",
    "adobe.com",
    "steampowered.com",
    "dropbox.com",
    "openai.com",
    "disneyplus.com",
    "dazn.com",
    "audible.de",
    "audible.com",
    "github.com",
    "nintendo.com",
    "playstation.com",
    "sony.com",
    "deezer.com",
];

// ---------------------------------------------------------------------------------------------
// Senders
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Aggregator {
    Apple,
    Google,
    PayPal,
    Amazon,
}

struct Sender {
    host: String,
    local: String,
    name: Option<String>,
}

fn sender(h: &MailHeader) -> Option<Sender> {
    let (name, addr) = parse_address(&h.from);
    let addr = addr?;
    Some(Sender {
        host: classify::host_of(&addr).to_string(),
        local: classify::local_of(&addr).to_string(),
        name,
    })
}

fn aggregator(s: &Sender) -> Option<Aggregator> {
    let (host, local) = (s.host.as_str(), s.local.as_str());
    if classify::host_is(host, "apple.com") {
        return Some(Aggregator::Apple);
    }
    if classify::host_is(host, "google.com")
        && (local.starts_with("googleplay")
            || local.starts_with("google-play")
            || local.starts_with("payments-noreply")
            || local.starts_with("noreply-payments"))
    {
        return Some(Aggregator::Google);
    }
    if host.split('.').any(|l| l == "paypal") {
        return Some(Aggregator::PayPal);
    }
    if host.split('.').any(|l| l == "amazon")
        && [
            "digital-no-reply",
            "auto-confirm",
            "order-update",
            "payments-",
            "invoice",
        ]
        .iter()
        .any(|p| local.starts_with(p))
    {
        return Some(Aggregator::Amazon);
    }
    None
}

/// `amazon.de` from `mail.amazon.de`, `amazon.co.uk` from `x.amazon.co.uk`.
fn amazon_domain(host: &str) -> String {
    let parts: Vec<&str> = host.split('.').collect();
    match parts.iter().position(|p| *p == "amazon") {
        Some(i) => parts[i..].join("."),
        None => host.to_string(),
    }
}

fn via_of(agg: Aggregator, s: &Sender) -> String {
    match agg {
        Aggregator::Apple => "apple.com".into(),
        Aggregator::Google => "google.com".into(),
        Aggregator::PayPal => "paypal.com".into(),
        Aggregator::Amazon => amazon_domain(&s.host),
    }
}

fn is_known_billing_host(host: &str) -> bool {
    KNOWN_BILLING_HOSTS
        .iter()
        .any(|d| classify::host_is(host, d))
}

// ---------------------------------------------------------------------------------------------
// payment_kind
// ---------------------------------------------------------------------------------------------

fn is_bulk_promo(h: &MailHeader, folded: &str) -> bool {
    classify::is_bulk(h) && classify::is_promo(folded)
}

fn subject_kind(f: &str, h: &MailHeader) -> Option<PaymentKind> {
    if FAILED.is_match(f) {
        return Some(PaymentKind::PaymentFailed);
    }
    let sub_word = SUB_WORD.is_match(f);
    let cancel_word = CANCEL_WORD.is_match(f);
    if CANCEL_STANDALONE.is_match(f) || (cancel_word && sub_word && !ANYTIME.is_match(f)) {
        return Some(PaymentKind::Cancelled);
    }
    if cancel_word && ORDER_WORD.is_match(f) && !sub_word {
        // "Your order has been cancelled": neither a charge nor a subscription cancel.
        return None;
    }
    if REFUND.is_match(f) {
        return Some(PaymentKind::Refund);
    }
    if PRICE_CHANGE.is_match(f) {
        return Some(PaymentKind::PriceChange);
    }
    if TRIAL_WORD.is_match(f) && TRIAL_STATE.is_match(f) && !classify::is_promo(f) {
        return Some(PaymentKind::TrialStarted);
    }
    if RENEWAL.is_match(f) && !UPCOMING.is_match(f) {
        return Some(PaymentKind::Renewal);
    }
    if is_bulk_promo(h, f) {
        return None;
    }
    if STRONG_CHARGE.is_match(f) {
        return Some(PaymentKind::Charge);
    }
    if WEAK_CHARGE.is_match(f) && !SHIPPING.is_match(f) {
        return Some(PaymentKind::Charge);
    }
    // Known billing mailbox with a billing-ish subject.
    if let Some(s) = sender(h) {
        if (aggregator(&s).is_some() || is_known_billing_host(&s.host))
            && BILLING_WORD.is_match(f)
            && !SHIPPING.is_match(f)
            && !UPCOMING.is_match(f)
        {
            return Some(PaymentKind::Charge);
        }
    }
    None
}

/// Does this mail look like a receipt / renewal / price change / cancellation /
/// failed payment? Header-only (subject keywords de/en/fr/es/it/nl + known billing senders).
pub fn payment_kind(header: &MailHeader) -> Option<PaymentKind> {
    let f = fold(&decode_header(&header.subject));
    if f.is_empty() {
        return None;
    }
    subject_kind(&f, header)
}

// ---------------------------------------------------------------------------------------------
// Amounts
// ---------------------------------------------------------------------------------------------

const CUR: &str = r"(?:€|EURO?|USD|US\$|\$|£|GBP|CHF|SFr\.?)";
const NUM: &str = r"(?:\d{1,3}(?:[.,'’\x{a0}\x{202f}]\d{3})+|\d+)(?:[.,](?:\d{1,2}|[-–—]))?";

static AMT_PRE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)(?P<cur>{CUR})\s*(?P<neg>[-−–])?\s*(?P<num>{NUM})"
    ))
    .expect("amount prefix regex")
});
static AMT_SUF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)(?P<neg>[-−–])?(?P<num>{NUM})\s*(?P<cur>{CUR})(?:[^\p{{L}}]|$)"
    ))
    .expect("amount suffix regex")
});

#[derive(Debug, Clone)]
struct AmountMatch {
    start: usize,
    end: usize,
    minor: i64,
    currency: String,
}

fn currency_code(raw: &str) -> String {
    let l = raw.trim_end_matches('.').to_lowercase();
    match l.as_str() {
        "€" | "eur" | "euro" => "EUR",
        "$" | "usd" | "us$" => "USD",
        "£" | "gbp" => "GBP",
        _ => "CHF",
    }
    .to_string()
}

fn parse_number(num: &str) -> Option<i64> {
    let s: String = num
        .chars()
        .filter(|c| !matches!(c, '\u{a0}' | '\u{202f}' | ' ' | '\'' | '’'))
        .collect();
    if s.is_empty() {
        return None;
    }
    // "12.–" / "12,-"
    if s.ends_with(['-', '–', '—']) {
        let t = s
            .trim_end_matches(['-', '–', '—'])
            .trim_end_matches(['.', ',']);
        if t.is_empty() || t.len() > 9 || !t.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        return Some(t.parse::<i64>().ok()? * 100);
    }
    let (int_part, frac_part): (String, String) = match s.rfind(['.', ',']) {
        None => (s.clone(), String::new()),
        Some(i) => {
            let sep = s.as_bytes()[i] as char;
            let after = &s[i + 1..];
            let before = &s[..i];
            let other = if sep == '.' { ',' } else { '.' };
            let thousands = if before.contains(other) {
                false // both separators present: the last one is the decimal mark
            } else if before.contains(sep) {
                true // "1.299.000"
            } else {
                after.len() == 3 // "1.299"
            };
            if thousands {
                (
                    s.chars().filter(char::is_ascii_digit).collect(),
                    String::new(),
                )
            } else {
                (
                    before.chars().filter(char::is_ascii_digit).collect(),
                    after.to_string(),
                )
            }
        }
    };
    if int_part.is_empty() || int_part.len() > 9 || !frac_part.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let cents = match frac_part.len() {
        0 => 0,
        1 => frac_part.parse::<i64>().ok()? * 10,
        2 => frac_part.parse::<i64>().ok()?,
        _ => return None,
    };
    Some(int_part.parse::<i64>().ok()? * 100 + cents)
}

fn prev_char(s: &str, idx: usize) -> Option<char> {
    s[..idx].chars().next_back()
}

/// All amounts in `s`, in order of appearance.
fn amounts(s: &str) -> Vec<AmountMatch> {
    let mut found: Vec<AmountMatch> = Vec::new();
    for caps in AMT_PRE.captures_iter(s) {
        let (Some(m), Some(num), Some(cur)) = (caps.get(0), caps.name("num"), caps.name("cur"))
        else {
            continue;
        };
        let Some(minor) = parse_number(num.as_str()) else {
            continue;
        };
        let neg = caps.name("neg").is_some()
            || matches!(prev_char(s, cur.start()), Some('-' | '−' | '–'));
        found.push(AmountMatch {
            start: m.start(),
            end: num.end(),
            minor: if neg { -minor } else { minor },
            currency: currency_code(cur.as_str()),
        });
    }
    for caps in AMT_SUF.captures_iter(s) {
        let (Some(num), Some(cur)) = (caps.name("num"), caps.name("cur")) else {
            continue;
        };
        // Part of a date / longer number ("12.03.2024 €") or a word: skip.
        if matches!(prev_char(s, num.start()), Some(c) if c.is_alphanumeric() || c == '.' || c == ',')
        {
            continue;
        }
        let Some(minor) = parse_number(num.as_str()) else {
            continue;
        };
        let neg = caps.name("neg").is_some();
        found.push(AmountMatch {
            start: caps.get(0).map_or(num.start(), |m| m.start()),
            end: cur.end(),
            minor: if neg { -minor } else { minor },
            currency: currency_code(cur.as_str()),
        });
    }
    found.sort_by_key(|a| a.start);
    let mut out: Vec<AmountMatch> = Vec::new();
    for a in found {
        if out.last().is_none_or(|l| a.start >= l.end) {
            out.push(a);
        }
    }
    out
}

/// Parse an amount like `13,99 €` / `€ 1.299,00` / `$9.99` into (minor units, ISO currency).
pub fn parse_amount(s: &str) -> Option<(i64, String)> {
    amounts(s).into_iter().next().map(|a| (a.minor, a.currency))
}

// ---------------------------------------------------------------------------------------------
// HTML / text
// ---------------------------------------------------------------------------------------------

static HTML_HINT: LazyLock<Regex> = LazyLock::new(|| {
    rx(
        r"(?i)<\s*/?\s*(?:html|body|table|tr|td|th|div|p|br|span|a|b|i|strong|em|li|ul|ol|h[1-6]|img|center|font|style|head|meta|title)(?:\s[^>]*)?/?\s*>",
    )
});
static RE_SCRIPT: LazyLock<Regex> = LazyLock::new(|| rx(r"(?is)<script\b.*?</script\s*>"));
static RE_STYLE: LazyLock<Regex> = LazyLock::new(|| rx(r"(?is)<style\b.*?</style\s*>"));
static RE_HEAD: LazyLock<Regex> = LazyLock::new(|| rx(r"(?is)<head\b.*?</head\s*>"));
static RE_COMMENT: LazyLock<Regex> = LazyLock::new(|| rx(r"(?s)<!--.*?-->"));
static RE_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    rx(
        r"(?i)<\s*br\s*/?\s*>|</\s*(?:p|div|tr|li|h[1-6]|table|ul|ol|section|article|header|footer|blockquote)\s*>|<\s*(?:p|div|tr|li|h[1-6])(?:\s[^>]*)?>",
    )
});
static RE_CELL: LazyLock<Regex> = LazyLock::new(|| rx(r"(?i)</\s*t[dh]\s*>"));
static RE_TAG: LazyLock<Regex> = LazyLock::new(|| rx(r"<[^>]*>"));
static RE_ENTITY: LazyLock<Regex> =
    LazyLock::new(|| rx(r"&(#[xX][0-9a-fA-F]{1,6}|#[0-9]{1,7}|[a-zA-Z]{2,8});"));

fn decode_entities(s: &str) -> String {
    RE_ENTITY
        .replace_all(s, |c: &regex::Captures| {
            let e = &c[1];
            let ch = if let Some(hex) = e.strip_prefix("#x").or_else(|| e.strip_prefix("#X")) {
                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
            } else if let Some(dec) = e.strip_prefix('#') {
                dec.parse::<u32>().ok().and_then(char::from_u32)
            } else {
                match e {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" | "ensp" | "emsp" | "thinsp" => Some('\u{a0}'),
                    "euro" => Some('€'),
                    "pound" => Some('£'),
                    "ndash" => Some('–'),
                    "mdash" => Some('—'),
                    "bull" => Some('•'),
                    "middot" => Some('·'),
                    "hellip" => Some('…'),
                    "copy" => Some('©'),
                    "reg" => Some('®'),
                    "laquo" => Some('«'),
                    "raquo" => Some('»'),
                    "auml" => Some('ä'),
                    "ouml" => Some('ö'),
                    "uuml" => Some('ü'),
                    "Auml" => Some('Ä'),
                    "Ouml" => Some('Ö'),
                    "Uuml" => Some('Ü'),
                    "szlig" => Some('ß'),
                    "eacute" => Some('é'),
                    "egrave" => Some('è'),
                    "agrave" => Some('à'),
                    "ccedil" => Some('ç'),
                    _ => None,
                }
            };
            ch.map_or_else(|| c[0].to_string(), |c| c.to_string())
        })
        .into_owned()
}

fn html_to_text(html: &str) -> String {
    let s = RE_COMMENT.replace_all(html, "");
    let s = RE_SCRIPT.replace_all(&s, "");
    let s = RE_STYLE.replace_all(&s, "");
    let s = RE_HEAD.replace_all(&s, "");
    let s = RE_BLOCK.replace_all(&s, "\n");
    let s = RE_CELL.replace_all(&s, "\t");
    let s = RE_TAG.replace_all(&s, "");
    decode_entities(&s)
}

/// Decoded body (plain or HTML) as normalised text: one line per block, table cells
/// separated by a tab, blank lines and invisible characters removed.
fn to_text(body: &str) -> String {
    let raw = if HTML_HINT.is_match(body) {
        html_to_text(body)
    } else {
        decode_entities(body)
    };
    let raw: String = raw
        .chars()
        .filter(|c| {
            !matches!(
                c,
                '\u{200b}'..='\u{200f}' | '\u{ad}' | '\u{34f}' | '\u{feff}'
            )
        })
        .collect();
    let mut out = String::with_capacity(raw.len());
    for line in raw.replace("\r\n", "\n").replace('\r', "\n").split('\n') {
        let cells: Vec<String> = line
            .split('\t')
            .map(|c| {
                c.split(' ')
                    .filter(|w| !w.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect();
        if cells.is_empty() {
            continue;
        }
        out.push_str(&cells.join("\t"));
        out.push('\n');
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Payment method
// ---------------------------------------------------------------------------------------------

const ENDING: &str = r"(?:ending in|ending with|ends with|endet auf|endet mit|endend auf|endend mit|mit endung|finissant par|se terminant par|terminant par|terminada en|termina en|termina in|termina con|finisce con|eindigend op|eindigt op|eindigend met|nr\.?|no\.?|n°|#|last 4 digits|last four digits|letzte 4 ziffern|letzten 4 ziffern)";
const MASK: &str = r"(?:[•*·xX.\x{2022}\x{25cf}\x{2217}]+[\s\-]*)*";

static CARD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)\b(?P<brand>visa|mastercard|master card|maestro|american express|amex|girocard|discover|diners club|diners|jcb|unionpay|v pay|ec-karte|ec karte)\b(?:\s*(?:credit |debit |kredit-?|debit-?)?(?:card|karte|kreditkarte|carte|tarjeta|carta|kaart))?[\s:\-–]*(?:{ENDING}[\s:\-–]*)?{MASK}(?P<last4>\d{{4}})(?:\d)?"
    ))
    .expect("card regex")
});
static CARD_GENERIC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)\b(?:card|karte|kreditkarte|carte|tarjeta|carta|kaart)\s+{ENDING}[\s:\-–]*{MASK}(?P<last4>\d{{4}})(?:\d)?"
    ))
    .expect("generic card regex")
});
static IBAN_RE: LazyLock<Regex> =
    LazyLock::new(|| rx(r"(?i)\biban\b[\s:.\-–]*(?P<iban>[A-Z]{2}[0-9A-Z*•xX \-]{6,60})"));
static IBAN_ENDING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)\biban\b[^\n\d]{{0,25}}?{ENDING}[\s:\-–]*{MASK}(?P<last4>\d{{4}})(?:\d)?"
    ))
    .expect("iban ending regex")
});
static PAY_CONTEXT: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "payment method",
        "payment",
        "paid with",
        "paid via",
        "billed to",
        "charged to",
        "zahlungsart",
        "zahlungsmethode",
        "zahlung",
        "bezahlt mit",
        "bezahlt via",
        "abgerechnet",
        "in rechnung gestellt",
        "belastet",
        "abbuchung",
        "methode de paiement",
        "moyen de paiement",
        "paiement",
        "forma de pago",
        "metodo de pago",
        "pago",
        "metodo di pagamento",
        "pagamento",
        "betaalmethode",
        "betaling",
        "via",
    ])
});

fn card_brand(raw: &str) -> String {
    let l = fold(raw);
    match l.as_str() {
        "visa" => "Visa",
        "mastercard" | "master card" => "Mastercard",
        "maestro" => "Maestro",
        "american express" | "amex" => "Amex",
        "girocard" | "ec-karte" | "ec karte" => "Girocard",
        "discover" => "Discover",
        "diners club" | "diners" => "Diners",
        "jcb" => "JCB",
        "unionpay" => "UnionPay",
        "v pay" => "V Pay",
        _ => "Karte",
    }
    .to_string()
}

/// Is the character run after a 4-digit match a continuation of a longer number
/// (`4111 1111 1111 1111`)? Then the whole thing must be rejected.
fn continues_number(rest: &str) -> bool {
    let r = rest.trim_start_matches([' ', '-', '.', '\u{a0}']);
    r.starts_with(|c: char| c.is_ascii_digit())
}

fn extract_card(text: &str) -> Option<PaymentMethod> {
    for caps in CARD_RE.captures_iter(text) {
        let (Some(m), Some(l4)) = (caps.get(0), caps.name("last4")) else {
            continue;
        };
        // A fifth digit directly after the group means a longer number.
        if m.end() > l4.end() || continues_number(&text[l4.end()..]) {
            continue;
        }
        return Some(PaymentMethod::Card {
            brand: card_brand(&caps["brand"]),
            last4: l4.as_str().to_string(),
        });
    }
    for caps in CARD_GENERIC_RE.captures_iter(text) {
        let (Some(m), Some(l4)) = (caps.get(0), caps.name("last4")) else {
            continue;
        };
        if m.end() > l4.end() || continues_number(&text[l4.end()..]) {
            continue;
        }
        return Some(PaymentMethod::Card {
            brand: "Karte".into(),
            last4: l4.as_str().to_string(),
        });
    }
    None
}

fn extract_iban_last4(text: &str) -> Option<Option<String>> {
    if let Some(caps) = IBAN_ENDING_RE.captures(text) {
        let l4 = caps.name("last4")?;
        if !(l4.end() < text.len() && text[l4.end()..].starts_with(|c: char| c.is_ascii_digit()))
            && !continues_number(&text[l4.end()..])
        {
            return Some(Some(l4.as_str().to_string()));
        }
    }
    let caps = IBAN_RE.captures(text)?;
    let raw = &caps["iban"];
    let mut compact = String::new();
    for (i, tok) in raw.split_whitespace().enumerate() {
        let tok = tok.trim_matches('-');
        if i == 0 {
            compact.push_str(tok);
            // A full unspaced IBAN is one token; a spaced one starts with "DE89".
            if tok.len() > 4 {
                break;
            }
            continue;
        }
        let has_digit_or_mask = tok
            .chars()
            .any(|c| c.is_ascii_digit() || matches!(c, '*' | '•' | 'x' | 'X'));
        if tok.chars().count() > 4 || !has_digit_or_mask {
            break;
        }
        compact.push_str(tok);
    }
    if compact.chars().count() < 8 {
        return Some(None);
    }
    let tail: String = compact
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if tail.len() == 4 && tail.chars().all(|c| c.is_ascii_digit()) {
        Some(Some(tail))
    } else {
        Some(None)
    }
}

fn extract_wallet(text: &str) -> Option<PaymentMethod> {
    for line in text.lines() {
        let f = fold(line);
        if !PAY_CONTEXT.is_match(&f) {
            continue;
        }
        if f.contains("apple pay") {
            return Some(PaymentMethod::ApplePay);
        }
        if f.contains("google pay") || f.contains("gpay") {
            return Some(PaymentMethod::GooglePay);
        }
        if f.contains("amazon pay") {
            return Some(PaymentMethod::AmazonPay);
        }
        if f.contains("klarna") {
            return Some(PaymentMethod::Klarna);
        }
        if f.contains("paypal") {
            return Some(PaymentMethod::PayPal);
        }
        if [
            "lastschrift",
            "sepa",
            "direct debit",
            "prelevement",
            "domiciliacion",
            "addebito",
            "incasso",
            "bankeinzug",
        ]
        .iter()
        .any(|k| f.contains(k))
        {
            return Some(PaymentMethod::Sepa { iban_last4: None });
        }
        if [
            "auf rechnung",
            "pay by invoice",
            "rechnungskauf",
            "invoice payment",
        ]
        .iter()
        .any(|k| f.contains(k))
        {
            return Some(PaymentMethod::Invoice);
        }
    }
    None
}

/// Payment method from a body text. Strict: only brand + exactly 4 digits, IBAN last 4
/// digits, or a wallet name; any longer digit sequence rejects the match.
fn extract_method(text: &str) -> Option<PaymentMethod> {
    if let Some(card) = extract_card(text) {
        return Some(card);
    }
    if let Some(last4) = extract_iban_last4(text) {
        return Some(PaymentMethod::Sepa { iban_last4: last4 });
    }
    extract_wallet(text)
}

// ---------------------------------------------------------------------------------------------
// Line parsing helpers
// ---------------------------------------------------------------------------------------------

fn short_name(name: &str) -> String {
    let mut end = name.len();
    for sep in [": ", " - ", " – ", " — ", " (", " | ", " by ", " von "] {
        if let Some(i) = name.find(sep) {
            end = end.min(i);
        }
    }
    let s = name[..end].trim();
    if s.is_empty() {
        name.trim().to_string()
    } else {
        s.to_string()
    }
}

const LEGAL_TOKENS: &[&str] = &[
    "ab",
    "gmbh",
    "inc",
    "ltd",
    "limited",
    "llc",
    "ag",
    "kg",
    "bv",
    "sa",
    "sarl",
    "co",
    "corp",
    "corporation",
    "international",
    "europe",
    "deutschland",
    "germany",
    "ug",
    "ohg",
    "oy",
    "aps",
    "plc",
    "srl",
    "spa",
    "eu",
    "&",
    "und",
    "and",
    "mbh",
    "ek",
    "se",
];

/// "Netflix International B.V." -> "Netflix", "Spotify AB" -> "Spotify".
fn strip_legal(name: &str) -> String {
    let mut toks: Vec<&str> = name.split_whitespace().collect();
    while toks.len() > 1 {
        let last = toks[toks.len() - 1].to_lowercase().replace(['.', ','], "");
        if LEGAL_TOKENS.contains(&last.as_str()) {
            toks.pop();
        } else {
            break;
        }
    }
    toks.join(" ")
        .trim_matches(|c: char| c == ',' || c == '.' || c.is_whitespace())
        .to_string()
}

fn unresolved(name: &str) -> Option<String> {
    let s = classify::slug(&strip_legal(&short_name(name)));
    (!s.is_empty()).then(|| format!("{}{s}", ausgeloggt_core::merge::UNRESOLVED_PREFIX))
}

fn clean_plan(s: &str) -> Option<String> {
    let mut s = s.trim().to_string();
    if let Some(a) = amounts(&s).into_iter().next() {
        s.truncate(a.start);
    }
    let s = s.trim_matches(|c: char| {
        c.is_whitespace() || matches!(c, '-' | '–' | '—' | ':' | '|' | '(' | ',' | '/')
    });
    let s: String = s.chars().take(80).collect();
    (s.chars().any(char::is_alphanumeric)).then_some(s)
}

fn apply_kind_sign(kind: PaymentKind, minor: Option<i64>) -> Option<i64> {
    match (kind, minor) {
        (PaymentKind::Refund, Some(m)) if m > 0 => Some(-m),
        (_, m) => m,
    }
}

// ---------------------------------------------------------------------------------------------
// Subject extraction
// ---------------------------------------------------------------------------------------------

static PAYPAL_MERCHANT: LazyLock<Regex> = LazyLock::new(|| {
    rx(r"(?i)\b(?:zahlung|zahlungen|payment)\b[^\n]*?\b(?:an|to)\s+(?P<m>[^\n]+)")
});
static PAYPAL_CUT: LazyLock<Regex> = LazyLock::new(|| {
    rx(
        r"(?i)\s+(?:gesendet|sent|wurde|was|has|hat|ist|is|bestaetigt|bestätigt|confirmed|ueber|über|for|fuer|für|per|via|von|from|erhalten)\b|[:;,(\[|–—!]|\.(?:\s|$)|\s-\s",
    )
});
static PAYPAL_LABELED: LazyLock<Regex> = LazyLock::new(|| {
    rx(
        r"(?im)^(?:haendler|händler|merchant|verkaeufer|verkäufer|empfaenger|empfänger|recipient|payee)\s*[:\t]\s*(?P<m>.+)$",
    )
});

fn paypal_merchant(text: &str) -> Option<String> {
    let valid = |m: &str| -> Option<String> {
        let m = strip_legal(m.trim());
        let first = m.split_whitespace().next()?.to_lowercase();
        let bad = [
            "ihr", "ihre", "ihren", "dein", "deine", "you", "your", "me", "mich", "dich", "sie",
            "die", "der", "das", "the", "a", "ein", "eine", "paypal",
        ];
        (m.chars().count() >= 2
            && m.chars().count() <= 60
            && m.starts_with(char::is_alphanumeric)
            && !bad.contains(&first.as_str()))
        .then_some(m)
    };
    for caps in PAYPAL_MERCHANT.captures_iter(text) {
        let rest = &caps["m"];
        let cut = PAYPAL_CUT.find(rest).map_or(rest.len(), |m| m.start());
        if let Some(m) = valid(&rest[..cut]) {
            return Some(m);
        }
    }
    PAYPAL_LABELED
        .captures(text)
        .and_then(|c| valid(c["m"].trim()))
}

fn base_line(kind: PaymentKind, via: Option<String>) -> ReceiptLine {
    ReceiptLine {
        kind,
        merchant_domain: None,
        merchant_name: None,
        amount_minor: None,
        currency: None,
        plan: None,
        method: None,
        via,
    }
}

/// Extract from the subject only (e.g. "Deine Rechnung über 13,99 €", "Your receipt
/// from Spotify – €10.99"). Amount formats: `13,99 €`, `€13.99`, `EUR 13,99`,
/// `1.299,00 €`, `$9.99`, `CHF 12.–`.
pub fn extract_from_subject(header: &MailHeader) -> Vec<ReceiptLine> {
    let Some(kind) = payment_kind(header) else {
        return Vec::new();
    };
    let subject = decode_header(&header.subject);
    let snd = sender(header);
    let agg = snd.as_ref().and_then(aggregator);
    let via = match (&snd, agg) {
        (Some(s), Some(a)) => Some(via_of(a, s)),
        _ => None,
    };
    let mut line = base_line(kind, via.clone());
    if let Some((minor, cur)) = parse_amount(&subject) {
        line.amount_minor = apply_kind_sign(kind, Some(minor));
        line.currency = Some(cur);
    }
    match (agg, &snd) {
        (Some(Aggregator::PayPal), _) => {
            line.method = Some(PaymentMethod::PayPal);
            match paypal_merchant(&subject) {
                Some(m) => {
                    line.merchant_domain = unresolved(&m);
                    line.merchant_name = Some(m);
                }
                None => {
                    line.merchant_domain = Some("paypal.com".into());
                    line.merchant_name = Some("PayPal".into());
                }
            }
        }
        (Some(Aggregator::Apple), _) => {
            line.merchant_domain = Some("apple.com".into());
            line.merchant_name = Some("Apple".into());
        }
        (Some(Aggregator::Google), _) => {
            line.merchant_domain = Some("google.com".into());
            line.merchant_name = Some("Google".into());
        }
        (Some(Aggregator::Amazon), Some(s)) => {
            line.merchant_domain = Some(amazon_domain(&s.host));
            line.merchant_name = Some("Amazon".into());
        }
        (_, Some(s)) => {
            line.merchant_domain = Some(s.host.clone());
            line.merchant_name = s.name.clone();
        }
        _ => {}
    }
    vec![line]
}

// ---------------------------------------------------------------------------------------------
// Body extraction
// ---------------------------------------------------------------------------------------------

const TOTAL_STRONG: &[&str] = &[
    "total",
    "gesamt",
    "summe",
    "amount paid",
    "amount due",
    "amount charged",
    "rechnungsbetrag",
    "bezahlt",
    "charged",
    "abgebucht",
    "order total",
    "grand total",
    "zu zahlen",
    "endbetrag",
    "montant total",
    "importe total",
    "importo totale",
    "totaal",
    "totale",
];
const TOTAL_WEAK: &[&str] = &[
    "betrag", "amount", "price", "preis", "prix", "precio", "prezzo", "prijs", "bedrag", "montant",
    "importe", "importo",
];
static SUBTOTAL_RE: LazyLock<Regex> = LazyLock::new(|| {
    rx(r"\b(?:zwischensumme|subtotal|sub-total|tax|steuer|mwst|ust|vat|inkl|incl|iva|tva|btw)\b")
});

static PLAN_RE: LazyLock<Regex> = LazyLock::new(|| {
    rx(
        r"(?im)^\s*(?:plan|tarif|abo|abonnement|subscription|produkt|product|paket|artikel|article|item|membership|mitgliedschaft|description|beschreibung|beschrijving|descripcion|descrizione|offre|forfait)\s*[:\t]\s*(?P<p>.{2,100})$",
    )
});
static NEW_PRICE_LINE: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "neuer preis",
        "neuer betrag",
        "neue preis",
        "new price",
        "new rate",
        "new monthly",
        "new annual",
        "ab dem",
        "ab ",
        "starting",
        "from ",
        "will be",
        "wird",
        "kostet",
        "auf ",
        "to ",
        "neu:",
        "dès",
        "a partir",
        "vanaf",
    ])
});
static OLD_MARK: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "bisher",
        "bisherig",
        "previously",
        "currently",
        "current",
        "aktuell",
        "derzeit",
        "vorher",
        "was ",
        "alt:",
        "alter preis",
        "old price",
        "jusqu",
        "antes",
        "prima",
        "voorheen",
    ])
});

fn has_any(f: &str, list: &[&str]) -> bool {
    list.iter().any(|k| f.contains(k))
}

/// Best total amount of a receipt text.
fn find_total(text: &str) -> Option<(i64, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut strong: Option<(i64, String)> = None;
    let mut weak: Option<(i64, String)> = None;
    for (i, line) in lines.iter().enumerate() {
        let f = fold(line);
        if SUBTOTAL_RE.is_match(&f) {
            continue;
        }
        let is_strong = has_any(&f, TOTAL_STRONG);
        let is_weak = has_any(&f, TOTAL_WEAK);
        if !is_strong && !is_weak {
            continue;
        }
        let am = parse_amount(line).or_else(|| {
            // Label on one line, value on the next.
            lines.get(i + 1).and_then(|n| {
                let nf = fold(n);
                (!SUBTOTAL_RE.is_match(&nf))
                    .then(|| parse_amount(n))
                    .flatten()
            })
        });
        if let Some(a) = am {
            if is_strong {
                strong = Some(a);
            } else if weak.is_none() {
                weak = Some(a);
            }
        }
    }
    strong.or(weak).or_else(|| {
        text.lines()
            .filter(|l| !SUBTOTAL_RE.is_match(&fold(l)))
            .find_map(parse_amount)
    })
}

fn find_plan(text: &str) -> Option<String> {
    PLAN_RE
        .captures_iter(text)
        .find_map(|c| clean_plan(&c["p"]))
}

fn find_new_price(text: &str) -> Option<(i64, String)> {
    for line in text.lines() {
        let f = fold(line);
        if !NEW_PRICE_LINE.is_match(&f) && !f.contains("preis") && !f.contains("price") {
            continue;
        }
        let ams = amounts(line);
        if ams.is_empty() {
            continue;
        }
        let pick = ams
            .iter()
            .rev()
            .find(|a| {
                let from = a.start.saturating_sub(30);
                let mut from = from;
                while !line.is_char_boundary(from) {
                    from -= 1;
                }
                !OLD_MARK.is_match(&fold(&line[from..a.start]))
            })
            .or_else(|| ams.first());
        if let Some(a) = pick {
            return Some((a.minor, a.currency.clone()));
        }
    }
    None
}

fn generic_line(
    header: &MailHeader,
    text: &str,
    kind: PaymentKind,
    via: Option<String>,
    method: Option<PaymentMethod>,
) -> ReceiptLine {
    let mut line = base_line(kind, via);
    let amount = if kind == PaymentKind::PriceChange {
        find_new_price(text).or_else(|| find_total(text))
    } else {
        find_total(text)
    };
    if let Some((minor, cur)) = amount {
        line.amount_minor = apply_kind_sign(kind, Some(minor));
        line.currency = Some(cur);
    }
    line.plan = find_plan(text);
    line.method = method;
    if let Some(s) = sender(header) {
        line.merchant_domain = Some(s.host);
        line.merchant_name = s.name;
    }
    line
}

// ---- aggregator item blocks (Apple, Google Play) -------------------------------------------

static PRICE_TAIL: LazyLock<Regex> = LazyLock::new(|| {
    rx(r"(?i)^\s*(?:/\s*\p{L}+|(?:pro|per|je|par|al|a|/)\s*\p{L}+|\(.*\)|inkl\..*|incl\..*)?\s*$")
});
static INFO_LINE: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "verlaengert sich",
        "verlaengerung",
        "erneuert",
        "erneuerung",
        "renews",
        "renewal",
        "auto-renew",
        "naechste abbuchung",
        "next billing",
        "next payment",
        "wird verlaengert",
        "se renueva",
        "renouvel",
        "rinnova",
        "wordt verlengd",
    ])
});
static NOISE_LINE: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "report a problem",
        "problem melden",
        "ein problem melden",
        "probleme melden",
        "learn more",
        "mehr erfahren",
        "view in",
        "manage",
        "verwalten",
        "kuendigen",
        "cancel",
        "write a review",
        "bewertung",
    ])
});
static STOP_LINE: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "order id",
        "order number",
        "order date",
        "document no",
        "dokumentnr",
        "dokument",
        "bestellnummer",
        "bestelldatum",
        "bestellung",
        "rechnungsdatum",
        "datum",
        "date",
        "billed to",
        "abgerechnet an",
        "apple account",
        "apple-account",
        "apple id",
        "apple-id",
        "sequenz",
        "sequence",
        "payment method",
        "zahlungsmethode",
        "zahlungsart",
        "paid with",
        "bezahlt mit",
        "in rechnung gestellt",
        "total",
        "summe",
        "gesamt",
        "subtotal",
        "zwischensumme",
        "tax",
        "mwst",
        "ust",
        "vat",
        "item",
        "artikel",
        "price",
        "preis",
        "rechnung",
        "receipt",
        "thanks for",
        "vielen dank",
        "hallo",
        "hello",
        "hi ",
        "dear",
        "liebe",
    ])
});
static PERIOD_RE: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "monatlich",
        "jaehrlich",
        "woechentlich",
        "monthly",
        "yearly",
        "annual",
        "annually",
        "weekly",
        "1 monat",
        "1 jahr",
        "1 woche",
        "1 month",
        "1 year",
        "1 week",
        "3 monate",
        "6 monate",
        "12 monate",
        "3 months",
        "6 months",
        "12 months",
        "renews",
        "verlaengert sich",
        "verlaengerung",
        "erneuert",
        "renewal",
        "auto-renew",
        "abonnement",
        "subscription",
        "abo",
        "/month",
        "/monat",
        "/year",
        "/jahr",
        "per month",
        "pro monat",
        "per year",
        "pro jahr",
    ])
});
static TRIAL_LINE: LazyLock<Regex> = LazyLock::new(|| {
    kw(&[
        "free trial",
        "kostenlose testphase",
        "testphase",
        "kostenlos testen",
        "gratis",
        "free for",
        "kostenlos",
        "probeabo",
    ])
});

struct Item {
    names: Vec<String>,
    amount: i64,
    currency: String,
    period: bool,
    trial: bool,
}

/// `Some` if the whole cell is one amount (optionally followed by "/month" etc.).
fn price_cell(cell: &str) -> Option<(i64, String)> {
    let a = amounts(cell).into_iter().next()?;
    let before = cell[..a.start].trim();
    (before.is_empty() && PRICE_TAIL.is_match(&cell[a.end..])).then_some((a.minor, a.currency))
}

static DATE_ONLY: LazyLock<Regex> = LazyLock::new(|| {
    rx(r"^\d{1,2}\.\s?\d{1,2}\.\s?\d{2,4}$|^\d{4}-\d{2}-\d{2}$|^\d{1,2}/\d{1,2}/\d{2,4}$")
});

fn is_date_only(line: &str) -> bool {
    DATE_ONLY.is_match(line.trim())
}

fn parse_items(text: &str) -> Vec<Item> {
    let lines: Vec<&str> = text.lines().collect();
    // Pass 1: price lines.
    let mut price_at: Vec<Option<(i64, String)>> = vec![None; lines.len()];
    for (i, line) in lines.iter().enumerate() {
        let cells: Vec<&str> = line.split('\t').collect();
        let Some(last) = cells.last() else { continue };
        if let Some(p) = price_cell(last) {
            price_at[i] = Some(p);
        }
    }
    let mut items = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some((amount, currency)) = price_at[i].clone() else {
            continue;
        };
        if SUBTOTAL_RE.is_match(&fold(line)) {
            continue;
        }
        let cells: Vec<&str> = line.split('\t').collect();
        let same_line: Vec<String> = cells[..cells.len() - 1]
            .iter()
            .map(|c| c.to_string())
            .collect();
        // Total / tax rows ("Gesamt<TAB>13,99 €") are not items.
        if same_line
            .iter()
            .any(|c| has_any(&fold(c), TOTAL_STRONG) || SUBTOTAL_RE.is_match(&fold(c)))
        {
            continue;
        }
        // A bare amount directly after a label line ("TOTAL" / "SUMME") is a total.
        if same_line.is_empty() && i > 0 {
            let pf = fold(lines[i - 1]);
            if has_any(&pf, TOTAL_STRONG)
                || SUBTOTAL_RE.is_match(&pf)
                || [
                    "rechnung", "receipt", "datum", "date", "item", "artikel", "price", "preis",
                ]
                .contains(&pf.trim())
            {
                continue;
            }
        }
        let mut names: Vec<String> = Vec::new();
        let mut infos: Vec<String> = Vec::new();
        let mut j = i;
        let mut steps = 0;
        while j > 0 && steps < 6 && names.len() + infos.len() < 4 {
            j -= 1;
            steps += 1;
            if price_at[j].is_some() {
                break;
            }
            let lf = fold(lines[j]);
            if NOISE_LINE.is_match(&lf) {
                continue;
            }
            if INFO_LINE.is_match(&lf) {
                infos.push(lines[j].to_string());
                continue;
            }
            if is_date_only(lines[j])
                || (STOP_LINE.is_match(&lf) && !lf.contains('('))
                || lines[j].chars().filter(char::is_ascii_digit).count() >= 9
                || !lines[j].chars().any(char::is_alphabetic)
            {
                break;
            }
            names.push(lines[j].to_string());
        }
        names.reverse();
        names.extend(
            same_line
                .into_iter()
                .filter(|c| c.chars().any(char::is_alphabetic)),
        );
        let names: Vec<String> = names
            .into_iter()
            .filter(|n| !INFO_LINE.is_match(&fold(n)))
            .collect();
        if names.is_empty() {
            continue;
        }
        let all_text = fold(&format!("{} {}", names.join(" "), infos.join(" ")));
        let period = PERIOD_RE.is_match(&all_text);
        let trial = amount == 0 && TRIAL_LINE.is_match(&all_text);
        items.push(Item {
            names,
            amount,
            currency,
            period,
            trial,
        });
        if items.len() >= 12 {
            break;
        }
    }
    items
}

fn item_lines(
    text: &str,
    kind: PaymentKind,
    via: &str,
    method: &Option<PaymentMethod>,
) -> Vec<ReceiptLine> {
    parse_items(text)
        .into_iter()
        .map(|it| {
            let name = it.names[0].clone();
            let mut line = base_line(kind, Some(via.to_string()));
            let item_kind = match kind {
                PaymentKind::Charge if it.trial => PaymentKind::TrialStarted,
                PaymentKind::Charge if it.period => PaymentKind::Renewal,
                k => k,
            };
            line.kind = item_kind;
            line.merchant_name = Some(short_name(&name));
            line.merchant_domain = unresolved(&name);
            line.plan = it.names.get(1).and_then(|p| clean_plan(p));
            line.amount_minor = apply_kind_sign(item_kind, Some(it.amount));
            line.currency = Some(it.currency);
            line.method = method.clone();
            line
        })
        .collect()
}

// ---- PayPal ------------------------------------------------------------------------------------

fn paypal_lines(header: &MailHeader, text: &str, kind: PaymentKind) -> Vec<ReceiptLine> {
    let subject = decode_header(&header.subject);
    let merchant = paypal_merchant(text).or_else(|| paypal_merchant(&subject));
    let mut line = base_line(kind, Some("paypal.com".into()));
    let f = fold(&format!("{subject}\n{text}"));
    if kind == PaymentKind::Charge
        && [
            "automatische zahlung",
            "automatic payment",
            "recurring",
            "wiederkehrend",
            "abo ",
        ]
        .iter()
        .any(|k| f.contains(k))
    {
        line.kind = PaymentKind::Renewal;
    }
    // First amount near the payment sentence, else the usual total logic.
    let sentence_amount = text
        .lines()
        .find(|l| {
            let lf = fold(l);
            (lf.contains("zahlung") || lf.contains("payment")) && parse_amount(l).is_some()
        })
        .and_then(parse_amount);
    let amount = sentence_amount
        .or_else(|| find_total(text))
        .or_else(|| parse_amount(&subject));
    if let Some((minor, cur)) = amount {
        line.amount_minor = apply_kind_sign(line.kind, Some(minor));
        line.currency = Some(cur);
    }
    line.plan = find_plan(text);
    line.method = Some(PaymentMethod::PayPal);
    match merchant {
        Some(m) => {
            line.merchant_domain = unresolved(&m);
            line.merchant_name = Some(m);
        }
        None => {
            line.merchant_domain = Some("paypal.com".into());
            line.merchant_name = Some("PayPal".into());
        }
    }
    vec![line]
}

/// Extract from a decoded text/plain or text/html body (HTML stripped to text first).
/// Handles aggregator receipts: Apple ("Deine Rechnung von Apple" / "Your receipt
/// from Apple": item lines with app name, plan, renewal period, price), Google Play,
/// PayPal ("Sie haben eine Zahlung an X gesendet", automatic payments), Amazon.
/// Payment method: "Visa •••• 1234", "Mastercard endet auf 1234", "ending in 1234",
/// "PayPal", "Lastschrift", "Apple Pay", "Klarna" ...
pub fn extract_from_body(header: &MailHeader, body_text: &str) -> Vec<ReceiptLine> {
    let text = to_text(body_text);
    let kind = payment_kind(header).unwrap_or(PaymentKind::Charge);
    let snd = sender(header);
    let agg = snd.as_ref().and_then(aggregator);
    let method = extract_method(&text);
    match (agg, &snd) {
        (Some(Aggregator::PayPal), _) => paypal_lines(header, &text, kind),
        (Some(a @ (Aggregator::Apple | Aggregator::Google)), Some(s)) => {
            let via = via_of(a, s);
            let mut lines = if matches!(
                kind,
                PaymentKind::Charge | PaymentKind::Renewal | PaymentKind::Refund
            ) {
                item_lines(&text, kind, &via, &method)
            } else {
                Vec::new()
            };
            if lines.is_empty() {
                let mut l = generic_line(header, &text, kind, Some(via), method);
                let (dom, name) = if a == Aggregator::Apple {
                    ("apple.com", "Apple")
                } else {
                    ("google.com", "Google")
                };
                l.merchant_domain = Some(dom.into());
                l.merchant_name = Some(name.into());
                lines.push(l);
            }
            lines
        }
        (Some(Aggregator::Amazon), Some(s)) => {
            let mut l = generic_line(
                header,
                &text,
                kind,
                Some(via_of(Aggregator::Amazon, s)),
                method,
            );
            l.merchant_domain = Some(amazon_domain(&s.host));
            l.merchant_name = Some("Amazon".into());
            vec![l]
        }
        _ => vec![generic_line(header, &text, kind, None, method)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hdr(from: &str, subject: &str) -> MailHeader {
        MailHeader {
            from: from.into(),
            subject: subject.into(),
            message_id: Some("<m1@x>".into()),
            to: "me@example.org".into(),
            ..Default::default()
        }
    }

    fn kind(from: &str, subject: &str) -> Option<PaymentKind> {
        payment_kind(&hdr(from, subject))
    }

    const SPOTIFY: &str = "Spotify <no-reply@spotify.com>";
    const APPLE: &str = "Apple <no_reply@email.apple.com>";
    const PAYPAL: &str = "PayPal <service@paypal.de>";

    // ---- parse_amount ---------------------------------------------------------------------

    fn amt(s: &str) -> Option<(i64, String)> {
        parse_amount(s)
    }

    #[test]
    fn amount_german_suffix() {
        assert_eq!(amt("13,99 €"), Some((1399, "EUR".into())));
        assert_eq!(amt("Summe 13,99€"), Some((1399, "EUR".into())));
        assert_eq!(amt("13,99 Euro"), Some((1399, "EUR".into())));
    }

    #[test]
    fn amount_thousands() {
        assert_eq!(amt("1.299,00 €"), Some((129_900, "EUR".into())));
        assert_eq!(amt("€1,299.00"), Some((129_900, "EUR".into())));
        assert_eq!(amt("1.299 €"), Some((129_900, "EUR".into())));
        assert_eq!(amt("1\u{a0}299,50 €"), Some((129_950, "EUR".into())));
    }

    #[test]
    fn amount_prefix_symbols() {
        assert_eq!(amt("€13.99"), Some((1399, "EUR".into())));
        assert_eq!(amt("€ 9,99"), Some((999, "EUR".into())));
        assert_eq!(amt("EUR 13,99"), Some((1399, "EUR".into())));
        assert_eq!(amt("$9.99"), Some((999, "USD".into())));
        assert_eq!(amt("USD 20"), Some((2000, "USD".into())));
        assert_eq!(amt("£4.50"), Some((450, "GBP".into())));
        assert_eq!(amt("9.99 USD"), Some((999, "USD".into())));
    }

    #[test]
    fn amount_chf_dash() {
        assert_eq!(amt("CHF 12.–"), Some((1200, "CHF".into())));
        assert_eq!(amt("CHF 12.50"), Some((1250, "CHF".into())));
        assert_eq!(amt("12.- CHF"), Some((1200, "CHF".into())));
    }

    #[test]
    fn amount_negative_and_none() {
        assert_eq!(amt("-5,00 €"), Some((-500, "EUR".into())));
        assert_eq!(amt("−€5.00"), Some((-500, "EUR".into())));
        assert_eq!(amt("Keine Zahl hier"), None);
        assert_eq!(amt("Bestellung 12345"), None);
        assert_eq!(amt(""), None);
    }

    #[test]
    fn amount_ignores_dates_and_huge_numbers() {
        assert_eq!(amt("am 12.03.2024 € fällig"), None);
        assert_eq!(amt("4111111111111111 €"), None);
    }

    #[test]
    fn amount_single_decimal_and_first_wins() {
        assert_eq!(amt("9,9 €"), Some((990, "EUR".into())));
        assert_eq!(amt("von 9,99 € auf 11,99 €"), Some((999, "EUR".into())));
    }

    // ---- payment_kind ----------------------------------------------------------------------

    #[test]
    fn kind_charges_multilingual() {
        for (from, s) in [
            ("Shop <billing@shop.example>", "Deine Rechnung über 13,99 €"),
            ("Shop <billing@shop.example>", "Your receipt from Shop"),
            ("Shop <billing@shop.example>", "Zahlungsbestätigung"),
            ("Shop <billing@shop.example>", "Facture n° 2024-001"),
            ("Shop <billing@shop.example>", "Recibo de tu compra"),
            ("Shop <billing@shop.example>", "La tua fattura"),
            ("Shop <billing@shop.example>", "Uw factuur van Shop"),
            ("Shop <billing@shop.example>", "Order confirmation #1234"),
            ("Shop <billing@shop.example>", "Bestellbestätigung 4711"),
        ] {
            assert_eq!(kind(from, s), Some(PaymentKind::Charge), "{s}");
        }
    }

    #[test]
    fn kind_renewal() {
        for s in [
            "Dein Abo wurde verlängert",
            "Your subscription has been renewed",
            "Abo-Verlängerung bestätigt",
            "Votre abonnement a été renouvelé",
            "Tu suscripción se ha renovado",
            "Il tuo abbonamento è stato rinnovato",
            "Je abonnement is verlengd",
            "Sie haben eine automatische Zahlung an Spotify gesendet",
        ] {
            assert_eq!(kind(SPOTIFY, s), Some(PaymentKind::Renewal), "{s}");
        }
    }

    #[test]
    fn kind_upcoming_renewal_is_not_a_payment() {
        assert_eq!(kind(SPOTIFY, "Dein Abo wird bald verlängert"), None);
        assert_eq!(kind(SPOTIFY, "Your subscription will renew soon"), None);
    }

    #[test]
    fn kind_trial() {
        assert_eq!(
            kind(SPOTIFY, "Your free trial has started"),
            Some(PaymentKind::TrialStarted)
        );
        assert_eq!(
            kind(SPOTIFY, "Willkommen zu deiner Testphase"),
            Some(PaymentKind::TrialStarted)
        );
        // Marketing is not a started trial.
        assert_eq!(kind(SPOTIFY, "Start your free trial today"), None);
    }

    #[test]
    fn kind_price_change() {
        for s in [
            "Preisanpassung für dein Abo",
            "Wichtig: neue Preise ab Mai",
            "Important: price change to your subscription",
            "Changes to our prices",
            "Changement de tarif de votre abonnement",
            "Cambio de precio en tu suscripción",
            "Prijswijziging van je abonnement",
        ] {
            assert_eq!(kind(SPOTIFY, s), Some(PaymentKind::PriceChange), "{s}");
        }
    }

    #[test]
    fn kind_refund_cancel_failed() {
        assert_eq!(
            kind(SPOTIFY, "Deine Erstattung ist unterwegs"),
            Some(PaymentKind::Refund)
        );
        assert_eq!(
            kind(SPOTIFY, "Your refund has been processed"),
            Some(PaymentKind::Refund)
        );
        assert_eq!(
            kind(SPOTIFY, "Kündigungsbestätigung"),
            Some(PaymentKind::Cancelled)
        );
        assert_eq!(
            kind(SPOTIFY, "Your subscription has been cancelled"),
            Some(PaymentKind::Cancelled)
        );
        assert_eq!(
            kind(SPOTIFY, "Zahlung fehlgeschlagen"),
            Some(PaymentKind::PaymentFailed)
        );
        assert_eq!(
            kind(SPOTIFY, "Action needed: payment failed"),
            Some(PaymentKind::PaymentFailed)
        );
    }

    #[test]
    fn kind_non_payments() {
        for s in [
            "Willkommen bei Spotify",
            "Dein Passwort zurücksetzen",
            "Neue Anmeldung erkannt",
            "Your order has shipped",
            "Deine Bestellung ist unterwegs",
            "Your order has been cancelled",
            "Try premium free, cancel anytime",
            "",
        ] {
            assert_eq!(kind(SPOTIFY, s), None, "{s}");
        }
    }

    #[test]
    fn kind_promo_newsletter_is_not_a_charge() {
        let mut h = hdr(
            "Shop <news@shop.example>",
            "20% Rabatt auf deine nächste Bestellung",
        );
        h.has_list_unsubscribe = true;
        h.list_unsubscribe = Some("<mailto:u@shop.example>".into());
        assert_eq!(payment_kind(&h), None);
    }

    #[test]
    fn kind_known_billing_sender_fallback() {
        assert_eq!(kind(APPLE, "Deine Abrechnung"), Some(PaymentKind::Charge));
        assert_eq!(
            kind(
                "Google Play <googleplay-noreply@google.com>",
                "Deine Bestellung bei Google Play"
            ),
            Some(PaymentKind::Charge)
        );
        assert_eq!(
            kind(PAYPAL, "Sie haben eine Zahlung an Netflix gesendet"),
            Some(PaymentKind::Charge)
        );
        // Not billing-ish: no payment.
        assert_eq!(kind(APPLE, "Dein Apple-Account wurde geändert"), None);
    }

    #[test]
    fn kind_decodes_rfc2047_subject() {
        assert_eq!(
            kind(SPOTIFY, "=?UTF-8?Q?Dein_Abo_wurde_verl=C3=A4ngert?="),
            Some(PaymentKind::Renewal)
        );
    }

    // ---- subject extraction -----------------------------------------------------------------

    #[test]
    fn subject_direct_sender() {
        let h = hdr(
            "Spotify <no-reply@spotify.com>",
            "Your receipt from Spotify – €10.99",
        );
        let l = extract_from_subject(&h);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].kind, PaymentKind::Charge);
        assert_eq!(l[0].amount_minor, Some(1099));
        assert_eq!(l[0].currency.as_deref(), Some("EUR"));
        assert_eq!(l[0].merchant_domain.as_deref(), Some("spotify.com"));
        assert_eq!(l[0].merchant_name.as_deref(), Some("Spotify"));
        assert_eq!(l[0].via, None);
    }

    #[test]
    fn subject_german_amount() {
        let h = hdr(
            "Shop <billing@shop.example>",
            "Deine Rechnung über 1.299,00 €",
        );
        let l = extract_from_subject(&h);
        assert_eq!(l[0].amount_minor, Some(129_900));
    }

    #[test]
    fn subject_not_a_payment_is_empty() {
        assert!(extract_from_subject(&hdr(SPOTIFY, "Willkommen")).is_empty());
    }

    #[test]
    fn subject_paypal_merchant() {
        let h = hdr(
            PAYPAL,
            "Sie haben eine Zahlung an Netflix International B.V. gesendet",
        );
        let l = extract_from_subject(&h);
        assert_eq!(l[0].merchant_domain.as_deref(), Some("unresolved:netflix"));
        assert_eq!(l[0].merchant_name.as_deref(), Some("Netflix"));
        assert_eq!(l[0].via.as_deref(), Some("paypal.com"));
        assert_eq!(l[0].method, Some(PaymentMethod::PayPal));
    }

    #[test]
    fn subject_apple_and_refund_sign() {
        let l = extract_from_subject(&hdr(APPLE, "Deine Rechnung von Apple."));
        assert_eq!(l[0].merchant_domain.as_deref(), Some("apple.com"));
        assert_eq!(l[0].via.as_deref(), Some("apple.com"));
        let r = extract_from_subject(&hdr(SPOTIFY, "Erstattung über 9,99 €"));
        assert_eq!(r[0].kind, PaymentKind::Refund);
        assert_eq!(r[0].amount_minor, Some(-999));
    }

    // ---- HTML ---------------------------------------------------------------------------------

    #[test]
    fn html_is_stripped() {
        let t = to_text(
            "<html><head><style>p{color:red}</style></head><body><p>Hallo&nbsp;Welt &amp; Co</p><script>x()</script><table><tr><td>Plan</td><td>Premium</td></tr></table><!-- c --></body></html>",
        );
        assert!(t.contains("Hallo\u{a0}Welt & Co"), "{t}");
        assert!(t.contains("Plan\tPremium"), "{t}");
        assert!(!t.contains("color") && !t.contains("x()") && !t.contains('<'));
    }

    #[test]
    fn plain_text_with_angle_address_not_treated_as_html() {
        let t = to_text("Von: Max <max@example.org>\nGesamt: 5,00 €\n");
        assert!(t.contains("<max@example.org>"), "{t}");
    }

    #[test]
    fn entities_numeric() {
        assert_eq!(
            decode_entities("&#8364;5 &#x20AC;6 &euro;7 &bogus;"),
            "€5 €6 €7 &bogus;"
        );
    }

    // ---- payment method -------------------------------------------------------------------------

    fn method(s: &str) -> Option<PaymentMethod> {
        extract_method(s)
    }
    fn card(b: &str, l: &str) -> Option<PaymentMethod> {
        Some(PaymentMethod::Card {
            brand: b.into(),
            last4: l.into(),
        })
    }

    #[test]
    fn method_cards() {
        assert_eq!(method("Zahlungsart: Visa •••• 1234"), card("Visa", "1234"));
        assert_eq!(
            method("Mastercard endet auf 4321"),
            card("Mastercard", "4321")
        );
        assert_eq!(
            method("Paid with Visa ending in 9876"),
            card("Visa", "9876")
        );
        assert_eq!(method("American Express **** 3005"), card("Amex", "3005"));
        assert_eq!(method("Visa-1234"), card("Visa", "1234"));
        assert_eq!(method("Girocard xxxx 5555"), card("Girocard", "5555"));
        assert_eq!(method("Kreditkarte endet auf 1111"), card("Karte", "1111"));
    }

    #[test]
    fn method_never_emits_full_card_numbers() {
        assert_eq!(method("Visa 4111111111111111"), None);
        assert_eq!(method("Visa 4111 1111 1111 1111"), None);
        assert_eq!(method("Mastercard: 5500-0000-0000-0004"), None);
        assert_eq!(method("Card number 4111 1111 1111 1111"), None);
        assert_eq!(method("Visa 4111 11** **** 1111"), None);
        // masked form is fine
        assert_eq!(method("Visa •••• •••• •••• 1111"), card("Visa", "1111"));
    }

    #[test]
    fn full_pipeline_never_leaks_long_digit_sequences() {
        let body = "Rechnung\nZahlungsart: Visa 4111 1111 1111 1111\nIBAN: DE89 3704 0044 0532 0130 00\nSumme 9,99 €\n";
        let lines = extract_from_body(&hdr("Shop <billing@shop.example>", "Rechnung"), body);
        let dump = format!("{lines:?}");
        let mut run = 0;
        let mut longest = 0;
        for c in dump.chars() {
            if c.is_ascii_digit() {
                run += 1;
                longest = longest.max(run);
            } else {
                run = 0;
            }
        }
        assert!(longest <= 4, "digit run {longest} in {dump}");
        assert_eq!(
            lines[0].method,
            Some(PaymentMethod::Sepa {
                iban_last4: Some("3000".into())
            })
        );
    }

    #[test]
    fn method_iban() {
        assert_eq!(
            method("IBAN: DE89 3704 0044 0532 0130 00"),
            Some(PaymentMethod::Sepa {
                iban_last4: Some("3000".into())
            })
        );
        assert_eq!(
            method("IBAN DE89370400440532013000 Mandatsreferenz 123"),
            Some(PaymentMethod::Sepa {
                iban_last4: Some("3000".into())
            })
        );
        assert_eq!(
            method("IBAN: DE** **** **** **** **12 34"),
            Some(PaymentMethod::Sepa {
                iban_last4: Some("1234".into())
            })
        );
        assert_eq!(
            method("IBAN endet auf 0042"),
            Some(PaymentMethod::Sepa {
                iban_last4: Some("0042".into())
            })
        );
    }

    #[test]
    fn method_wallets() {
        assert_eq!(method("Zahlungsart: PayPal"), Some(PaymentMethod::PayPal));
        assert_eq!(
            method("Payment method: Apple Pay"),
            Some(PaymentMethod::ApplePay)
        );
        assert_eq!(
            method("Bezahlt mit Google Pay"),
            Some(PaymentMethod::GooglePay)
        );
        assert_eq!(
            method("Zahlungsmethode: Amazon Pay"),
            Some(PaymentMethod::AmazonPay)
        );
        assert_eq!(method("Zahlungsart: Klarna"), Some(PaymentMethod::Klarna));
        assert_eq!(
            method("Zahlungsart: Lastschrift"),
            Some(PaymentMethod::Sepa { iban_last4: None })
        );
        assert_eq!(
            method("Zahlungsart: Kauf auf Rechnung"),
            Some(PaymentMethod::Invoice)
        );
        // Wallet names outside a payment context are ignored (footers, marketing).
        assert_eq!(method("Jetzt mit PayPal bequem bestellen"), None);
        assert_eq!(method("Nichts"), None);
    }

    // ---- Apple ------------------------------------------------------------------------------------

    const APPLE_MULTI_DE: &str = "Deine Rechnung von Apple.\n\
Apple-Account\tme@example.org\n\
ABGERECHNET AN\tVisa •••• 1234\n\
DATUM\t12.03.2024\n\
BESTELLNUMMER\tMQX1234567\n\
DOKUMENTNR.\t123456789012\n\
Spotify\n\
Spotify Premium (1 Monat)\n\
Verlängert sich am 12.04.2024\n\
Problem melden\n\
9,99 €\n\
Disney+\n\
Disney+ Standard (Jährlich)\n\
Verlängert sich am 12.03.2025\n\
Problem melden\n\
89,90 €\n\
Notability\n\
Notability Pro\n\
Problem melden\n\
14,99 €\n\
ZWISCHENSUMME\t114,88 €\n\
MWST\t18,34 €\n\
SUMME\t114,88 €\n";

    #[test]
    fn apple_multi_item_receipt_de() {
        let h = hdr(APPLE, "Deine Rechnung von Apple.");
        let lines = extract_from_body(&h, APPLE_MULTI_DE);
        assert_eq!(lines.len(), 3, "{lines:#?}");
        assert_eq!(lines[0].merchant_name.as_deref(), Some("Spotify"));
        assert_eq!(
            lines[0].merchant_domain.as_deref(),
            Some("unresolved:spotify")
        );
        assert_eq!(lines[0].plan.as_deref(), Some("Spotify Premium (1 Monat)"));
        assert_eq!(lines[0].amount_minor, Some(999));
        assert_eq!(lines[0].kind, PaymentKind::Renewal);
        assert_eq!(lines[0].via.as_deref(), Some("apple.com"));
        assert_eq!(lines[0].method, card("Visa", "1234"));
        assert_eq!(
            lines[1].merchant_domain.as_deref(),
            Some("unresolved:disney")
        );
        assert_eq!(lines[1].amount_minor, Some(8990));
        assert_eq!(lines[1].kind, PaymentKind::Renewal);
        assert_eq!(lines[2].merchant_name.as_deref(), Some("Notability"));
        assert_eq!(lines[2].amount_minor, Some(1499));
        assert_eq!(lines[2].kind, PaymentKind::Charge);
        assert!(lines.iter().all(|l| l.currency.as_deref() == Some("EUR")));
    }

    #[test]
    fn apple_receipt_en_html_table() {
        let html = "<html><body><p>Your receipt from Apple.</p><table>\
<tr><td>BILLED TO</td><td>Mastercard &bull;&bull;&bull;&bull; 4321</td></tr>\
<tr><td>ORDER ID</td><td>MQX99</td></tr>\
<tr><td>DOCUMENT NO.</td><td>1234567890123</td></tr>\
<tr><td><b>Duolingo</b><br>Super Duolingo (1 Month)<br>Renews Apr 12, 2024</td><td>&euro;12.99</td></tr>\
<tr><td>Report a Problem</td></tr>\
<tr><td>TOTAL</td><td>&euro;12.99</td></tr></table></body></html>";
        let h = hdr(APPLE, "Your receipt from Apple.");
        let lines = extract_from_body(&h, html);
        assert_eq!(lines.len(), 1, "{lines:#?}");
        assert_eq!(lines[0].merchant_name.as_deref(), Some("Duolingo"));
        assert_eq!(lines[0].plan.as_deref(), Some("Super Duolingo (1 Month)"));
        assert_eq!(lines[0].amount_minor, Some(1299));
        assert_eq!(lines[0].kind, PaymentKind::Renewal);
        assert_eq!(lines[0].method, card("Mastercard", "4321"));
    }

    #[test]
    fn apple_free_trial_item() {
        let body = "Your receipt from Apple.\nORDER ID\tMQX1\nHeadspace\nHeadspace: Annual (7 days free trial)\nRenews Mar 20, 2024\n$0.00\nTOTAL\t$0.00\n";
        let lines = extract_from_body(&hdr(APPLE, "Your receipt from Apple."), body);
        assert_eq!(lines.len(), 1, "{lines:#?}");
        assert_eq!(lines[0].kind, PaymentKind::TrialStarted);
        assert_eq!(lines[0].amount_minor, Some(0));
        assert_eq!(lines[0].currency.as_deref(), Some("USD"));
    }

    #[test]
    fn apple_without_items_falls_back_to_total() {
        let body = "Your receipt from Apple.\nSUMME 4,99 €\n";
        let lines = extract_from_body(&hdr(APPLE, "Deine Rechnung von Apple."), body);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].merchant_domain.as_deref(), Some("apple.com"));
        assert_eq!(lines[0].amount_minor, Some(499));
    }

    // ---- Google Play --------------------------------------------------------------------------------

    #[test]
    fn google_play_receipt() {
        let body = "Vielen Dank für deine Bestellung bei Google Play\n\
Bestellnummer: GPA.1234-5678-9012-34567\n\
Bestelldatum: 12.03.2024\n\
Artikel\tPreis\n\
Duolingo: Sprachen lernen\n\
Super Duolingo (Duolingo Inc.)\n\
Monatliches Abo\n\
12,99 €/Monat\n\
Zwischensumme\t10,92 €\n\
MwSt\t2,07 €\n\
Gesamt: 12,99 €\n\
Zahlungsmethode: Visa-1234\n";
        let h = hdr(
            "Google Play <googleplay-noreply@google.com>",
            "Deine Bestellung bei Google Play",
        );
        let lines = extract_from_body(&h, body);
        assert_eq!(lines.len(), 1, "{lines:#?}");
        assert_eq!(lines[0].merchant_name.as_deref(), Some("Duolingo"));
        assert_eq!(
            lines[0].merchant_domain.as_deref(),
            Some("unresolved:duolingo")
        );
        assert_eq!(lines[0].amount_minor, Some(1299));
        assert_eq!(lines[0].via.as_deref(), Some("google.com"));
        assert_eq!(lines[0].method, card("Visa", "1234"));
    }

    // ---- PayPal --------------------------------------------------------------------------------------

    #[test]
    fn paypal_payment_de() {
        let body = "Hallo Max,\n\nSie haben eine Zahlung über 12,99 € EUR an Spotify AB gesendet.\n\nTransaktionscode: 8XK12345AB6789012\nZahlungsart: PayPal-Guthaben\nGesamtbetrag 12,99 € EUR\n";
        let h = hdr(PAYPAL, "Sie haben eine Zahlung an Spotify AB gesendet");
        let lines = extract_from_body(&h, body);
        assert_eq!(lines.len(), 1);
        let l = &lines[0];
        assert_eq!(l.merchant_domain.as_deref(), Some("unresolved:spotify"));
        assert_eq!(l.merchant_name.as_deref(), Some("Spotify"));
        assert_eq!(l.amount_minor, Some(1299));
        assert_eq!(l.currency.as_deref(), Some("EUR"));
        assert_eq!(l.method, Some(PaymentMethod::PayPal));
        assert_eq!(l.via.as_deref(), Some("paypal.com"));
        assert_eq!(l.kind, PaymentKind::Charge);
    }

    #[test]
    fn paypal_automatic_payment_is_renewal() {
        let body = "Sie haben eine automatische Zahlung an Netflix International B.V. gesendet.\nBetrag: 13,99 €\n";
        let h = hdr(
            PAYPAL,
            "Sie haben eine automatische Zahlung an Netflix gesendet",
        );
        let lines = extract_from_body(&h, body);
        assert_eq!(lines[0].kind, PaymentKind::Renewal);
        assert_eq!(
            lines[0].merchant_domain.as_deref(),
            Some("unresolved:netflix")
        );
        assert_eq!(lines[0].amount_minor, Some(1399));
    }

    #[test]
    fn paypal_english_receipt() {
        let body = "You sent a payment of $20.00 USD to Adobe Systems Software Ireland Ltd.\nTransaction ID: 1AB23456CD789012E\n";
        let h = hdr(
            "PayPal <service@paypal.com>",
            "Receipt for your payment to Adobe",
        );
        let lines = extract_from_body(&h, body);
        assert_eq!(
            lines[0].merchant_name.as_deref(),
            Some("Adobe Systems Software Ireland")
        );
        assert_eq!(lines[0].amount_minor, Some(2000));
        assert_eq!(lines[0].currency.as_deref(), Some("USD"));
    }

    #[test]
    fn paypal_labeled_merchant_fallback() {
        let body = "Zahlungsbestätigung\nHändler\tDeezer S.A.\nBetrag: 10,99 €\n";
        let h = hdr(PAYPAL, "Zahlungsbestätigung");
        let lines = extract_from_body(&h, body);
        assert_eq!(
            lines[0].merchant_domain.as_deref(),
            Some("unresolved:deezer")
        );
    }

    #[test]
    fn paypal_unknown_merchant_falls_back_to_paypal() {
        let h = hdr(PAYPAL, "Zahlungsbestätigung");
        let lines = extract_from_body(&h, "Betrag: 1,00 €");
        assert_eq!(lines[0].merchant_domain.as_deref(), Some("paypal.com"));
    }

    // ---- Amazon ----------------------------------------------------------------------------------------

    #[test]
    fn amazon_digital() {
        let body = "Bestelldetails\nAbonnement: Prime Video Channels – Paramount+\nBestellsumme: 7,99 €\nZahlungsart: Visa endet auf 0005\n";
        let h = hdr(
            "Amazon.de <digital-no-reply@amazon.de>",
            "Ihre Amazon.de Bestellung: Paramount+",
        );
        let lines = extract_from_body(&h, body);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].merchant_domain.as_deref(), Some("amazon.de"));
        assert_eq!(lines[0].via.as_deref(), Some("amazon.de"));
        assert_eq!(lines[0].amount_minor, Some(799));
        assert_eq!(lines[0].method, card("Visa", "0005"));
        assert_eq!(
            lines[0].plan.as_deref(),
            Some("Prime Video Channels – Paramount+")
        );
    }

    // ---- direct merchants -----------------------------------------------------------------------------

    #[test]
    fn direct_receipt_with_plan_total_and_card() {
        let body = "Danke für deine Zahlung!\n\nPlan: Premium Family\nZwischensumme 14,12 €\nMwSt 19% 2,68 €\nGesamt 16,80 €\n\nBezahlt mit Visa ending in 4242\n";
        let h = hdr(SPOTIFY, "Deine Rechnung von Spotify");
        let lines = extract_from_body(&h, body);
        assert_eq!(lines.len(), 1);
        let l = &lines[0];
        assert_eq!(l.amount_minor, Some(1680));
        assert_eq!(l.plan.as_deref(), Some("Premium Family"));
        assert_eq!(l.method, card("Visa", "4242"));
        assert_eq!(l.merchant_domain.as_deref(), Some("spotify.com"));
        assert_eq!(l.merchant_name.as_deref(), Some("Spotify"));
        assert_eq!(l.via, None);
    }

    #[test]
    fn direct_receipt_html() {
        let html = "<div>Your receipt</div><table><tr><td>Description</td><td>Pro plan</td></tr><tr><td>Amount paid</td><td>$20.00</td></tr></table><p>Paid with Visa &bull;&bull;&bull;&bull; 4242</p>";
        let h = hdr("OpenAI <noreply@tm.openai.com>", "Your receipt from OpenAI");
        let l = &extract_from_body(&h, html)[0];
        assert_eq!(l.amount_minor, Some(2000));
        assert_eq!(l.currency.as_deref(), Some("USD"));
        assert_eq!(l.plan.as_deref(), Some("Pro plan"));
        assert_eq!(l.method, card("Visa", "4242"));
    }

    #[test]
    fn price_change_takes_new_price_not_old() {
        let body =
            "Hallo,\n\nAb dem 01.05.2024 kostet dein Abo 12,99 € pro Monat (bisher 10,99 €).\n";
        let h = hdr(SPOTIFY, "Preisanpassung für dein Abo");
        let l = &extract_from_body(&h, body)[0];
        assert_eq!(l.kind, PaymentKind::PriceChange);
        assert_eq!(l.amount_minor, Some(1299));
    }

    #[test]
    fn price_change_english_from_to() {
        let body =
            "Your plan price is changing.\nNew price: $11.99/month\nCurrent price: $9.99/month\n";
        let h = hdr(
            "Netflix <info@account.netflix.com>",
            "Important: price change to your subscription",
        );
        let l = &extract_from_body(&h, body)[0];
        assert_eq!(l.amount_minor, Some(1199));
        assert_eq!(l.currency.as_deref(), Some("USD"));
    }

    #[test]
    fn refund_amount_negative_in_body() {
        let body = "Wir haben dir 9,99 € erstattet.\nErstattungsbetrag: 9,99 €\n";
        let h = hdr(SPOTIFY, "Deine Erstattung");
        let l = &extract_from_body(&h, body)[0];
        assert_eq!(l.kind, PaymentKind::Refund);
        assert_eq!(l.amount_minor, Some(-999));
    }

    #[test]
    fn cancelled_has_no_forced_amount() {
        let h = hdr(SPOTIFY, "Kündigungsbestätigung");
        let l = &extract_from_body(&h, "Dein Abo endet am 30.04.2024.")[0];
        assert_eq!(l.kind, PaymentKind::Cancelled);
        assert_eq!(l.amount_minor, None);
    }

    #[test]
    fn body_without_amount_is_still_a_line() {
        let h = hdr(SPOTIFY, "Your receipt from Spotify");
        let l = &extract_from_body(&h, "Thanks!")[0];
        assert_eq!(l.amount_minor, None);
        assert_eq!(l.merchant_domain.as_deref(), Some("spotify.com"));
    }

    #[test]
    fn total_label_on_previous_line() {
        let body = "Rechnung\nGesamtbetrag\n24,00 €\n";
        let l = &extract_from_body(&hdr(SPOTIFY, "Rechnung"), body)[0];
        assert_eq!(l.amount_minor, Some(2400));
    }

    #[test]
    fn strip_legal_forms() {
        assert_eq!(strip_legal("Netflix International B.V."), "Netflix");
        assert_eq!(strip_legal("Spotify AB"), "Spotify");
        assert_eq!(strip_legal("Foo GmbH & Co. KG"), "Foo");
        assert_eq!(strip_legal("GmbH"), "GmbH");
    }

    #[test]
    fn unresolved_slugs() {
        assert_eq!(
            unresolved("Spotify: Music").as_deref(),
            Some("unresolved:spotify")
        );
        assert_eq!(
            unresolved("Dein Foto Studio GmbH").as_deref(),
            Some("unresolved:dein-foto-studio")
        );
        assert_eq!(unresolved("!!!"), None);
    }
}
