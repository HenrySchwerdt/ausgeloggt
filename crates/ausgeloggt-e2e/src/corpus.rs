//! Deterministic synthetic mailbox generator with ground truth.
//!
//! All data is fake: the sender domains are real, well-known service domains (so the
//! domain / PSL / catalog code paths are exercised realistically), but every address,
//! amount and person is invented. The generator produces raw RFC 5322 header blocks
//! (as an IMAP server would deliver them) plus optional bodies for receipts / SSO mails.

use std::collections::{BTreeMap, BTreeSet};

use ausgeloggt_core::domain::{canonical_service_domain, registrable_domain};
use ausgeloggt_core::insights::BillingInterval;
use ausgeloggt_core::{PaymentMethod, SsoProvider};
use chrono::{DateTime, Datelike, Duration, FixedOffset, Months, TimeZone, Utc};

use crate::rng::Rng;

/// The user's own mailbox address (mails *from* it must never create accounts).
pub const OWN: &str = "erika.beispiel@gmx.de";

/// "Now" for all insight computations on the corpus.
pub fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 1, 12, 0, 0).unwrap()
}

#[derive(Debug, Clone, Default)]
pub struct RawMail {
    pub uid: u32,
    pub header: Vec<u8>,
    pub body: Option<Vec<u8>>,
    pub body_headers: String,
    pub internal_date: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    De,
    En,
    Fr,
}

#[derive(Debug, Clone)]
pub struct ExpectedAccount {
    pub name: String,
    /// Canonical key (`spotify.com` or `unresolved:slug`).
    pub key: String,
    pub slug: String,
    pub sso: Option<SsoProvider>,
}

#[derive(Debug, Clone)]
pub struct ExpectedList {
    pub list_key: String,
    pub domain: String,
    /// Unique (by Message-ID) mails of the list.
    pub count: u64,
    pub one_click: bool,
    pub any_unsub: bool,
}

#[derive(Debug, Clone)]
pub struct ExpectedBilling {
    pub key: String,
    pub name: String,
    pub interval: Option<BillingInterval>,
    pub amount_minor: i64,
    pub currency: String,
    pub active: bool,
    pub price_increased: bool,
    pub method: Option<PaymentMethod>,
    pub via: Option<String>,
    pub bodies_needed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Truth {
    /// Services that must be detected as accounts (confidence >= 0.5).
    pub accounts: Vec<ExpectedAccount>,
    /// Domains that may become accounts (e.g. order confirmations of guest shops).
    pub tolerated: BTreeSet<String>,
    /// Domains that must NOT become accounts with confidence >= 0.5.
    pub negatives: BTreeSet<String>,
    /// Phishing domains: classifying them as accounts is a known limitation (reported only).
    pub phishing: BTreeSet<String>,
    /// Accounts that only have unusual-wording mails (recall is expected to be lower).
    pub hard: BTreeSet<String>,
    pub lists: Vec<ExpectedList>,
    pub billing: Vec<ExpectedBilling>,
    pub unique_mails: u64,
    pub duplicate_mails: u64,
    pub service_count: usize,
    /// Per-service ground truth for payment method index: method -> account keys.
    pub method_index: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Debug, Clone)]
pub struct Corpus {
    pub mails: Vec<RawMail>,
    pub truth: Truth,
}

// ------------------------------------------------------------------------------------------
// Rosters
// ------------------------------------------------------------------------------------------

type Svc = (&'static str, &'static str, Lang);
use Lang::{De, En, Fr};

const FULL: &[Svc] = &[
    ("GitHub", "github.com", En),
    ("GitLab", "gitlab.com", En),
    ("Stack Overflow", "stackoverflow.com", En),
    ("Figma", "figma.com", En),
    ("Trello", "trello.com", En),
    ("Asana", "asana.com", En),
    ("Zoom", "zoom.us", En),
    ("Twitch", "twitch.tv", En),
    ("Steam", "steampowered.com", En),
    ("Epic Games", "epicgames.com", En),
    ("Ubisoft", "ubisoft.com", Fr),
    ("Electronic Arts", "ea.com", En),
    ("GOG", "gog.com", En),
    ("itch.io", "itch.io", En),
    ("Airbnb", "airbnb.de", De),
    ("Booking.com", "booking.com", De),
    ("Tripadvisor", "tripadvisor.de", De),
    ("Uber", "uber.com", En),
    ("Bolt", "bolt.eu", En),
    ("FlixBus", "flixbus.de", De),
    ("Deutsche Bahn", "bahn.de", De),
    ("Lufthansa", "lufthansa.com", De),
    ("Ryanair", "ryanair.com", En),
    ("easyJet", "easyjet.com", En),
    ("Zalando", "zalando.de", De),
    ("OTTO", "otto.de", De),
    ("eBay", "ebay.de", De),
    ("Kleinanzeigen", "kleinanzeigen.de", De),
    ("MediaMarkt", "mediamarkt.de", De),
    ("IKEA", "ikea.com", De),
    ("dm", "dm.de", De),
    ("Lieferando", "lieferando.de", De),
    ("Wolt", "wolt.com", En),
    ("Flink", "goflink.com", De),
    ("CHECK24", "check24.de", De),
    ("N26", "n26.com", De),
    ("ING", "ing.de", De),
    ("DKB", "dkb.de", De),
    ("comdirect", "comdirect.de", De),
    ("Trade Republic", "traderepublic.com", De),
    ("Klarna", "klarna.com", De),
    ("Wise", "wise.com", En),
    ("Revolut", "revolut.com", En),
    ("Komoot", "komoot.com", De),
    ("Coursera", "coursera.org", En),
    ("Udemy", "udemy.com", En),
    ("LinkedIn", "linkedin.com", En),
    ("XING", "xing.com", De),
    ("Mozilla", "mozilla.org", En),
    ("Vinted", "vinted.de", De),
    ("Etsy", "etsy.com", En),
    ("AliExpress", "aliexpress.com", En),
    ("Temu", "temu.com", En),
    ("Decathlon", "decathlon.fr", Fr),
    ("Fnac", "fnac.com", Fr),
    ("Leboncoin", "leboncoin.fr", Fr),
    ("Cdiscount", "cdiscount.com", Fr),
    ("BlaBlaCar", "blablacar.fr", Fr),
    ("Doctolib", "doctolib.de", De),
    ("PayPal", "paypal.de", De),
    ("Amazon", "amazon.de", De),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pay {
    Visa4242,
    Mc5454,
    Sepa,
}

struct Paid {
    svc: Svc,
    interval: BillingInterval,
    amount: i64,
    cur: &'static str,
    pay: Pay,
    plan: &'static str,
    cancelled: bool,
    trial: bool,
    refund: bool,
    /// New price after a price-change notice.
    new_price: Option<i64>,
}

const fn paid(
    svc: Svc,
    interval: BillingInterval,
    amount: i64,
    cur: &'static str,
    pay: Pay,
    plan: &'static str,
) -> Paid {
    Paid {
        svc,
        interval,
        amount,
        cur,
        pay,
        plan,
        cancelled: false,
        trial: false,
        refund: false,
        new_price: None,
    }
}

use BillingInterval::{
    BiMonthly, BiWeekly, FourMonthly, HalfYearly, Monthly, Quarterly, Weekly, Yearly,
};

fn paid_roster() -> Vec<Paid> {
    let mut v = vec![
        paid(
            ("Spotify", "spotify.com", De),
            Monthly,
            1099,
            "EUR",
            Pay::Visa4242,
            "Premium Individual",
        ),
        Paid {
            new_price: Some(1549),
            ..paid(
                ("Netflix", "netflix.com", De),
                Monthly,
                1399,
                "EUR",
                Pay::Visa4242,
                "Standard",
            )
        },
        paid(
            ("Disney+", "disneyplus.com", De),
            Yearly,
            8990,
            "EUR",
            Pay::Mc5454,
            "Standard Jaehrlich",
        ),
        paid(
            ("Dropbox", "dropbox.com", En),
            Yearly,
            11988,
            "USD",
            Pay::Visa4242,
            "Plus",
        ),
        paid(
            ("Notion", "notion.so", En),
            Monthly,
            1000,
            "USD",
            Pay::Visa4242,
            "Plus",
        ),
        paid(
            ("OpenAI", "openai.com", En),
            Monthly,
            2000,
            "USD",
            Pay::Visa4242,
            "ChatGPT Plus",
        ),
        paid(
            ("Adobe", "adobe.com", De),
            Monthly,
            2419,
            "EUR",
            Pay::Mc5454,
            "Creative Cloud",
        ),
        paid(
            ("The New York Times", "nytimes.com", En),
            Monthly,
            1700,
            "USD",
            Pay::Visa4242,
            "All Access",
        ),
        paid(
            ("DER SPIEGEL", "spiegel.de", De),
            Monthly,
            1999,
            "EUR",
            Pay::Sepa,
            "Spiegel+",
        ),
        Paid {
            new_price: Some(2090),
            ..paid(
                ("ZEIT", "zeit.de", De),
                Monthly,
                1790,
                "EUR",
                Pay::Sepa,
                "Zeit Digital",
            )
        },
        Paid {
            cancelled: true,
            ..paid(
                ("Sky", "sky.de", De),
                Monthly,
                2500,
                "EUR",
                Pay::Sepa,
                "Entertainment",
            )
        },
        Paid {
            cancelled: true,
            ..paid(
                ("DAZN", "dazn.com", De),
                Monthly,
                2999,
                "EUR",
                Pay::Visa4242,
                "Standard",
            )
        },
        Paid {
            trial: true,
            ..paid(
                ("Audible", "audible.de", De),
                Monthly,
                995,
                "EUR",
                Pay::Mc5454,
                "Standard",
            )
        },
        paid(
            ("1Password", "1password.com", En),
            Yearly,
            3588,
            "USD",
            Pay::Visa4242,
            "Families",
        ),
        paid(
            ("NordVPN", "nordvpn.com", En),
            Yearly,
            5976,
            "EUR",
            Pay::Visa4242,
            "Complete",
        ),
        paid(
            ("PlayStation", "playstation.com", De),
            Quarterly,
            2499,
            "EUR",
            Pay::Mc5454,
            "PlayStation Plus Essential",
        ),
        Paid {
            refund: true,
            ..paid(
                ("Xbox", "xbox.com", De),
                Monthly,
                1299,
                "EUR",
                Pay::Visa4242,
                "Game Pass Ultimate",
            )
        },
        paid(
            ("Duolingo", "duolingo.com", En),
            Monthly,
            1299,
            "EUR",
            Pay::Visa4242,
            "Super",
        ),
        Paid {
            trial: true,
            ..paid(
                ("Headspace", "headspace.com", En),
                Yearly,
                6999,
                "USD",
                Pay::Visa4242,
                "Annual",
            )
        },
        Paid {
            cancelled: true,
            ..paid(
                ("McFit", "mcfit.com", De),
                Monthly,
                2490,
                "EUR",
                Pay::Sepa,
                "Premium",
            )
        },
        paid(
            ("Urban Sports Club", "urbansportsclub.com", De),
            Monthly,
            5900,
            "EUR",
            Pay::Sepa,
            "M",
        ),
        paid(
            ("Vodafone", "vodafone.de", De),
            Monthly,
            3999,
            "EUR",
            Pay::Sepa,
            "GigaMobil",
        ),
        paid(
            ("Telekom", "telekom.de", De),
            Monthly,
            4995,
            "EUR",
            Pay::Sepa,
            "MagentaZuhause",
        ),
        paid(
            ("1&1", "1und1.de", De),
            Monthly,
            1999,
            "EUR",
            Pay::Sepa,
            "DSL 100",
        ),
        Paid {
            refund: true,
            ..paid(
                ("Tidal", "tidal.com", En),
                Monthly,
                1099,
                "EUR",
                Pay::Mc5454,
                "HiFi",
            )
        },
        Paid {
            refund: true,
            ..paid(
                ("Deezer", "deezer.com", Fr),
                Monthly,
                1099,
                "EUR",
                Pay::Visa4242,
                "Premium",
            )
        },
        paid(
            ("Strava", "strava.com", En),
            Yearly,
            7999,
            "EUR",
            Pay::Visa4242,
            "Summit",
        ),
        paid(
            ("kicker", "kicker.de", De),
            HalfYearly,
            5990,
            "EUR",
            Pay::Sepa,
            "kicker digital",
        ),
        paid(
            ("Bitdefender", "bitdefender.com", En),
            Yearly,
            2999,
            "EUR",
            Pay::Visa4242,
            "Total Security",
        ),
        paid(
            ("Mullvad", "mullvad.net", En),
            Monthly,
            500,
            "EUR",
            Pay::Visa4242,
            "VPN",
        ),
        paid(
            ("Hetzner", "hetzner.com", De),
            Monthly,
            449,
            "EUR",
            Pay::Sepa,
            "Cloud CX22",
        ),
        paid(
            ("The Guardian", "theguardian.com", En),
            Monthly,
            1200,
            "GBP",
            Pay::Visa4242,
            "Digital",
        ),
        paid(
            ("NZZ", "nzz.ch", De),
            Monthly,
            2900,
            "CHF",
            Pay::Mc5454,
            "Digital",
        ),
        paid(
            ("HelloFresh", "hellofresh.de", De),
            Weekly,
            4999,
            "EUR",
            Pay::Sepa,
            "Classic Box",
        ),
    ];
    v.shrink_to_fit();
    v
}

/// (name, domain, flavour) newsletter-only senders. flavour: 0 one-click, 1 mailto only,
/// 2 link only, 3 one-click + List-Id, 4 no List-Unsubscribe but `Precedence: bulk`.
const NEWS: &[(&str, &str, u8)] = &[
    ("heise online", "heise.de", 0),
    ("Golem.de", "golem.de", 3),
    ("t3n", "t3n.com", 2),
    ("netzpolitik.org", "netzpolitik.org", 1),
    ("Morning Brew", "morningbrew.com", 3),
    ("Lidl", "lidl.de", 0),
    ("ALDI SUED", "aldi-sued.de", 2),
    ("REWE", "rewe.de", 0),
    ("EDEKA", "edeka.de", 1),
    ("Kaufland", "kaufland.de", 0),
    ("Douglas", "douglas.de", 3),
    ("H&M", "hm.com", 0),
    ("Zara", "zara.com", 2),
    ("ASOS", "asos.com", 0),
    ("adidas", "adidas.de", 0),
    ("Nike", "nike.com", 0),
    ("PUMA", "puma.com", 1),
    ("Conrad", "conrad.de", 2),
    ("Cyberport", "cyberport.de", 0),
    ("Thalia", "thalia.de", 3),
    ("Hugendubel", "hugendubel.de", 4),
    ("tagesschau", "tagesschau.de", 4),
];

const SUBSTACK_LISTS: &[(&str, &str)] = &[
    ("Stratechery Lite", "stratecherylite"),
    ("Die Wochenschau", "diewochenschau"),
    ("Pragmatic Notes", "pragmaticnotes"),
];

const SSO_GOOGLE: &[(&str, &str)] = &[
    ("Miro", "miro.com"),
    ("Linear", "linear.app"),
    ("Vercel", "vercel.com"),
    ("Todoist", "todoist.com"),
    ("Canva", "canva.com"),
    ("Loom", "loom.com"),
    ("Airtable", "airtable.com"),
    ("Pitch", "pitch.com"),
    ("Notion", "notion.so"),
];
const SSO_GITHUB: &[(&str, &str)] = &[
    ("CircleCI", "circleci.com"),
    ("Netlify", "netlify.com"),
    ("Codecov", "codecov.io"),
    ("Sentry", "sentry.io"),
];
const SSO_MS: &[(&str, &str)] = &[
    ("Evernote", "evernote.com"),
    ("Smartsheet", "smartsheet.com"),
    ("Mural", "mural.co"),
];
const HARD: &[Svc] = &[
    ("Pocket Casts", "pocketcasts.com", En),
    ("Readwise", "readwise.io", En),
    ("Raindrop.io", "raindrop.io", En),
    ("Obsidian", "obsidian.md", En),
    ("Plex", "plex.tv", En),
    ("Trakt", "trakt.tv", En),
    ("Last.fm", "last.fm", En),
    ("Bandcamp", "bandcamp.com", En),
    ("SoundCloud", "soundcloud.com", En),
    ("Vimeo", "vimeo.com", En),
    ("Flickr", "flickr.com", En),
    ("Imgur", "imgur.com", En),
    ("CodePen", "codepen.io", En),
    ("Replit", "replit.com", En),
    ("Kaggle", "kaggle.com", En),
    ("LeetCode", "leetcode.com", En),
    ("Scalable Capital", "scalable.capital", De),
    ("Finanzguru", "finanzguru.de", De),
    ("mydealz", "mydealz.de", De),
    ("idealo", "idealo.de", De),
    ("FitX", "fitx.de", De),
    ("Parkster", "parkster.de", De),
    ("Qonto", "qonto.com", Fr),
    ("Alan", "alan.com", Fr),
    ("Lydia", "lydia-app.com", Fr),
    ("Ulule", "ulule.com", Fr),
];
const HARD_EN: &[&str] = &[
    "Your password was changed",
    "You are in!",
    "Let us get started",
    "Confirm it was you",
    "Finish setting up your account",
    "Your account is ready",
    "Thanks for verifying your email",
    "Email address confirmed",
    "Sign in to {n}",
    "Your {n} login link",
    "Welcome back",
    "Action required: verify your account",
    "Hi Erika, here is how to get started",
];
const HARD_DE: &[&str] = &[
    "Dein Passwort wurde geändert",
    "Schön, dass du da bist",
    "Anmeldung bestätigt",
    "Dein neues Konto ist startklar",
    "Aktivierung deines Kontos",
    "Deine Registrierung bei {n}",
    "E-Mail-Adresse erfolgreich bestätigt",
    "Login-Link für {n}",
    "Du hast dich angemeldet",
    "Sicherheitsinfo zu deinem Konto",
];
const HARD_FR: &[&str] = &[
    "Merci d'avoir rejoint {n}",
    "Active ton compte",
    "Votre inscription est confirmée",
    "Bienvenue ! Voici comment commencer",
    "Votre mot de passe a été modifié",
];
const APPLE_RELAY: &[Svc] = &[
    ("Vivino", "vivino.com", En),
    ("Untappd", "untappd.com", En),
    ("Letterboxd", "letterboxd.com", En),
    ("Goodreads", "goodreads.com", En),
    ("Pinterest", "pinterest.com", En),
    ("Yazio", "yazio.com", De),
];
const PAYPAL_MERCHANTS: &[(&str, i64)] = &[
    ("Skillshare", 1900),
    ("Patreon", 500),
    ("Scribd", 1199),
    ("Calm", 1499),
    ("Surfshark", 299),
];
/// Apple in-app subscriptions: (app, plan, monthly amount minor).
const APPLE_APPS: &[(&str, &str, i64)] = &[
    ("Day One", "Day One Premium (1 Monat)", 349),
    ("Fantastical", "Fantastical Premium (1 Monat)", 499),
    ("Bear", "Bear Pro (1 Monat)", 149),
];
const GUEST_SHOPS: &[&str] = &[
    "blumenversand24.de",
    "ticketshop-nord.de",
    "bastelparadies.de",
    "tierbedarf-direkt.de",
    "weindepot-sued.de",
    "gartenfreunde-shop.de",
    "fahrradteile24.de",
    "buecherecke-online.de",
];
const ESP_BRANDS: &[(&str, &str, &str)] = &[
    ("Kleinbrauerei Nord", "kleinbrauerei-nord.de", "mcsv.net"),
    ("Yogastudio Lotus", "yogastudio-lotus.de", "sendgrid.net"),
    (
        "Buchladen am Markt",
        "buchladen-am-markt.de",
        "amazonses.com",
    ),
    ("Radsport Becker", "radsport-becker.de", "mailgun.org"),
];
const FREEMAIL_FRIENDS: &[&str] = &[
    "lena.koch@gmail.com",
    "tobias.wagner@gmx.de",
    "mama.beispiel@web.de",
    "chris.meyer@outlook.com",
    "sabine.fischer@t-online.de",
    "jonas@icloud.com",
    "anna.schmidt@googlemail.com",
    "peter.richter@yahoo.de",
    "info@posteo.de",
    "dieter@freenet.de",
];

// ------------------------------------------------------------------------------------------
// Encoding helpers
// ------------------------------------------------------------------------------------------

pub fn rfc2822(dt: DateTime<Utc>, offset_minutes: i32) -> String {
    let off = FixedOffset::east_opt(offset_minutes * 60).unwrap();
    dt.with_timezone(&off)
        .format("%a, %d %b %Y %H:%M:%S %z")
        .to_string()
}

fn internal_date(dt: DateTime<Utc>) -> String {
    dt.format("%d-%b-%Y %H:%M:%S +0000").to_string()
}

fn q_encode(bytes: &[u8]) -> String {
    let mut s = String::new();
    for &b in bytes {
        match b {
            b' ' => s.push('_'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => s.push(b as char),
            _ => s.push_str(&format!("={b:02X}")),
        }
    }
    s
}

fn b64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = match chunk.len() {
            3 => (chunk[0] as u32) << 16 | (chunk[1] as u32) << 8 | chunk[2] as u32,
            2 => (chunk[0] as u32) << 16 | (chunk[1] as u32) << 8,
            _ => (chunk[0] as u32) << 16,
        };
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Encode a header text in one of several valid ways (all must decode back to `s`).
pub fn encode_text(rng: &mut Rng, s: &str) -> Vec<u8> {
    let ascii = s.is_ascii();
    match rng.below(10) {
        0..=3 if ascii => s.as_bytes().to_vec(),
        // raw 8-bit UTF-8 (not RFC-conform, but very common in the wild)
        0..=2 => s.as_bytes().to_vec(),
        4 | 5 => format!("=?UTF-8?B?{}?=", b64(s.as_bytes())).into_bytes(),
        6 | 7 => format!("=?utf-8?q?{}?=", q_encode(s.as_bytes())).into_bytes(),
        8 => {
            // Latin-1 if representable, else UTF-8 Q.
            if s.chars().all(|c| (c as u32) < 256) {
                let bytes: Vec<u8> = s.chars().map(|c| c as u32 as u8).collect();
                format!("=?ISO-8859-1?Q?{}?=", q_encode(&bytes)).into_bytes()
            } else {
                format!("=?UTF-8?Q?{}?=", q_encode(s.as_bytes())).into_bytes()
            }
        }
        _ => {
            // Folded: several encoded words on continuation lines, split at char boundaries.
            let chars: Vec<char> = s.chars().collect();
            let mut out = String::new();
            for (i, part) in chars.chunks(12).enumerate() {
                let t: String = part.iter().collect();
                if i > 0 {
                    out.push_str("\r\n ");
                }
                out.push_str(&format!("=?UTF-8?B?{}?=", b64(t.as_bytes())));
            }
            out.into_bytes()
        }
    }
}

pub fn fmt_amount(minor: i64, cur: &str, lang: Lang) -> String {
    let (whole, frac) = (minor.abs() / 100, minor.abs() % 100);
    let sign = if minor < 0 { "-" } else { "" };
    match (cur, lang) {
        ("EUR", En) => format!("{sign}€{whole}.{frac:02}"),
        ("EUR", _) => format!("{sign}{whole},{frac:02} €"),
        ("USD", _) => format!("{sign}${whole}.{frac:02}"),
        ("GBP", _) => format!("{sign}£{whole}.{frac:02}"),
        ("CHF", _) => format!("CHF {sign}{whole}.{frac:02}"),
        _ => format!("{sign}{whole}.{frac:02} {cur}"),
    }
}

fn slug_of(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

pub fn canon_key(domain: &str) -> String {
    registrable_domain(domain)
        .map(|d| canonical_service_domain(&d))
        .unwrap_or_else(|| domain.to_string())
}

// ------------------------------------------------------------------------------------------
// Mail spec / builder
// ------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct Spec {
    date: Option<DateTime<Utc>>,
    arrival: DateTime<Utc>,
    tz: i32,
    from: Vec<u8>,
    to: String,
    subject: Vec<u8>,
    msgid: Option<String>,
    extra: Vec<(String, Vec<u8>)>,
    body: Option<String>,
    body_headers: Option<String>,
}

struct Gen {
    rng: Rng,
    specs: Vec<Spec>,
    truth: Truth,
    seq: u64,
    hosts: BTreeMap<String, String>,
}

fn date_between(rng: &mut Rng, a: DateTime<Utc>, b: DateTime<Utc>) -> DateTime<Utc> {
    let span = (b - a).num_seconds().max(1);
    a + Duration::seconds(rng.below(span as u64) as i64)
}

impl Gen {
    fn host_for(&mut self, domain: &str) -> String {
        if let Some(h) = self.hosts.get(domain) {
            return h.clone();
        }
        let prefix = *self
            .rng
            .pick(&["", "", "mail.", "email.", "accounts.", "notify.", "m."]);
        let h = format!("{prefix}{domain}");
        self.hosts.insert(domain.to_string(), h.clone());
        h
    }

    fn sender_hdr(&mut self, name: &str, local: &str, host: &str) -> Vec<u8> {
        let disp = match self.rng.below(5) {
            0 => format!("\"{name}\""),
            1 => name.to_string(),
            _ => format!("\"{name}\""),
        };
        let name_enc = if name.is_ascii() {
            disp.into_bytes()
        } else {
            encode_text(&mut self.rng, name)
        };
        let mut v = name_enc;
        v.extend_from_slice(format!(" <{local}@{host}>").as_bytes());
        v
    }

    fn msgid(&mut self, host: &str) -> String {
        self.seq += 1;
        let r = self.rng.next_u64() % 1_000_000;
        format!("<{}.{r}@{host}>", self.seq)
    }

    fn push(&mut self, mut s: Spec) {
        s.tz = *self.rng.pick(&[0, 60, 120, 120, -300, 330, -480, 540]);
        if s.arrival == DateTime::<Utc>::default() {
            s.arrival = s.date.unwrap_or_else(now);
        }
        self.specs.push(s);
    }

    fn simple(
        &mut self,
        date: DateTime<Utc>,
        from: Vec<u8>,
        host: &str,
        to: &str,
        subject: &str,
    ) -> Spec {
        let subj = encode_text(&mut self.rng, subject);
        Spec {
            date: Some(date),
            arrival: date + Duration::seconds(self.rng.range(1, 90)),
            from,
            to: to.to_string(),
            subject: subj,
            msgid: Some(self.msgid(host)),
            ..Default::default()
        }
    }

    // ---- account lifecycle mails ---------------------------------------------------------

    fn account_mails(&mut self, name: &str, domain: &str, lang: Lang, to: &str, relay: bool) {
        let host = self.host_for(domain);
        let local = *self
            .rng
            .pick(&["noreply", "no-reply", "hello", "service", "support"]);
        let start = Utc.with_ymd_and_hms(2021, 6, 1, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2025, 10, 1, 0, 0, 0).unwrap();
        let d0 = date_between(&mut self.rng, start, end);
        let n = name;
        let (welcome, verify, reset, login, code) = match lang {
            De => (
                format!("Willkommen bei {n}!"),
                "Bitte bestätige deine E-Mail-Adresse".to_string(),
                "Passwort zurücksetzen".to_string(),
                format!("Neue Anmeldung bei deinem {n}-Konto"),
                "Dein Bestätigungscode: 482913".to_string(),
            ),
            En => (
                format!("Welcome to {n}!"),
                "Please verify your email address".to_string(),
                "Reset your password".to_string(),
                format!("New sign-in to your {n} account"),
                "Your verification code is 482913".to_string(),
            ),
            Fr => (
                format!("Bienvenue chez {n}"),
                "Confirmez votre adresse e-mail".to_string(),
                "Réinitialisation du mot de passe".to_string(),
                format!("Nouvelle connexion à votre compte {n}"),
                "Votre code de vérification : 482913".to_string(),
            ),
        };
        let from = self.sender_hdr(n, local, &host);
        // Verification first (a minute before the welcome mail), then welcome.
        let s = self.simple(d0 - Duration::minutes(1), from.clone(), &host, to, &verify);
        self.push(s);
        let s = self.simple(d0, from.clone(), &host, to, &welcome);
        self.push(s);
        for _ in 0..self.rng.range(0, 3) {
            let d = date_between(&mut self.rng, d0, now());
            let s = self.simple(d, from.clone(), &host, to, &login);
            self.push(s);
        }
        if self.rng.chance(0.5) {
            let d = date_between(&mut self.rng, d0, now());
            let s = self.simple(d, from.clone(), &host, to, &reset);
            self.push(s);
        }
        if self.rng.chance(0.4) {
            for _ in 0..self.rng.range(1, 4) {
                let d = date_between(&mut self.rng, d0, now());
                let s = self.simple(d, from.clone(), &host, to, &code);
                self.push(s);
            }
        }
        self.truth.accounts.push(ExpectedAccount {
            name: name.to_string(),
            key: canon_key(domain),
            slug: slug_of(name),
            sso: relay.then_some(SsoProvider::Apple),
        });
    }

    // ---- paid subscriptions -----------------------------------------------------------------

    fn charge_dates(&mut self, interval: BillingInterval, n: usize) -> Vec<DateTime<Utc>> {
        let step_days = match interval {
            Weekly => 7,
            BiWeekly => 14,
            Monthly => 30,
            BiMonthly => 61,
            Quarterly => 91,
            FourMonthly => 122,
            HalfYearly => 182,
            Yearly => 365,
        };
        let last = now() - Duration::days(self.rng.range(3, (step_days / 2).max(4)));
        let mut out = Vec::new();
        for k in 0..n {
            let d = match interval {
                Weekly => last - Duration::days(7 * k as i64),
                BiWeekly => last - Duration::days(14 * k as i64),
                Monthly => last - Months::new(k as u32),
                BiMonthly => last - Months::new(2 * k as u32),
                Quarterly => last - Months::new(3 * k as u32),
                FourMonthly => last - Months::new(4 * k as u32),
                HalfYearly => last - Months::new(6 * k as u32),
                Yearly => last - Months::new(12 * k as u32),
            };
            let jitter = if interval == Weekly {
                0
            } else {
                self.rng.range(0, 1)
            };
            out.push(d + Duration::days(jitter));
        }
        out.reverse();
        out
    }

    fn pay_body(&self, p: &Paid, amount: i64, lang: Lang) -> String {
        let amt = fmt_amount(amount, p.cur, lang);
        let method = match p.pay {
            Pay::Visa4242 => "Bezahlt mit Visa ending in 4242",
            Pay::Mc5454 => "Paid with Mastercard ending in 5454",
            Pay::Sepa => "Zahlungsart: Lastschrift\nIBAN: DE89 3704 0044 0532 0130 00",
        };
        format!(
            "Danke für deine Zahlung!\n\nPlan: {}\nGesamt {amt}\n\n{method}\n",
            p.plan
        )
    }

    fn paid_service(&mut self, p: &Paid) {
        let (name, domain, lang) = p.svc;
        let host = self.host_for(domain);
        let from = self.sender_hdr(name, "billing", &host);
        let n = match p.interval {
            Weekly | BiWeekly => 14,
            Monthly => self.rng.range(8, 26) as usize,
            BiMonthly | Quarterly | FourMonthly => self.rng.range(4, 8) as usize,
            HalfYearly => 4,
            Yearly => self.rng.range(2, 4) as usize,
        };
        let dates = self.charge_dates(p.interval, n);
        let change_at = p.new_price.map(|_| dates.len().saturating_sub(1));
        let mut amount_now = p.amount;
        for (i, d) in dates.iter().enumerate() {
            if let (Some(c), Some(np)) = (change_at, p.new_price) {
                if i == c {
                    // Price-change notice a few days before the first higher charge.
                    let (subj, body) = match lang {
                        En | Fr => (
                            "Important: price change to your subscription".to_string(),
                            format!(
                                "Your plan price is changing.\nNew price: {}/month\nCurrent price: {}/month\n",
                                fmt_amount(np, p.cur, En),
                                fmt_amount(p.amount, p.cur, En)
                            ),
                        ),
                        De => (
                            format!("Preisanpassung für dein {name}-Abo"),
                            format!(
                                "Ab dem 01.05.2025 kostet dein Abo {} pro Monat (bisher {}).\n",
                                fmt_amount(np, p.cur, De),
                                fmt_amount(p.amount, p.cur, De)
                            ),
                        ),
                    };
                    let mut s =
                        self.simple(*d - Duration::days(10), from.clone(), &host, OWN, &subj);
                    s.body = Some(body);
                    self.push(s);
                    amount_now = np;
                }
            }
            let amt = fmt_amount(amount_now, p.cur, lang);
            let subj = if i == 0 {
                match lang {
                    De => format!("Deine Rechnung von {name}: {amt}"),
                    En => format!("Your receipt from {name} – {amt}"),
                    Fr => format!("Votre facture {name} : {amt}"),
                }
            } else {
                match lang {
                    De => format!("Dein {name}-Abo wurde verlängert: {amt}"),
                    En => format!("Your {name} subscription was renewed ({amt})"),
                    Fr => format!("Votre abonnement {name} a été renouvelé : {amt}"),
                }
            };
            let body = self.pay_body(p, amount_now, lang);
            let mut s = self.simple(*d, from.clone(), &host, OWN, &subj);
            s.body = Some(body);
            self.push(s);
            if p.trial && i == 0 {
                let subj = match lang {
                    En | Fr => format!("Your free trial of {name} has started"),
                    De => format!("Deine Testphase bei {name} hat begonnen"),
                };
                let s = self.simple(*d - Duration::days(14), from.clone(), &host, OWN, &subj);
                self.push(s);
            }
        }
        let last = *dates.last().unwrap();
        if p.refund {
            let subj = match lang {
                De => format!("Erstattung über {}", fmt_amount(amount_now, p.cur, De)),
                _ => format!("Refund of {}", fmt_amount(amount_now, p.cur, En)),
            };
            let mut s = self.simple(
                *dates.first().unwrap() + Duration::days(3),
                from.clone(),
                &host,
                OWN,
                &subj,
            );
            s.body = Some(format!(
                "Erstattungsbetrag: {}\n",
                fmt_amount(amount_now, p.cur, De)
            ));
            self.push(s);
        }
        if p.cancelled {
            let subj = match lang {
                De => format!("Kündigungsbestätigung für dein {name}-Abo"),
                _ => "Your cancellation is confirmed".to_string(),
            };
            let s = self.simple(last + Duration::days(2), from.clone(), &host, OWN, &subj);
            self.push(s);
        }
        // Account lifecycle for ~2/3 of the paid services (rest: account via payments only).
        if self.rng.chance(0.66) {
            self.account_mails(name, domain, lang, OWN, false);
            self.truth.accounts.pop(); // re-added below with billing info
        }
        let key = canon_key(domain);
        self.truth.accounts.push(ExpectedAccount {
            name: name.to_string(),
            key: key.clone(),
            slug: slug_of(name),
            sso: None,
        });
        let method = match p.pay {
            Pay::Visa4242 => PaymentMethod::Card {
                brand: "Visa".into(),
                last4: "4242".into(),
            },
            Pay::Mc5454 => PaymentMethod::Card {
                brand: "Mastercard".into(),
                last4: "5454".into(),
            },
            Pay::Sepa => PaymentMethod::Sepa {
                iban_last4: Some("3000".into()),
            },
        };
        self.truth
            .method_index
            .entry(method.label_de())
            .or_default()
            .insert(key.clone());
        self.truth.billing.push(ExpectedBilling {
            key,
            name: name.to_string(),
            interval: Some(p.interval),
            amount_minor: amount_now,
            currency: p.cur.to_string(),
            active: !p.cancelled,
            price_increased: p.new_price.is_some(),
            method: Some(method),
            via: None,
            bodies_needed: false,
        });
    }

    // ---- newsletters ---------------------------------------------------------------------

    fn newsletter(&mut self, name: &str, domain: &str, flavour: u8, list_id: Option<String>) {
        let host = self.host_for(domain);
        let local = *self
            .rng
            .pick(&["newsletter", "news", "hello", "angebote", "info"]);
        let from_addr = format!("{local}@{host}");
        let from = self.sender_hdr(name, local, &host);
        let count = self.rng.range(18, 70) as usize;
        let start = Utc.with_ymd_and_hms(2023, 1, 5, 8, 0, 0).unwrap();
        let end = now() - Duration::days(2);
        let step = (end - start).num_seconds() / count as i64;
        let promo = [
            "20% Rabatt auf alles",
            "Black Friday Deals: bis zu 50% sparen",
            "Nur heute: Gutschein für dich",
            "Summer SALE starts now",
            "Save 30% today",
            "Neuheiten der Kollektion",
            "Jetzt sichern: Angebot der Woche",
            "Soldes d'été: -40%",
        ];
        let content = [
            "Newsletter KW 12: Die Themen der Woche",
            "This week in review",
            "Das Wichtigste vom Tag",
            "Weekly digest: what you missed",
            "Neuigkeiten aus der Redaktion",
        ];
        let mut one_click = false;
        let mut any = false;
        for i in 0..count {
            let d = start + Duration::seconds(step * i as i64 + self.rng.range(0, 3600));
            let subj = if self.rng.chance(0.6) {
                *self.rng.pick(&promo)
            } else {
                *self.rng.pick(&content)
            };
            let subj = format!("{subj} #{i}");
            let mut s = self.simple(d, from.clone(), &host, OWN, &subj);
            let url = format!(
                "https://{host}/unsubscribe?u={}&l={i}",
                self.rng.next_u64() % 99999
            );
            let mailto = format!("mailto:unsubscribe@{host}?subject=unsubscribe");
            match flavour {
                0 | 3 => {
                    s.extra.push((
                        "List-Unsubscribe".into(),
                        format!("<{url}>, <{mailto}>").into_bytes(),
                    ));
                    s.extra.push((
                        "List-Unsubscribe-Post".into(),
                        b"List-Unsubscribe=One-Click".to_vec(),
                    ));
                    one_click = true;
                    any = true;
                }
                1 => {
                    s.extra.push((
                        "List-Unsubscribe".into(),
                        format!("<{mailto}>").into_bytes(),
                    ));
                    any = true;
                }
                2 => {
                    s.extra
                        .push(("List-Unsubscribe".into(), format!("<{url}>").into_bytes()));
                    any = true;
                }
                _ => {
                    s.extra.push(("Precedence".into(), b"bulk".to_vec()));
                }
            }
            if let Some(l) = &list_id {
                s.extra
                    .push(("List-Id".into(), format!("{name} <{l}>").into_bytes()));
            }
            self.push(s);
        }
        // A few exact duplicates (same Message-ID), as Gmail "All Mail" produces.
        let dups = self.rng.range(0, 2) as usize;
        for _ in 0..dups {
            let idx = self.specs.len() - 1 - self.rng.below(count as u64) as usize;
            let mut dup = self.specs[idx].clone();
            dup.arrival += Duration::seconds(5);
            self.specs.push(dup);
            self.truth.duplicate_mails += 1;
        }
        let list_key = list_id.unwrap_or(from_addr);
        self.truth.lists.push(ExpectedList {
            list_key: list_key.to_lowercase(),
            domain: host.clone(),
            count: count as u64,
            one_click,
            any_unsub: any,
        });
        self.truth.negatives.insert(canon_key(domain));
    }

    // ---- aggregator receipts ---------------------------------------------------------------

    fn paypal_merchant(&mut self, merchant: &str, amount: i64) {
        let host = "paypal.de".to_string();
        let from = b"PayPal <service@paypal.de>".to_vec();
        let n = self.rng.range(6, 14) as usize;
        let dates = self.charge_dates(Monthly, n);
        for d in &dates {
            let subj = format!("Sie haben eine automatische Zahlung an {merchant} gesendet");
            let mut s = self.simple(*d, from.clone(), &host, OWN, &subj);
            s.body = Some(format!(
                "Sie haben eine automatische Zahlung an {merchant} Inc. gesendet.\nBetrag: {}\nZahlungsart: PayPal-Guthaben\n",
                fmt_amount(amount, "EUR", De)
            ));
            self.push(s);
        }
        let key = format!("unresolved:{}", slug_of(merchant));
        self.truth.accounts.push(ExpectedAccount {
            name: merchant.to_string(),
            key: key.clone(),
            slug: slug_of(merchant),
            sso: None,
        });
        self.truth.billing.push(ExpectedBilling {
            key: key.clone(),
            name: merchant.to_string(),
            interval: Some(Monthly),
            amount_minor: amount,
            currency: "EUR".into(),
            active: true,
            price_increased: false,
            method: Some(PaymentMethod::PayPal),
            via: Some("paypal.com".into()),
            bodies_needed: true,
        });
        self.truth
            .method_index
            .entry(PaymentMethod::PayPal.label_de())
            .or_default()
            .insert(key);
    }

    fn apple_receipts(&mut self) {
        let from = b"Apple <no_reply@email.apple.com>".to_vec();
        let host = "email.apple.com".to_string();
        let dates = self.charge_dates(Monthly, 14);
        for d in &dates {
            let mut body = String::from(
                "Deine Rechnung von Apple.\nApple-Account\terika.beispiel@gmx.de\nABGERECHNET AN\tVisa •••• 1234\nDATUM\t12.03.2025\nBESTELLNUMMER\tMQX1234567\nDOKUMENTNR.\t123456789012\n",
            );
            let mut sum = 0;
            for (app, plan, amt) in APPLE_APPS {
                body.push_str(&format!(
                    "{app}\n{plan}\nVerlängert sich am 12.04.2025\nProblem melden\n{}\n",
                    fmt_amount(*amt, "EUR", De)
                ));
                sum += amt;
            }
            body.push_str(&format!("SUMME\t{}\n", fmt_amount(sum, "EUR", De)));
            let mut s = self.simple(*d, from.clone(), &host, OWN, "Deine Rechnung von Apple.");
            s.body = Some(body);
            self.push(s);
        }
        for (app, _, amt) in APPLE_APPS {
            let key = format!("unresolved:{}", slug_of(app));
            self.truth.accounts.push(ExpectedAccount {
                name: (*app).to_string(),
                key: key.clone(),
                slug: slug_of(app),
                sso: None,
            });
            self.truth.billing.push(ExpectedBilling {
                key: key.clone(),
                name: (*app).to_string(),
                interval: Some(Monthly),
                amount_minor: *amt,
                currency: "EUR".into(),
                active: true,
                price_increased: false,
                method: Some(PaymentMethod::Card {
                    brand: "Visa".into(),
                    last4: "1234".into(),
                }),
                via: Some("apple.com".into()),
                bodies_needed: true,
            });
            self.truth
                .method_index
                .entry("Visa •••• 1234".to_string())
                .or_default()
                .insert(key);
        }
    }

    // ---- SSO grants ----------------------------------------------------------------------

    fn sso_grants(&mut self) {
        let g_from = b"Google <no-reply@accounts.google.com>".to_vec();
        for (i, (app, dom)) in SSO_GOOGLE.iter().enumerate() {
            let d = date_between(
                &mut self.rng,
                Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                now(),
            );
            let (subj, body) = if i % 2 == 0 {
                (
                    "Sicherheitswarnung",
                    format!("Sicherheitswarnung\r\n\r\n{app} hat Zugriff auf Ihr Google-Konto erhalten\r\nhttps://www.{dom}/\r\n"),
                )
            } else {
                (
                    "Security alert",
                    format!("Security alert\r\n\r\n{app} was granted access to your Google Account\r\nhttps://{dom}/\r\n"),
                )
            };
            let mut s = self.simple(d, g_from.clone(), "accounts.google.com", OWN, subj);
            s.body = Some(body);
            self.push(s);
            self.truth.accounts.push(ExpectedAccount {
                name: (*app).to_string(),
                key: canon_key(dom),
                slug: slug_of(app),
                sso: Some(SsoProvider::Google),
            });
        }
        // Google's own account: plain sign-in alert (no app grant).
        for _ in 0..3 {
            let d = date_between(
                &mut self.rng,
                Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
                now(),
            );
            let mut s = self.simple(
                d,
                g_from.clone(),
                "accounts.google.com",
                OWN,
                "Security alert",
            );
            s.body = Some(
                "New sign-in on Windows\r\nIf this was you, you don't need to do anything.\r\n"
                    .into(),
            );
            self.push(s);
        }
        self.truth.accounts.push(ExpectedAccount {
            name: "Google".into(),
            key: "google.com".into(),
            slug: "google".into(),
            sso: None,
        });
        for (app, dom) in SSO_GITHUB {
            let d = date_between(
                &mut self.rng,
                Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                now(),
            );
            let mut s = self.simple(
                d,
                b"GitHub <noreply@github.com>".to_vec(),
                "github.com",
                OWN,
                "[GitHub] A third-party OAuth application has been added to your account",
            );
            s.body = Some(format!(
                "A third-party OAuth application ({app}) with read:user scope was recently authorized to access your account.\nhttps://{dom}\n"
            ));
            self.push(s);
            self.truth.accounts.push(ExpectedAccount {
                name: (*app).to_string(),
                key: canon_key(dom),
                slug: slug_of(app),
                sso: Some(SsoProvider::GitHub),
            });
        }
        for (app, dom) in SSO_MS {
            let d = date_between(
                &mut self.rng,
                Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                now(),
            );
            let mut s = self.simple(
                d,
                b"Microsoft account team <account-security-noreply@accountprotection.microsoft.com>".to_vec(),
                "accountprotection.microsoft.com",
                OWN,
                "App connected to your Microsoft account",
            );
            s.body = Some(format!("Application: {app}\nhttps://{dom}\n"));
            self.push(s);
            self.truth.accounts.push(ExpectedAccount {
                name: (*app).to_string(),
                key: canon_key(dom),
                slug: slug_of(app),
                sso: Some(SsoProvider::Microsoft),
            });
        }
        // Microsoft sign-in alert + Apple sign-in alert for the providers themselves.
        let mut s = self.simple(
            Utc.with_ymd_and_hms(2025, 5, 5, 9, 0, 0).unwrap(),
            b"Microsoft account team <account-security-noreply@accountprotection.microsoft.com>"
                .to_vec(),
            "accountprotection.microsoft.com",
            OWN,
            "New sign-in to your Microsoft account",
        );
        s.body = None;
        self.push(s);
        self.truth.accounts.push(ExpectedAccount {
            name: "Microsoft".into(),
            key: "microsoft.com".into(),
            slug: "microsoft".into(),
            sso: None,
        });
        let s = self.simple(
            Utc.with_ymd_and_hms(2025, 6, 6, 9, 0, 0).unwrap(),
            b"Apple <noreply@email.apple.com>".to_vec(),
            "email.apple.com",
            OWN,
            "Your Apple ID was used to sign in on a new device",
        );
        self.push(s);
        self.truth.accounts.push(ExpectedAccount {
            name: "Apple".into(),
            key: "apple.com".into(),
            slug: "apple".into(),
            sso: None,
        });
    }

    // ---- noise: personal, spam, ESP, own mail --------------------------------------------------

    /// Services whose mails use wording that is not (obviously) covered by keyword rules.
    fn hard_services(&mut self) {
        for (name, domain, lang) in HARD {
            let pool: &[&str] = match lang {
                En => HARD_EN,
                De => HARD_DE,
                Fr => HARD_FR,
            };
            let host = self.host_for(domain);
            let from = self.sender_hdr(name, "noreply", &host);
            let d0 = date_between(
                &mut self.rng,
                Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
                now() - Duration::days(30),
            );
            for i in 0..3 {
                let subj = self.rng.pick(pool).replace("{n}", name);
                let s = self.simple(d0 + Duration::days(i * 40), from.clone(), &host, OWN, &subj);
                self.push(s);
            }
            self.truth.accounts.push(ExpectedAccount {
                name: (*name).to_string(),
                key: canon_key(domain),
                slug: slug_of(name),
                sso: None,
            });
            self.truth.hard.insert(canon_key(domain));
        }
    }

    fn noise(&mut self) {
        let start = Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap();
        let personal = [
            "Re: Wochenende?",
            "Willkommen in der Familie!",
            "Rechnung Handwerker",
            "Neue Anmeldung Sportverein",
            "Fwd: Reset your password (komisch)",
            "Welcome home! 🏡",
            "Bestellung Pizza heute Abend?",
            "Passwort für das WLAN",
            "Verification code von Papa: 1234",
            "Security alert: Oma hat ihr Handy verloren",
            "Bienvenue à Paris",
            "Urlaubsfotos",
        ];
        for i in 0..160 {
            let f = *self.rng.pick(FREEMAIL_FRIENDS);
            let (user, host) = f.split_once('@').unwrap();
            let subj = *self.rng.pick(&personal);
            let d = date_between(&mut self.rng, start, now());
            let from = format!("{} <{f}>", user.replace('.', " ")).into_bytes();
            let mut s = self.simple(d, from, host, OWN, subj);
            s.msgid = (i % 11 != 0).then(|| s.msgid.clone().unwrap());
            self.push(s);
        }
        // Mails sent by the user themselves.
        for subj in [
            "Willkommen",
            "Passwort zurücksetzen",
            "Rechnung 2025",
            "Re: Angebot",
        ] {
            let d = date_between(&mut self.rng, start, now());
            let s = self.simple(
                d,
                format!("Erika <{OWN}>").into_bytes(),
                "gmx.de",
                "chef@firma-xyz.de",
                subj,
            );
            self.push(s);
        }
        // Spam without bulk headers from throwaway domains.
        let spam = [
            "Du hast gewonnen!!! 1.000.000 €",
            "Make money fast with Bitcoin",
            "Viagra online günstig",
            "Dringende Antwort erforderlich",
            "Singles in deiner Nähe",
            "Casino Bonus 500%",
            "Erbschaft: Sie wurden ausgewählt",
        ];
        for i in 0..70 {
            let dom = format!("promo-{}-{}.top", self.rng.below(900), i);
            let local = *self
                .rng
                .pick(&["promo", "offer", "winner", "j.smith", "kontakt"]);
            let d = date_between(&mut self.rng, start, now());
            let subj = *self.rng.pick(&spam);
            let from = format!("{local} <{local}@{dom}>").into_bytes();
            let mut s = self.simple(d, from, &dom, OWN, subj);
            if i % 4 == 0 {
                s.extra.push(("Precedence".into(), b"junk".to_vec()));
            }
            if i % 5 == 0 {
                s.extra.push((
                    "List-Unsubscribe".into(),
                    format!("<mailto:stop@{dom}>").into_bytes(),
                ));
            }
            self.push(s);
            self.truth.negatives.insert(canon_key(&dom));
        }
        // Phishing: login-alert wording from look-alike domains (known limitation).
        for dom in [
            "paypa1-secure.top",
            "amaz0n-konto-pruefung.xyz",
            "apple-id-locked.top",
        ] {
            let d = date_between(&mut self.rng, start, now());
            let s = self.simple(
                d,
                format!("Account Security <security@{dom}>").into_bytes(),
                dom,
                OWN,
                "Security alert: unusual sign-in activity detected",
            );
            self.push(s);
            self.truth.phishing.insert(canon_key(dom));
        }
        // ESP-sent newsletters: From is the brand, Return-Path / Sender is the ESP.
        for (name, dom, esp) in ESP_BRANDS {
            let count = self.rng.range(8, 20);
            let list_id = format!("{}.{esp}", slug_of(name));
            for i in 0..count {
                let d = date_between(&mut self.rng, start, now());
                let mut s = self.simple(
                    d,
                    format!("{name} <news@{dom}>").into_bytes(),
                    dom,
                    OWN,
                    &format!("Neuigkeiten aus dem Laden #{i}"),
                );
                s.extra.push((
                    "Sender".into(),
                    format!("bounce-{i}@mail{}.{esp}", self.rng.below(99)).into_bytes(),
                ));
                s.extra.push((
                    "Return-Path".into(),
                    format!("<bounce-{i}@mail1.{esp}>").into_bytes(),
                ));
                s.extra
                    .push(("List-Id".into(), format!("<{list_id}>").into_bytes()));
                s.extra.push((
                    "List-Unsubscribe".into(),
                    format!("<https://{esp}/u/{i}>").into_bytes(),
                ));
                self.push(s);
            }
            self.truth.lists.push(ExpectedList {
                list_key: list_id,
                domain: (*dom).to_string(),
                count: count as u64,
                one_click: false,
                any_unsub: true,
            });
            self.truth.negatives.insert(canon_key(dom));
            self.truth.negatives.insert(canon_key(esp));
        }
        // Mail with the ESP as the visible From address.
        for esp in [
            "em4711.sendgrid.net",
            "bounce.mailchimp.com",
            "mg.mailgun.org",
        ] {
            let d = date_between(&mut self.rng, start, now());
            let mut s = self.simple(
                d,
                format!("Newsletter <info@{esp}>").into_bytes(),
                esp,
                OWN,
                "Dein wöchentlicher Newsletter",
            );
            s.extra.push((
                "List-Unsubscribe".into(),
                format!("<https://{esp}/u>").into_bytes(),
            ));
            self.push(s);
            self.truth.negatives.insert(canon_key(esp));
        }
        // Guest-shop shipping notices (receipt only, weak): not an account.
        for dom in GUEST_SHOPS {
            let d = date_between(&mut self.rng, start, now());
            let s = self.simple(
                d,
                format!("{dom} <versand@{dom}>").into_bytes(),
                dom,
                OWN,
                "Ihre Sendung ist unterwegs",
            );
            self.push(s);
            let host = (*dom).to_string();
            let d2 = d - Duration::days(1);
            let s = self.simple(
                d2,
                format!("{dom} <bestellung@{dom}>").into_bytes(),
                &host,
                OWN,
                "Bestellbestätigung Nr. 100234",
            );
            self.push(s);
            // An order confirmation is a payment signal: tolerated either way.
            self.truth.tolerated.insert(canon_key(dom));
        }
    }

    // ---- malformed / edge-case headers -------------------------------------------------------------

    fn edge(&mut self) {
        let base = Utc.with_ymd_and_hms(2025, 2, 1, 10, 0, 0).unwrap();
        // 1. Valid-but-unusual encodings of otherwise clear welcome mails -> accounts.
        let robust: &[(&str, &str, &str)] = &[
            (
                "Emoji Shop",
                "emoji-shop.de",
                "🎉 Willkommen bei Emoji Shop 🎉",
            ),
            (
                "Café Müller",
                "cafe-mueller.de",
                "Willkommen im Café Müller – schön, dass du da bist",
            ),
            (
                "RTL Dienst",
                "rtl-dienst.com",
                "مرحبا بك – Welcome to RTL Dienst",
            ),
            (
                "Tokyo Mart",
                "tokyo-mart.jp",
                "ようこそ – Welcome to Tokyo Mart",
            ),
            (
                "Zürich Bank",
                "zuerich-bank.ch",
                "Willkommen bei der Zürcher Kantonalbank",
            ),
        ];
        for (name, dom, subj) in robust {
            for enc in 0..6 {
                let d = base + Duration::days(enc as i64);
                let subject: Vec<u8> = match enc {
                    0 => subj.as_bytes().to_vec(),
                    1 => format!("=?UTF-8?B?{}?=", b64(subj.as_bytes())).into_bytes(),
                    2 => format!("=?UTF-8?Q?{}?=", q_encode(subj.as_bytes())).into_bytes(),
                    3 => format!("=?utf-8?b?{}?=", b64(subj.as_bytes())).into_bytes(),
                    4 => {
                        // folded across lines, split on char boundaries
                        let chars: Vec<char> = subj.chars().collect();
                        let mut out = String::new();
                        for (i, part) in chars.chunks(7).enumerate() {
                            let t: String = part.iter().collect();
                            if i > 0 {
                                out.push_str("\r\n\t");
                            }
                            out.push_str(&format!("=?UTF-8?B?{}?=", b64(t.as_bytes())));
                        }
                        out.into_bytes()
                    }
                    _ => {
                        // plain, folded at a space
                        let (a, b) =
                            subj.split_at(subj.char_indices().nth(9).map(|x| x.0).unwrap_or(0));
                        format!("{a}\r\n {b}").into_bytes()
                    }
                };
                let mut s = self.simple(
                    d,
                    format!("{name} <hello@{dom}>").into_bytes(),
                    dom,
                    OWN,
                    "x",
                );
                s.subject = subject;
                self.push(s);
            }
            self.truth.accounts.push(ExpectedAccount {
                name: (*name).to_string(),
                key: canon_key(dom),
                slug: slug_of(name),
                sso: None,
            });
        }
        // 2. Missing Date header: INTERNALDATE must be used.
        let mut s = self.simple(
            base,
            b"NoDate GmbH <hello@nodate-gmbh.de>".to_vec(),
            "nodate-gmbh.de",
            OWN,
            "Willkommen bei NoDate",
        );
        s.date = None;
        s.arrival = base;
        self.push(s);
        self.truth.accounts.push(ExpectedAccount {
            name: "NoDate GmbH".into(),
            key: canon_key("nodate-gmbh.de"),
            slug: slug_of("NoDate GmbH"),
            sso: None,
        });
        // 3. Latin-1 raw bytes in subject (invalid UTF-8) and in From name.
        let mut s = self.simple(base, Vec::new(), "latin1-shop.de", OWN, "x");
        s.from = b"Gr\xfcn Shop <hello@latin1-shop.de>".to_vec();
        s.subject = b"Willkommen bei Gr\xfcn Shop, sch\xf6n dass Sie da sind".to_vec();
        self.push(s);
        self.truth.accounts.push(ExpectedAccount {
            name: "Grün Shop".into(),
            key: canon_key("latin1-shop.de"),
            slug: "gr-n-shop".into(),
            sso: None,
        });
        // 4. Huge subject (200 KB) on a newsletter.
        let mut s = self.simple(
            base,
            b"Big <news@bigsubject.de>".to_vec(),
            "bigsubject.de",
            OWN,
            "x",
        );
        s.subject = "Rabatt ".repeat(30_000).into_bytes();
        s.extra.push((
            "List-Unsubscribe".into(),
            b"<https://bigsubject.de/u>".to_vec(),
        ));
        self.push(s);
        self.truth.lists.push(ExpectedList {
            list_key: "news@bigsubject.de".into(),
            domain: "bigsubject.de".into(),
            count: 1,
            one_click: false,
            any_unsub: true,
        });
        self.truth.negatives.insert(canon_key("bigsubject.de"));
        // 5. Garbage that only has to survive (no truth): broken encoded words, weird From,
        // empty headers, control characters, NUL bytes, very long header lines.
        self.truth.tolerated.insert("example.org".into());
        let junk_from: Vec<Vec<u8>> = vec![
            b"".to_vec(),
            b"undisclosed-recipients:;".to_vec(),
            b"<>".to_vec(),
            b"foo@bar.com (Kommentar) , other@x.com".to_vec(),
            b"\"Foo, Inc.\" <a@b.example.org>, <c@d.example.org>".to_vec(),
            b"=?UTF-8?B?!!!not-base64?= <x@broken-b64.example.org>".to_vec(),
            b"=?unknown-charset?Q?Willkommen?= <x@unknown-cs.example.org>".to_vec(),
            b"=?UTF-8?Q?unterminated <x@unterminated.example.org>".to_vec(),
            b"X <x@UPPER.EXAMPLE.ORG>".to_vec(),
            b"X <x@xn--mller-kva.example.org>".to_vec(),
            b"X <x@trailing-dot.example.org.>".to_vec(),
            b"X <x@[127.0.0.1]>".to_vec(),
            b"X <x@localhost>".to_vec(),
            b"X <x@192.168.0.1>".to_vec(),
            b"X <@nolocal.example.org>".to_vec(),
            b"X <x@>".to_vec(),
            b"\x00\x01\x02 <nul@nul.example.org>".to_vec(),
            "A".repeat(100_000).into_bytes(),
        ];
        for (i, f) in junk_from.into_iter().enumerate() {
            let mut s = self.simple(
                base + Duration::hours(i as i64),
                f,
                "junk.example.org",
                OWN,
                "Willkommen bei Junk",
            );
            if i % 3 == 0 {
                s.subject =
                    b"=?UTF-8?B?!!!?= Welcome =?UTF-8?Q?=ZZ=?= =?ISO-8859-1?Q?\xe4?=".to_vec();
            }
            if i % 4 == 0 {
                s.msgid = None;
            }
            if i % 5 == 0 {
                s.extra.push((
                    "List-Unsubscribe".into(),
                    b"<<<>>>, <mailto:>, <https://>, , <javascript:alert(1)>, <mailto:a@b@c>"
                        .to_vec(),
                ));
            }
            self.push(s);
        }
        // 6. Structural oddities, injected as complete raw header blocks.
        // (handled in `raw_extras`)
    }

    /// Duplicate Message-IDs of mails that are identical (Gmail labels), plus extra headers.
    fn finalize(mut self) -> Corpus {
        self.specs.sort_by_key(|s| s.arrival);
        let mut mails = Vec::with_capacity(self.specs.len() + 8);
        let mut uid = 100u32;
        let mut ids = BTreeSet::new();
        let mut unique = 0u64;
        for s in &self.specs {
            uid += self.rng.range(1, 3) as u32;
            let mut h: Vec<u8> = Vec::with_capacity(512);
            let mut put = |name: &str, value: &[u8]| {
                h.extend_from_slice(name.as_bytes());
                h.extend_from_slice(b": ");
                h.extend_from_slice(value);
                h.extend_from_slice(b"\r\n");
            };
            put("From", &s.from);
            put("To", s.to.as_bytes());
            put("Subject", &s.subject);
            if let Some(d) = s.date {
                put("Date", rfc2822(d, s.tz).as_bytes());
            }
            if let Some(m) = &s.msgid {
                put("Message-ID", m.as_bytes());
                if ids.insert(m.clone()) {
                    unique += 1;
                }
            } else {
                unique += 1;
            }
            for (k, v) in &s.extra {
                put(k, v);
            }
            h.extend_from_slice(b"\r\n");
            mails.push(RawMail {
                uid,
                header: h,
                body: s.body.as_ref().map(|b| b.clone().into_bytes()),
                body_headers: s
                    .body_headers
                    .clone()
                    .unwrap_or_else(|| "Content-Type: text/plain; charset=utf-8".into()),
                internal_date: internal_date(s.arrival),
            });
        }
        self.truth.unique_mails = unique;
        self.truth.service_count = self.truth.accounts.len()
            + self.truth.lists.len()
            + self.truth.tolerated.len()
            + self.truth.negatives.len();
        Corpus {
            mails,
            truth: self.truth,
        }
    }
}

/// Generate the corpus. `scale` multiplies the number of personal / spam mails only.
pub fn generate(seed: u64) -> Corpus {
    let mut g = Gen {
        rng: Rng::new(seed),
        specs: Vec::new(),
        truth: Truth::default(),
        seq: 0,
        hosts: BTreeMap::new(),
    };
    for (name, domain, lang) in FULL {
        g.account_mails(name, domain, *lang, OWN, false);
    }
    for p in paid_roster() {
        g.paid_service(&p);
    }
    for (name, domain, flavour) in NEWS {
        let list_id = (*flavour == 3).then(|| format!("{}.{domain}", slug_of(name)));
        g.newsletter(name, domain, *flavour, list_id);
    }
    for (name, list) in SUBSTACK_LISTS {
        g.newsletter(
            name,
            "substack.com",
            0,
            Some(format!("{list}.substack.com")),
        );
    }
    for (name, domain, lang) in APPLE_RELAY {
        let relay = format!(
            "{}@privaterelay.appleid.com",
            slug_of(name).replace('-', "") + "x7q2"
        );
        g.account_mails(name, domain, *lang, &relay, true);
    }
    for (m, amt) in PAYPAL_MERCHANTS {
        g.paypal_merchant(m, *amt);
    }
    g.apple_receipts();
    g.hard_services();
    g.sso_grants();
    g.noise();
    g.edge();
    g.finalize()
}

/// Account keys that appear in the truth (positive set), de-duplicated.
pub fn expected_keys(truth: &Truth) -> BTreeSet<String> {
    truth.accounts.iter().map(|a| a.key.clone()).collect()
}

pub fn year_of(d: DateTime<Utc>) -> i32 {
    d.year()
}
