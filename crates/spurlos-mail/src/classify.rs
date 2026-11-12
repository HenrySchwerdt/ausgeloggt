//! Pure, offline mail classification (no network). Heavily unit-tested.
//!
//! All keyword matching runs on a folded form of the text (lowercase, umlauts transliterated,
//! accents stripped), see [`fold`]. Keyword lists below are therefore written in folded form.

use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use regex::Regex;
use spurlos_core::merge::UNRESOLVED_PREFIX;
use spurlos_core::{EvidenceKind, MailCategory, SsoProvider};

/// The header fields we fetch for every mail.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MailHeader {
    /// Raw `From:` value, e.g. `"Spotify" <no-reply@spotify.com>` (may be RFC 2047 encoded).
    pub from: String,
    /// Raw `Subject:` (may be RFC 2047 encoded).
    pub subject: String,
    pub date: Option<DateTime<Utc>>,
    pub message_id: Option<String>,
    /// `To:` / `Delivered-To:`, used to learn which identifier was used (incl. Apple relay addresses).
    pub to: String,
    pub has_list_unsubscribe: bool,
    /// Raw `List-Unsubscribe` value.
    pub list_unsubscribe: Option<String>,
    /// Raw `List-Unsubscribe-Post` value.
    pub list_unsubscribe_post: Option<String>,
    /// Raw `List-Id` value.
    pub list_id: Option<String>,
    /// `Precedence: bulk/list`.
    pub precedence: Option<String>,
    /// `Return-Path` / `Sender`, for ESP-sent mail where From is the real brand.
    pub sender: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Classification {
    pub kind: EvidenceKind,
    /// Service domain or `unresolved:<slug>`.
    pub domain: String,
    pub name: Option<String>,
    pub sso: Option<SsoProvider>,
    /// Address the mail was sent to (the identifier at the service).
    pub identifier: Option<String>,
}

// ---------------------------------------------------------------------------------------------
// Text helpers
// ---------------------------------------------------------------------------------------------

fn fold_into(c: char, out: &mut String) {
    match c {
        'ä' => out.push_str("ae"),
        'ö' => out.push_str("oe"),
        'ü' => out.push_str("ue"),
        'ß' => out.push_str("ss"),
        'á' | 'à' | 'â' | 'ã' | 'å' | 'ą' => out.push('a'),
        'é' | 'è' | 'ê' | 'ë' | 'ę' => out.push('e'),
        'í' | 'ì' | 'î' | 'ï' => out.push('i'),
        'ó' | 'ò' | 'ô' | 'õ' => out.push('o'),
        'ú' | 'ù' | 'û' => out.push('u'),
        'ñ' | 'ń' => out.push('n'),
        'ç' | 'ć' => out.push('c'),
        'ł' => out.push('l'),
        'ś' => out.push('s'),
        'ź' | 'ż' => out.push('z'),
        '’' | '‘' | '`' => out.push('\''),
        _ => out.push(c),
    }
}

/// Lowercase and fold accents / umlauts to ASCII (`Bestätigung` -> `bestaetigung`).
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        for lc in c.to_lowercase() {
            fold_into(lc, &mut out);
        }
    }
    out
}

fn contains_any(text: &str, list: &[&str]) -> bool {
    list.iter().any(|k| text.contains(k))
}

/// Unfold a header value and decode RFC 2047 encoded words.
pub fn decode_header(raw: &str) -> String {
    let unfolded: String = raw
        .replace("\r\n", "")
        .replace(['\n', '\r'], "")
        .trim()
        .to_string();
    if !unfolded.contains("=?") {
        return unfolded;
    }
    match mailparse::parse_header(format!("X: {unfolded}").as_bytes()) {
        Ok((h, _)) => h.get_value().trim().to_string(),
        Err(_) => unfolded,
    }
}

/// Split a `From`-style value into (decoded display name, lowercase address).
pub fn parse_address(raw: &str) -> (Option<String>, Option<String>) {
    let raw = raw.trim();
    let (name_part, addr_part) = match (raw.rfind('<'), raw.rfind('>')) {
        (Some(lt), Some(gt)) if gt > lt => (&raw[..lt], &raw[lt + 1..gt]),
        _ => ("", raw),
    };
    let addr = addr_part.trim().trim_matches('"').trim();
    let addr = addr
        .strip_prefix("mailto:")
        .unwrap_or(addr)
        .trim()
        .to_lowercase();
    let addr = if addr.matches('@').count() == 1
        && !addr.starts_with('@')
        && !addr.ends_with('@')
        && !addr.chars().any(char::is_whitespace)
    {
        Some(addr)
    } else {
        None
    };
    let name = name_part.trim().trim_matches('"').trim();
    let name = if name.is_empty() {
        None
    } else {
        Some(decode_header(name))
    };
    let name = name.map(|n| n.trim().trim_matches('"').trim().to_string());
    let name = name.filter(|n| !n.is_empty() && !n.contains('@'));
    (name, addr)
}

static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[A-Za-z0-9._%+='\-]+@[A-Za-z0-9\-]+(?:\.[A-Za-z0-9\-]+)+").expect("email regex")
});

fn addresses(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for m in EMAIL_RE.find_iter(raw) {
        let a = m.as_str().to_lowercase();
        if !out.contains(&a) {
            out.push(a);
        }
    }
    out
}

pub(crate) fn host_of(addr: &str) -> &str {
    addr.rsplit_once('@').map(|(_, h)| h).unwrap_or("")
}

pub(crate) fn local_of(addr: &str) -> &str {
    addr.rsplit_once('@').map(|(l, _)| l).unwrap_or(addr)
}

pub(crate) fn host_is(host: &str, domain: &str) -> bool {
    host == domain
        || (host.len() > domain.len()
            && host.ends_with(domain)
            && host.as_bytes()[host.len() - domain.len() - 1] == b'.')
}

/// Public mailbox providers: mail from these addresses is personal, not a service.
const FREEMAIL: &[&str] = &[
    "gmail.com",
    "googlemail.com",
    "gmx.de",
    "gmx.net",
    "gmx.at",
    "gmx.ch",
    "gmx.com",
    "gmx.eu",
    "web.de",
    "outlook.com",
    "outlook.de",
    "outlook.fr",
    "outlook.es",
    "outlook.it",
    "hotmail.com",
    "hotmail.de",
    "hotmail.fr",
    "hotmail.it",
    "hotmail.es",
    "hotmail.co.uk",
    "hotmail.nl",
    "live.com",
    "live.de",
    "live.fr",
    "live.it",
    "live.nl",
    "live.co.uk",
    "msn.com",
    "yahoo.com",
    "yahoo.de",
    "yahoo.fr",
    "yahoo.it",
    "yahoo.es",
    "yahoo.co.uk",
    "yahoo.nl",
    "ymail.com",
    "rocketmail.com",
    "icloud.com",
    "me.com",
    "mac.com",
    "aol.com",
    "aol.de",
    "t-online.de",
    "freenet.de",
    "posteo.de",
    "posteo.net",
    "mailbox.org",
    "protonmail.com",
    "protonmail.ch",
    "proton.me",
    "pm.me",
    "fastmail.com",
    "fastmail.fm",
    "mail.com",
    "email.de",
    "online.de",
    "arcor.de",
    "mail.ru",
    "yandex.com",
    "yandex.ru",
    "qq.com",
    "163.com",
    "wp.pl",
    "o2.pl",
    "onet.pl",
    "libero.it",
    "orange.fr",
    "free.fr",
    "laposte.net",
    "ziggo.nl",
    "kpnmail.nl",
    "terra.com.br",
    "uol.com.br",
    "bol.com.br",
];

pub(crate) fn is_freemail(host: &str) -> bool {
    FREEMAIL.contains(&host)
}

pub fn is_bulk(h: &MailHeader) -> bool {
    if h.has_list_unsubscribe
        || h.list_unsubscribe
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty())
    {
        return true;
    }
    h.precedence.as_deref().is_some_and(|p| {
        let p = p.to_ascii_lowercase();
        p.contains("bulk") || p.contains("list") || p.contains("junk")
    })
}

fn is_spam_precedence(h: &MailHeader) -> bool {
    h.precedence
        .as_deref()
        .is_some_and(|p| p.to_ascii_lowercase().contains("junk"))
}

// ---------------------------------------------------------------------------------------------
// Keyword lists (folded form)
// ---------------------------------------------------------------------------------------------

const PASSWORD_RESET: &[&str] = &[
    // en
    "reset your password",
    "reset password",
    "password reset",
    "forgot your password",
    "forgot password",
    "change your password",
    "choose a new password",
    "set a new password",
    "create a new password",
    "reset your pass",
    // de
    "passwort zuruecksetzen",
    "passwort zuruecksetzen",
    "passwort-reset",
    "passwort reset",
    "passwort vergessen",
    "neues passwort",
    "passwort aendern",
    "kennwort zuruecksetzen",
    "kennwort vergessen",
    "zuruecksetzen ihres passworts",
    "zuruecksetzen deines passworts",
    // fr
    "reinitialisation du mot de passe",
    "reinitialiser votre mot de passe",
    "reinitialisez votre mot de passe",
    "reinitialiser le mot de passe",
    "mot de passe oublie",
    "nouveau mot de passe",
    // es
    "restablecer tu contrasena",
    "restablecer su contrasena",
    "restablecer la contrasena",
    "restablece tu contrasena",
    "recuperar contrasena",
    "recupera tu contrasena",
    "olvidaste tu contrasena",
    "cambiar contrasena",
    "nueva contrasena",
    // it
    "reimposta la password",
    "reimpostare la password",
    "reimposta la tua password",
    "recupera la password",
    "recupero password",
    "password dimenticata",
    "nuova password",
    // nl
    "wachtwoord resetten",
    "wachtwoord opnieuw instellen",
    "wachtwoord herstellen",
    "wachtwoord vergeten",
    "nieuw wachtwoord",
    "wachtwoord wijzigen",
    "reset je wachtwoord",
    "reset uw wachtwoord",
    // pl
    "zresetuj haslo",
    "resetowanie hasla",
    "reset hasla",
    "zmiana hasla",
    "przypomnienie hasla",
    // pt
    "redefinir senha",
    "redefinicao de senha",
    "recuperar senha",
    "redefina sua senha",
    "esqueceu sua senha",
    "nova senha",
];

const VERIFICATION: &[&str] = &[
    // en
    "verify your email",
    "verify your e-mail",
    "verify your account",
    "confirm your email",
    "confirm your e-mail",
    "confirm your account",
    "confirm your registration",
    "activate your account",
    "account activation",
    "email verification",
    "verification code",
    "verification link",
    "please verify",
    "finish signing up",
    "complete your registration",
    "complete your sign up",
    "complete your signup",
    // de
    "bestaetige deine e-mail",
    "bestaetigen sie ihre e-mail",
    "bestaetige deine email",
    "bestaetigen sie ihre email",
    "e-mail-adresse bestaetigen",
    "e-mail bestaetigen",
    "email bestaetigen",
    "mail-adresse bestaetigen",
    "adresse bestaetigen",
    "bestaetige deine registrierung",
    "bestaetigen sie ihre registrierung",
    "registrierung bestaetigen",
    "bestaetige deine anmeldung",
    "bestaetigen sie ihre anmeldung",
    "konto bestaetigen",
    "konto aktivieren",
    "account aktivieren",
    "aktiviere dein konto",
    "aktivieren sie ihr konto",
    "aktivierungslink",
    "bestaetigungslink",
    "bestaetigungscode",
    "verifizierungscode",
    "verifiziere",
    "e-mail-verifizierung",
    "bitte bestaetigen",
    // fr
    "confirmez votre adresse",
    "confirmez votre e-mail",
    "confirmez votre email",
    "confirmez votre compte",
    "verifiez votre adresse",
    "verifiez votre e-mail",
    "verifiez votre email",
    "verifiez votre compte",
    "code de verification",
    "activez votre compte",
    "confirmer votre adresse",
    "valider votre adresse",
    "validez votre adresse",
    "valider votre compte",
    "validez votre compte",
    // es
    "confirma tu correo",
    "confirma tu e-mail",
    "confirma tu email",
    "confirma tu cuenta",
    "confirme su correo",
    "verifica tu correo",
    "verifica tu email",
    "verifica tu cuenta",
    "verifique su correo",
    "codigo de verificacion",
    "activa tu cuenta",
    "active su cuenta",
    "confirmar tu correo",
    "confirmar su correo",
    "valida tu correo",
    // it
    "conferma il tuo indirizzo",
    "conferma la tua email",
    "conferma la tua e-mail",
    "conferma il tuo account",
    "verifica il tuo indirizzo",
    "verifica la tua email",
    "verifica il tuo account",
    "codice di verifica",
    "attiva il tuo account",
    "attiva il tuo profilo",
    "conferma la registrazione",
    // nl
    "bevestig je e-mail",
    "bevestig uw e-mail",
    "bevestig je emailadres",
    "bevestig uw emailadres",
    "bevestig je account",
    "bevestig uw account",
    "bevestig je registratie",
    "verifieer je",
    "verifieer uw",
    "verificatiecode",
    "verificatie code",
    "activeer je account",
    "activeer uw account",
    "bevestigingscode",
    // pl
    "potwierdz adres e-mail",
    "potwierdz swoj adres",
    "potwierdz email",
    "potwierdz konto",
    "potwierdz rejestracje",
    "aktywuj konto",
    "kod weryfikacyjny",
    "zweryfikuj",
    // pt
    "confirme seu e-mail",
    "confirme seu email",
    "confirme o seu e-mail",
    "confirme o seu email",
    "confirme sua conta",
    "verifique seu e-mail",
    "verifique seu email",
    "verifique a sua conta",
    "codigo de verificacao",
    "ative sua conta",
    "ative a sua conta",
    "confirmar seu e-mail",
];

const LOGIN_ALERT: &[&str] = &[
    // en
    "new sign-in",
    "new sign in",
    "new login",
    "new device",
    "sign-in attempt",
    "sign in attempt",
    "login attempt",
    "unusual sign",
    "unusual activity",
    "suspicious sign",
    "suspicious login",
    "suspicious activity",
    "security alert",
    "security notice",
    "security code",
    "login code",
    "log-in code",
    "sign-in code",
    "one-time code",
    "one-time password",
    "two-factor",
    "2-step",
    "2fa",
    "new browser",
    "signed in from",
    "logged in from",
    "was used to sign in",
    // de
    "neue anmeldung",
    "neuer login",
    "neues geraet",
    "anmeldeversuch",
    "ungewoehnliche anmeldung",
    "ungewoehnliche aktivitaet",
    "verdaechtige",
    "sicherheitswarnung",
    "sicherheitshinweis",
    "sicherheitscode",
    "anmeldecode",
    "login-code",
    "einmalcode",
    "einmalpasswort",
    "zwei-faktor",
    "2-faktor",
    "neu angemeldet",
    "anmeldung erkannt",
    // fr
    "nouvelle connexion",
    "connexion inhabituelle",
    "tentative de connexion",
    "alerte de securite",
    "code de securite",
    "code de connexion",
    "nouvel appareil",
    // es
    "nuevo inicio de sesion",
    "inicio de sesion inusual",
    "intento de inicio de sesion",
    "alerta de seguridad",
    "codigo de seguridad",
    "codigo de acceso",
    "nuevo dispositivo",
    "actividad inusual",
    "actividad sospechosa",
    // it
    "nuovo accesso",
    "accesso insolito",
    "tentativo di accesso",
    "avviso di sicurezza",
    "codice di sicurezza",
    "codice di accesso",
    "nuovo dispositivo",
    "attivita insolita",
    // nl
    "nieuwe aanmelding",
    "nieuwe login",
    "nieuwe inlog",
    "inlogpoging",
    "ongebruikelijke aanmelding",
    "beveiligingswaarschuwing",
    "beveiligingsmelding",
    "beveiligingscode",
    "inlogcode",
    "nieuw apparaat",
    "ongebruikelijke activiteit",
    // pl
    "nowe logowanie",
    "proba logowania",
    "alert bezpieczenstwa",
    "kod bezpieczenstwa",
    "kod logowania",
    "nowe urzadzenie",
    "podejrzane logowanie",
    // pt
    "novo login",
    "novo acesso",
    "tentativa de login",
    "alerta de seguranca",
    "codigo de seguranca",
    "novo dispositivo",
    "atividade incomum",
    "atividade suspeita",
];

const WELCOME: &[&str] = &[
    // en
    "welcome",
    "thanks for signing up",
    "thank you for signing up",
    "thanks for joining",
    "thank you for joining",
    "thanks for registering",
    "thank you for registering",
    "thanks for creating",
    "your account has been created",
    "your new account",
    "account created",
    "you're all set",
    "you are all set",
    "get started with",
    "getting started with",
    "registration confirmation",
    // de
    "willkommen",
    "danke fuer deine registrierung",
    "danke fuer ihre registrierung",
    "vielen dank fuer ihre registrierung",
    "vielen dank fuer deine registrierung",
    "danke fuer deine anmeldung",
    "vielen dank fuer ihre anmeldung",
    "dein konto wurde erstellt",
    "ihr konto wurde erstellt",
    "ihr konto wurde angelegt",
    "dein konto wurde angelegt",
    "registrierung erfolgreich",
    "erfolgreich registriert",
    "konto erstellt",
    "deine registrierung",
    "ihre registrierung",
    "schoen, dass du dabei bist",
    "schoen dass du dabei bist",
    "erste schritte",
    // fr
    "bienvenue",
    "merci de votre inscription",
    "merci pour votre inscription",
    "votre compte a ete cree",
    "votre inscription",
    "confirmation d'inscription",
    "compte cree",
    // es
    "bienvenido",
    "bienvenida",
    "gracias por registrarte",
    "gracias por tu registro",
    "gracias por registrarse",
    "tu cuenta ha sido creada",
    "su cuenta ha sido creada",
    "cuenta creada",
    // it
    "benvenuto",
    "benvenuta",
    "grazie per esserti registrato",
    "grazie per la registrazione",
    "il tuo account e stato creato",
    "registrazione completata",
    "account creato",
    // nl
    "welkom",
    "bedankt voor je registratie",
    "bedankt voor uw registratie",
    "je account is aangemaakt",
    "uw account is aangemaakt",
    "account aangemaakt",
    "registratie voltooid",
    // pl
    "witamy",
    "witaj w",
    "dziekujemy za rejestracje",
    "dziekujemy za zalozenie konta",
    "konto zostalo utworzone",
    // pt
    "bem-vindo",
    "bem vindo",
    "bem-vinda",
    "obrigado por se registrar",
    "obrigado por se cadastrar",
    "obrigado por criar",
    "sua conta foi criada",
    "conta criada",
    "cadastro concluido",
    "cadastro realizado",
];

const RECEIPT: &[&str] = &[
    // en
    "your order",
    "order confirmation",
    "order #",
    "order no",
    "order number",
    "your receipt",
    "receipt for",
    "receipt from",
    "payment receipt",
    "payment confirmation",
    "payment received",
    "invoice",
    "has shipped",
    "has been shipped",
    "shipping confirmation",
    "shipment",
    "out for delivery",
    "your purchase",
    "thanks for your purchase",
    "thank you for your purchase",
    "thanks for your order",
    "thank you for your order",
    "booking confirmation",
    "your booking",
    "your reservation",
    "your ticket",
    // de
    "bestellung",
    "bestellbestaetigung",
    "auftragsbestaetigung",
    "rechnung",
    "zahlungsbestaetigung",
    "zahlungseingang",
    "versandbestaetigung",
    "versendet",
    "wurde versandt",
    "paket",
    "lieferung",
    "sendungsverfolgung",
    "buchungsbestaetigung",
    "ihre buchung",
    "deine buchung",
    "kaufbestaetigung",
    "vielen dank fuer ihren einkauf",
    "vielen dank fuer deinen einkauf",
    "quittung",
    "zahlung",
    // fr
    "votre commande",
    "confirmation de commande",
    "facture",
    "recu de paiement",
    "confirmation de paiement",
    "paiement recu",
    "expedie",
    "votre colis",
    "livraison",
    "confirmation d'achat",
    "confirmation de reservation",
    "votre reservation",
    // es
    "tu pedido",
    "su pedido",
    "confirmacion de pedido",
    "confirmacion del pedido",
    "factura",
    "recibo",
    "comprobante",
    "pago recibido",
    "confirmacion de pago",
    "tu envio",
    "su envio",
    "confirmacion de compra",
    "confirmacion de reserva",
    "tu reserva",
    // it
    "il tuo ordine",
    "conferma d'ordine",
    "conferma dell'ordine",
    "fattura",
    "ricevuta",
    "pagamento ricevuto",
    "conferma di pagamento",
    "spedito",
    "la tua spedizione",
    "conferma d'acquisto",
    "conferma di prenotazione",
    "la tua prenotazione",
    // nl
    "je bestelling",
    "uw bestelling",
    "orderbevestiging",
    "bestelbevestiging",
    "factuur",
    "betaalbewijs",
    "betaling ontvangen",
    "betalingsbevestiging",
    "verzonden",
    "je pakket",
    "boekingsbevestiging",
    "je boeking",
    "aankoopbevestiging",
    // pl
    "zamowienie",
    "potwierdzenie zamowienia",
    "faktura",
    "potwierdzenie platnosci",
    "platnosc",
    "przesylka",
    "potwierdzenie rezerwacji",
    "rezerwacja",
    // pt
    "seu pedido",
    "o seu pedido",
    "confirmacao de pedido",
    "fatura",
    "nota fiscal",
    "pagamento recebido",
    "confirmacao de pagamento",
    "sua encomenda",
    "confirmacao de compra",
    "confirmacao de reserva",
    "sua reserva",
];

/// Local parts that indicate machine-generated service mail.
const AUTOMATED_LOCAL: &[&str] = &[
    "noreply",
    "no-reply",
    "no_reply",
    "donotreply",
    "do-not-reply",
    "do_not_reply",
    "bounce",
    "notification",
    "notify",
    "mailer",
    "alert",
    "automated",
    "system",
    "robot",
    "security",
    "accountprotection",
    "account-security",
    "service",
    "support",
    "customercare",
    "customer-care",
    "kundenservice",
    "kundenservice",
    "billing",
    "orders",
    "order",
    "invoice",
    "rechnung",
    "info",
    "hello",
    "hallo",
    "hi",
    "team",
    "welcome",
    "account",
    "accounts",
    "update",
    "updates",
    "news",
    "newsletter",
    "contact",
    "kontakt",
    "shop",
    "store",
    "online",
    "mail",
    "email",
    "post",
    "id",
    "appleid",
    "verify",
    "verification",
    "confirm",
    "auth",
    "login",
    "signup",
    "registration",
    "members",
    "membership",
    "community",
    "feedback",
    "booking",
    "reservations",
    "tickets",
    "payments",
    "payment",
    "receipts",
    "help",
];

fn is_automated_local(local: &str) -> bool {
    let l = local.to_ascii_lowercase();
    if l.contains('+') || l.contains('=') {
        return true;
    }
    // Compare on the leading token ("support.eu", "orders-de") and on containment for the long ones.
    let first = l.split(['.', '-', '_']).next().unwrap_or("");
    if AUTOMATED_LOCAL.contains(&l.as_str()) || AUTOMATED_LOCAL.contains(&first) {
        return true;
    }
    [
        "noreply",
        "no-reply",
        "no_reply",
        "donotreply",
        "do-not-reply",
        "bounce",
        "notification",
        "mailer",
        "newsletter",
        "kundenservice",
    ]
    .iter()
    .any(|k| l.contains(k))
}

static PROMO_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(?:rabatt\w*|sale|deals?|offers?|promo\w*|coupons?|vouchers?|gutschein\w*|angebot\w*|sonderangebot\w*|schnaeppchen|sparen|spare|aktion(?:en)?|gratis|geschenk\w*|gewinnspiel\w*|exklusiv\w*|exclusive|limited|last chance|letzte chance|nur heute|jetzt sichern|black friday|cyber monday|flash sale|free shipping|kostenloser versand|versandkostenfrei|new arrivals|neuheiten|kollektion|collection|soldes?|promotions?|remises?|bon de reduction|offres?|ofertas?|descuentos?|rebajas|offerte|sconto|sconti|saldi|korting\w*|aanbieding\w*|uitverkoop|actie|promocj\w+|rabat|znizk\w+|promocao|promocoes|desconto\w*|save up|save \d+)\b",
    )
    .expect("promo regex")
});

static PERCENT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\d\s?%|%\s?(?:off|rabatt|reduction|korting|descuento|sconto|desconto)")
        .expect("percent regex")
});

pub(crate) fn is_promo(folded_subject: &str) -> bool {
    PROMO_RE.is_match(folded_subject) || PERCENT_RE.is_match(folded_subject)
}

fn keyword_kind(folded_subject: &str) -> Option<EvidenceKind> {
    let s = folded_subject;
    if contains_any(s, PASSWORD_RESET) {
        Some(EvidenceKind::PasswordReset)
    } else if contains_any(s, VERIFICATION) {
        Some(EvidenceKind::Verification)
    } else if contains_any(s, LOGIN_ALERT) {
        Some(EvidenceKind::LoginAlert)
    } else if contains_any(s, WELCOME) {
        Some(EvidenceKind::Welcome)
    } else if contains_any(s, RECEIPT) {
        Some(EvidenceKind::Receipt)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------------------------
// SSO providers
// ---------------------------------------------------------------------------------------------

fn provider_for_host(host: &str) -> Option<SsoProvider> {
    if host_is(host, "accounts.google.com") || host_is(host, "google.com") {
        Some(SsoProvider::Google)
    } else if host_is(host, "github.com") {
        Some(SsoProvider::GitHub)
    } else if host_is(host, "microsoft.com")
        || host_is(host, "live.com")
        || host_is(host, "microsoftonline.com")
    {
        Some(SsoProvider::Microsoft)
    } else if host_is(host, "apple.com") {
        Some(SsoProvider::Apple)
    } else if host_is(host, "facebookmail.com") || host_is(host, "facebook.com") {
        Some(SsoProvider::Facebook)
    } else if host_is(host, "discord.com") || host_is(host, "discordapp.com") {
        Some(SsoProvider::Discord)
    } else if host_is(host, "slack.com") {
        Some(SsoProvider::Other("Slack".to_string()))
    } else if host_is(host, "twitter.com") || host_is(host, "x.com") {
        Some(SsoProvider::X)
    } else {
        None
    }
}

fn sender_provider(h: &MailHeader) -> Option<SsoProvider> {
    let (_, addr) = parse_address(&h.from);
    provider_for_host(host_of(&addr?))
}

const SSO_SUBJECTS_GOOGLE: &[&str] = &[
    "sicherheitswarnung",
    "security alert",
    "alerte de securite",
    "alerta de seguridad",
    "avviso di sicurezza",
    "beveiligingswaarschuwing",
    "alert bezpieczenstwa",
    "alerta de seguranca",
    "zugriff auf ihr google-konto",
    "zugriff auf dein google-konto",
    "access to your google account",
    "granted access",
    "hat zugriff",
];
const SSO_SUBJECTS_GITHUB: &[&str] = &[
    "oauth application",
    "oauth app",
    "github app",
    "authorized application",
    "application has been added",
    "app has been installed",
    "app was installed",
    "third-party",
];
const SSO_SUBJECTS_MICROSOFT: &[&str] = &[
    "app connected",
    "app verbunden",
    "neue app",
    "app hat zugriff",
    "app has access",
    "apps connected",
    "hat zugriff",
    "connected to your microsoft account",
    "mit ihrem microsoft-konto verbunden",
    "has been granted",
    "berechtigung",
    "new app",
];
const SSO_SUBJECTS_APPLE: &[&str] = &[
    "sign in with apple",
    "mit apple anmelden",
    "apple anmelden",
    "se connecter avec apple",
    "iniciar sesion con apple",
    "accedi con apple",
    "inloggen met apple",
    "logowanie przez apple",
    "entrar com apple",
];
const SSO_SUBJECTS_FACEBOOK: &[&str] = &[
    "used facebook to log",
    "used facebook to sign",
    "facebook to log in",
    "facebook to sign in",
    "mit facebook bei",
    "facebook verwendet",
    "utilise facebook",
    "utilise facebook",
    "has usado facebook",
    "hai usato facebook",
    "je hebt facebook gebruikt",
];
const SSO_SUBJECTS_DISCORD: &[&str] = &[
    "authorized app",
    "authorised app",
    "autorisierte app",
    "app autorisiert",
    "new app",
    "application authorized",
    "autorise",
    "autorizada",
    "autorizzata",
    "geautoriseerd",
];

/// True if this is an SSO-provider security mail whose body must be fetched to learn
/// the connected app (Google "Security alert" / "was granted access", GitHub
/// "A third-party OAuth application has been added", Microsoft "app connected", ...).
pub fn needs_body(header: &MailHeader) -> bool {
    let Some(provider) = sender_provider(header) else {
        return false;
    };
    let s = fold(&decode_header(&header.subject));
    match provider {
        SsoProvider::Google => contains_any(&s, SSO_SUBJECTS_GOOGLE),
        SsoProvider::GitHub => contains_any(&s, SSO_SUBJECTS_GITHUB),
        SsoProvider::Microsoft => contains_any(&s, SSO_SUBJECTS_MICROSOFT),
        SsoProvider::Apple => contains_any(&s, SSO_SUBJECTS_APPLE),
        SsoProvider::Facebook => contains_any(&s, SSO_SUBJECTS_FACEBOOK),
        SsoProvider::Discord => contains_any(&s, SSO_SUBJECTS_DISCORD),
        SsoProvider::Other(_) => {
            s.contains("app")
                && contains_any(&s, &["added", "installed", "hinzugefuegt", "installiert"])
        }
        SsoProvider::X => contains_any(
            &s,
            &[
                "connected app",
                "granted access",
                "zugriff auf dein",
                "zugriff auf deinen",
            ],
        ),
        _ => false,
    }
}

struct SsoRegexes {
    google: Regex,
    gh_paren: Regex,
    gh_quoted: Regex,
    line_app: Regex,
    ms_quoted: Regex,
    ms_has: Regex,
    apple_quoted: Regex,
    apple_plain: Regex,
    fb: Vec<Regex>,
    dc_label: Regex,
    dc_quoted: Regex,
    dc_phrase: Regex,
    dc_you: Regex,
    url: Regex,
    domain_like: Regex,
    block_tags: Regex,
    tags: Regex,
}

static SSO: LazyLock<SsoRegexes> = LazyLock::new(|| {
    let re = |p: &str| Regex::new(p).expect("sso regex");
    SsoRegexes {
        google: re(
            r#"(?im)^[\s*>•\-]*(.{1,80}?)\s+(?:was granted access to your google account|has been granted access to your google account|hat zugriff auf (?:ihr|dein|euer) google-konto(?: erhalten)?|a obtenu l.acc.s . votre compte google|ha obtenido acceso a tu cuenta de google|ha ottenuto l.accesso al tuo account google|heeft toegang gekregen tot je google-account|recebeu acesso . sua conta do google)"#,
        ),
        gh_paren: re(
            r#"(?i)\b(?:oauth application|oauth app|github app|application)\s*\(\s*([^)\n]{1,80}?)\s*\)"#,
        ),
        gh_quoted: re(
            r#"(?i)\b(?:oauth application|oauth app|github app|application|app)\s*[:,]?\s*["“‘'`]([^"”’'`\n]{1,80})["”’'`]"#,
        ),
        line_app: re(
            r#"(?im)^\s*(?:app(?:lication)?(?:[ -]?name)?|anwendung|name der app)\s*[:：]\s*(.{1,80}?)\s*$"#,
        ),
        ms_quoted: re(
            r#"(?i)\b(?:die app|the app|app|application|anwendung|die anwendung)\s+["„“‘']([^"”“’'\n]{1,80})["”“’']"#,
        ),
        ms_has: re(
            r#"(?i)["„“]([^"”“\n]{1,80})["”“]\s+(?:has access|hat zugriff|wurde berechtigt|has been (?:granted|given))"#,
        ),
        apple_quoted: re(
            r#"(?i)(?:with|bei|für|fuer|for|avec|con|met|mit)\s+["„“‘'«]([^"”“’'»\n]{1,60})["”“’'»]"#,
        ),
        apple_plain: re(
            r#"(?i)(?:sign in with apple|mit apple anmelden)["”“’']?\s+(?:with|bei|für|for|mit)\s+([^\n.!,"”“]{1,50})"#,
        ),
        fb: vec![
            re(
                r#"(?im)used facebook to (?:log ?in ?to|log ?in|sign ?in ?to|sign ?in)\s+(.+?)\s*(?:[.!]|$)"#,
            ),
            re(
                r#"(?im)(?:mit facebook bei|facebook verwendet, um (?:dich|sich|euch) bei)\s+(.+?)\s+(?:angemeldet|anzumelden|eingeloggt|einzuloggen)"#,
            ),
            re(r#"(?im)utilis[ée] facebook pour (?:vous )?connecter [àa]\s+(.+?)\s*(?:[.!]|$)"#),
            re(r#"(?im)facebook para (?:iniciar sesi.n|entrar) en\s+(.+?)\s*(?:[.!]|$)"#),
            re(r#"(?im)facebook per accedere a\s+(.+?)\s*(?:[.!]|$)"#),
        ],
        dc_label: re(
            r#"(?im)(?:authori[sz]ed app(?:lication)?|autorisierte app|app autorisiert|application autoris.e|aplicaci.n autorizada)\s*[:：]\s*(.{1,80}?)\s*$"#,
        ),
        dc_quoted: re(
            r#"(?i)["“„]([^"”“\n]{1,80})["”“]\s+(?:has been authori[sz]ed|wurde autorisiert|a .t. autoris.e|has access|hat zugriff)"#,
        ),
        dc_phrase: re(
            r#"(?im)^\s*(.{1,80}?)\s+(?:has been authori[sz]ed|was authori[sz]ed|wurde autorisiert|hat zugriff auf dein(?:en)? (?:discord-)?(?:konto|account))"#,
        ),
        dc_you: re(
            r#"(?i)you(?:'|’)?(?:ve| have)? authori[sz]ed\s+(.+?)\s*(?:to access|to your|to use|[.!]|$)"#,
        ),
        url: re(r#"(?i)https?://([a-z0-9][a-z0-9.\-]*[a-z0-9])"#),
        domain_like: re(r"^(?:[a-z0-9\-]+\.)+[a-z]{2,}$"),
        block_tags: re(r"(?i)<\s*(?:br|/p|/div|/tr|/li|/h[1-6]|/td)\b[^>]*>"),
        tags: re(r"<[^>]*>"),
    }
});

fn html_to_text(body: &str) -> String {
    let looks_html =
        body.contains('<') && (body.contains("</") || body.to_ascii_lowercase().contains("<br"));
    if !looks_html {
        return body.to_string();
    }
    let s = SSO.block_tags.replace_all(body, "\n");
    let s = SSO.tags.replace_all(&s, "");
    let s = s
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'");
    s.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n")
}

const GENERIC_APP_NAMES: &[&str] = &[
    "your account",
    "ihr konto",
    "dein konto",
    "a new app",
    "an app",
    "ein app",
    "eine app",
    "a third-party app",
    "the following app",
    "app",
    "application",
    "anwendung",
    "google",
];

fn clean_app(raw: &str) -> Option<String> {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_matches(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '"' | '\''
                    | '„'
                    | '“'
                    | '”'
                    | '‘'
                    | '’'
                    | '«'
                    | '»'
                    | '`'
                    | '*'
                    | '.'
                    | ','
                    | ':'
                    | ';'
                    | '!'
                    | '('
                    | ')'
            )
    });
    if trimmed.is_empty()
        || trimmed.chars().count() > 80
        || trimmed.contains('@')
        || fold(trimmed).starts_with("http")
        || GENERIC_APP_NAMES.contains(&fold(trimmed).as_str())
    {
        return None;
    }
    Some(trimmed.to_string())
}

fn first_capture(re: &Regex, text: &str) -> Option<String> {
    re.captures_iter(text)
        .find_map(|c| c.get(1).and_then(|m| clean_app(m.as_str())))
}

fn compact_slug(s: &str) -> String {
    fold(s)
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect()
}

/// Dash separated lowercase slug for `unresolved:<slug>` domains.
pub(crate) fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in fold(s).chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// Registrable-ish label of a host ("open.spotify.com" -> "spotify", "shop.example.co.uk" -> "example").
fn main_label(host: &str) -> Option<String> {
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    let mut idx = parts.len() - 2;
    if parts.len() >= 3 && matches!(parts[idx], "co" | "com" | "org" | "net" | "gov" | "ac") {
        idx -= 1;
    }
    Some(parts[idx].to_lowercase())
}

/// Find a URL host in the body that belongs to the app (its main label matches the app name).
fn app_domain(app: &str, text: &str) -> Option<String> {
    let lower = app.trim().to_lowercase();
    if SSO.domain_like.is_match(&lower) {
        return Some(lower);
    }
    let compact = compact_slug(app);
    if compact.len() < 3 {
        return None;
    }
    for cap in SSO.url.captures_iter(text) {
        let host = cap[1].to_lowercase();
        if let Some(label) = main_label(&host) {
            let related = label == compact
                || (label.len() >= 4 && compact.contains(&label))
                || (compact.len() >= 4 && label.contains(&compact));
            if related {
                return Some(host);
            }
        }
    }
    None
}

fn parse_app(provider: &SsoProvider, subject: &str, text: &str) -> Option<String> {
    let s = &*SSO;
    let with_subject = || format!("{subject}\n{text}");
    match provider {
        SsoProvider::Google => first_capture(&s.google, text),
        SsoProvider::GitHub => first_capture(&s.gh_paren, text)
            .or_else(|| first_capture(&s.gh_quoted, text))
            .or_else(|| first_capture(&s.line_app, text))
            .or_else(|| first_capture(&s.gh_paren, subject))
            .or_else(|| first_capture(&s.gh_quoted, subject)),
        SsoProvider::Microsoft => first_capture(&s.line_app, text)
            .or_else(|| first_capture(&s.ms_quoted, text))
            .or_else(|| first_capture(&s.ms_has, text))
            .or_else(|| first_capture(&s.ms_quoted, subject)),
        SsoProvider::Apple => {
            let all = with_subject();
            let relevant: Vec<&str> = all
                .lines()
                .filter(|l| {
                    let f = fold(l);
                    contains_any(&f, SSO_SUBJECTS_APPLE)
                })
                .collect();
            let relevant = relevant.join("\n");
            first_capture(&s.apple_quoted, &relevant)
                .or_else(|| first_capture(&s.apple_plain, &relevant))
        }
        SsoProvider::Facebook => {
            let all = with_subject();
            s.fb.iter().find_map(|re| first_capture(re, &all))
        }
        SsoProvider::Discord => first_capture(&s.dc_label, text)
            .or_else(|| first_capture(&s.dc_quoted, text))
            .or_else(|| first_capture(&s.dc_phrase, text))
            .or_else(|| first_capture(&s.dc_you, text))
            .or_else(|| first_capture(&s.dc_label, subject)),
        _ => first_capture(&s.line_app, text)
            .or_else(|| first_capture(&s.ms_quoted, text))
            .or_else(|| first_capture(&s.ms_quoted, subject)),
    }
}

/// Extract (provider, app name, app domain if found) from an SSO security mail body.
pub fn parse_sso_body(
    header: &MailHeader,
    body: &str,
) -> Option<(SsoProvider, String, Option<String>)> {
    let provider = sender_provider(header)?;
    let subject = decode_header(&header.subject);
    let text = html_to_text(body);
    let app = parse_app(&provider, &subject, &text)?;
    let domain = app_domain(&app, &format!("{text}\n{body}"));
    Some((provider, app, domain))
}

/// Full classification of an SSO security mail with its body: the connected app becomes an
/// `SsoGrant` for the app's domain (or `unresolved:<slug>` if no domain is known).
pub fn classify_sso(header: &MailHeader, body: &str) -> Option<Classification> {
    let (provider, app, domain) = parse_sso_body(header, body)?;
    let domain = domain.unwrap_or_else(|| format!("{UNRESOLVED_PREFIX}{}", slug(&app)));
    if domain == UNRESOLVED_PREFIX {
        return None;
    }
    let identifier = {
        let addrs = addresses(&header.to);
        addrs
            .iter()
            .find(|a| is_apple_relay(a))
            .cloned()
            .or_else(|| {
                if addrs.len() == 1 {
                    addrs.into_iter().next()
                } else {
                    None
                }
            })
    };
    Some(Classification {
        kind: EvidenceKind::SsoGrant,
        domain,
        name: Some(app),
        sso: Some(provider),
        identifier,
    })
}

fn is_apple_relay(addr: &str) -> bool {
    host_of(addr) == "privaterelay.appleid.com"
}

// ---------------------------------------------------------------------------------------------
// classify / categorize
// ---------------------------------------------------------------------------------------------

/// Classify one mail by headers. Returns `None` for personal mail / irrelevant mail.
/// Multilingual keyword rules (de, en, fr, es, it, nl at least) for welcome, verification,
/// password reset, login alert, receipt; List-Unsubscribe without other signal -> Newsletter.
pub fn classify(header: &MailHeader) -> Option<Classification> {
    classify_for(header, None)
}

/// Like [`classify`], additionally ignoring mails sent from the user's own address.
pub fn classify_for(header: &MailHeader, own_address: Option<&str>) -> Option<Classification> {
    let (name, from) = parse_address(&header.from);
    let from = from?;
    let host = host_of(&from).to_string();
    let local = local_of(&from).to_string();
    if !host.contains('.') || is_freemail(&host) {
        return None;
    }
    if matches!(local.as_str(), "mailer-daemon" | "postmaster") {
        return None;
    }
    let to_addrs = addresses(&header.to);
    if own_address.is_some_and(|o| o.trim().eq_ignore_ascii_case(&from))
        || (to_addrs.len() == 1 && to_addrs[0] == from)
    {
        return None;
    }

    let folded = fold(&decode_header(&header.subject));
    let bulk = is_bulk(header);
    let kind = match keyword_kind(&folded) {
        Some(
            k @ (EvidenceKind::PasswordReset
            | EvidenceKind::Verification
            | EvidenceKind::LoginAlert),
        ) => k,
        Some(k @ (EvidenceKind::Welcome | EvidenceKind::Receipt)) => {
            if bulk && is_promo(&folded) {
                EvidenceKind::Newsletter
            } else {
                k
            }
        }
        _ => {
            if bulk {
                EvidenceKind::Newsletter
            } else if is_automated_local(&local) {
                EvidenceKind::OtherMail
            } else {
                return None;
            }
        }
    };

    let relay = to_addrs.iter().find(|a| is_apple_relay(a)).cloned();
    let sender_is_apple = matches!(provider_for_host(&host), Some(SsoProvider::Apple));
    let (sso, identifier) = match relay {
        Some(r) if !sender_is_apple => (Some(SsoProvider::Apple), Some(r)),
        _ => (
            None,
            if to_addrs.len() == 1 {
                to_addrs.first().cloned()
            } else {
                None
            },
        ),
    };

    Some(Classification {
        kind,
        domain: host,
        name,
        sso,
        identifier,
    })
}

const SOCIAL_HOSTS: &[&str] = &[
    "facebookmail.com",
    "facebook.com",
    "linkedin.com",
    "x.com",
    "twitter.com",
    "instagram.com",
    "tiktok.com",
    "pinterest.com",
    "reddit.com",
    "redditmail.com",
    "xing.com",
    "youtube.com",
    "meetup.com",
    "quora.com",
    "snapchat.com",
    "tumblr.com",
    "nextdoor.com",
    "mastodon.social",
    "bsky.app",
    "threads.net",
    "vk.com",
    "whatsapp.com",
    "discord.com",
];

const NEWSLETTER_HOSTS: &[&str] = &[
    "substack.com",
    "beehiiv.com",
    "medium.com",
    "ghost.io",
    "buttondown.email",
    "getrevue.co",
];

const SPAM_WORDS: &[&str] = &[
    "viagra",
    "cialis",
    "casino",
    "you have won",
    "du hast gewonnen",
    "sie haben gewonnen",
    "lottery",
    "lotterie",
    "inheritance",
    "erbschaft",
    "bitcoin",
    "make money",
    "geld verdienen",
    "singles in",
    "weight loss",
    "abnehmen",
    "porn",
    "sex",
    "urgent reply",
    "dringende antwort",
];

static NEWSLETTER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(?:digest|weekly|daily|monthly|newsletter|briefing|roundup|round-up|issue|ausgabe|this week|diese woche|woechentlich|taeglich|monatlich|wochenrueckblick|highlights|neuigkeiten|news)\b",
    )
    .expect("newsletter regex")
});

/// Rule-based category for bulk mail (used for subscriptions; AI may refine later).
pub fn categorize(header: &MailHeader) -> MailCategory {
    let (_, from) = parse_address(&header.from);
    let host = from.as_deref().map(host_of).unwrap_or("").to_string();
    let local = from.as_deref().map(local_of).unwrap_or("").to_string();
    let folded = fold(&decode_header(&header.subject));
    let promo = is_promo(&folded);

    // Transactional: never suggest unsubscribing.
    if contains_any(&folded, PASSWORD_RESET)
        || contains_any(&folded, VERIFICATION)
        || contains_any(&folded, LOGIN_ALERT)
        || (contains_any(&folded, RECEIPT) && !promo)
        || matches!(
            local.as_str(),
            "orders" | "order" | "billing" | "invoice" | "rechnung" | "security"
        )
    {
        return MailCategory::Transactional;
    }
    if SOCIAL_HOSTS.iter().any(|d| host_is(&host, d)) {
        return MailCategory::Social;
    }
    if is_spam_precedence(header) || SPAM_WORDS.iter().any(|w| has_word(&folded, w)) {
        return MailCategory::Spam;
    }
    if promo {
        return MailCategory::Marketing;
    }
    if NEWSLETTER_HOSTS.iter().any(|d| host_is(&host, d))
        || NEWSLETTER_RE.is_match(&folded)
        || local.contains("newsletter")
    {
        return MailCategory::Newsletter;
    }
    MailCategory::Unknown
}

/// Whole-word (or whole-phrase) containment on ASCII-folded text.
fn has_word(text: &str, word: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = text[start..].find(word) {
        let i = start + pos;
        let before_ok = i == 0 || !text.as_bytes()[i - 1].is_ascii_alphanumeric();
        let end = i + word.len();
        let after_ok = end >= text.len() || !text.as_bytes()[end].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        start = i + word.len().max(1);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(from: &str, subject: &str) -> MailHeader {
        MailHeader {
            from: from.into(),
            subject: subject.into(),
            ..Default::default()
        }
    }

    fn bulk(from: &str, subject: &str) -> MailHeader {
        MailHeader {
            from: from.into(),
            subject: subject.into(),
            has_list_unsubscribe: true,
            list_unsubscribe: Some("<mailto:u@x.example>".into()),
            ..Default::default()
        }
    }

    fn kind(from: &str, subject: &str) -> Option<EvidenceKind> {
        classify(&h(from, subject)).map(|c| c.kind)
    }

    #[test]
    fn fold_transliterates() {
        assert_eq!(
            fold("Bestätigung Ärger Straße"),
            "bestaetigung aerger strasse"
        );
        assert_eq!(fold("Contraseña código"), "contrasena codigo");
        assert_eq!(fold("Hasło zamówienie"), "haslo zamowienie");
    }

    #[test]
    fn parse_address_variants() {
        assert_eq!(
            parse_address("\"Spotify\" <No-Reply@Spotify.com>"),
            (Some("Spotify".into()), Some("no-reply@spotify.com".into()))
        );
        assert_eq!(
            parse_address("a@b.example"),
            (None, Some("a@b.example".into()))
        );
        assert_eq!(
            parse_address("\"x@y.com\" <real@shop.example>"),
            (None, Some("real@shop.example".into()))
        );
        assert_eq!(parse_address("garbage"), (None, None));
    }

    #[test]
    fn rfc2047_subject_and_name() {
        let c = classify(&h(
            "=?UTF-8?B?U3BvdGlmeSDDnA==?= <no-reply@spotify.com>",
            "=?UTF-8?Q?Willkommen_bei_Spotify?=",
        ))
        .unwrap();
        assert_eq!(c.kind, EvidenceKind::Welcome);
        assert_eq!(c.name.as_deref(), Some("Spotify Ü"));
        assert_eq!(decode_header("=?UTF-8?Q?B=C3=A4r?="), "Bär");
    }

    #[test]
    fn folded_subject_header() {
        assert_eq!(
            kind("Shop <no-reply@shop.example>", "Willkommen\r\n bei Shop"),
            Some(EvidenceKind::Welcome)
        );
    }

    #[test]
    fn welcome_multilingual() {
        for s in [
            "Welcome to Spotify",
            "Willkommen bei Spotify!",
            "Bienvenue chez Spotify",
            "Bienvenido a Spotify",
            "Benvenuto su Spotify",
            "Welkom bij Spotify",
            "Witamy w Spotify",
            "Bem-vindo ao Spotify",
            "Vielen Dank für Ihre Registrierung",
            "Thanks for signing up",
        ] {
            assert_eq!(
                kind("Spotify <no-reply@spotify.com>", s),
                Some(EvidenceKind::Welcome),
                "{s}"
            );
        }
    }

    #[test]
    fn verification_multilingual() {
        for s in [
            "Verify your email address",
            "Bitte bestätige deine E-Mail-Adresse",
            "Bestätigen Sie Ihre E-Mail",
            "Confirmez votre adresse e-mail",
            "Confirma tu correo electrónico",
            "Conferma il tuo indirizzo email",
            "Bevestig je e-mailadres",
            "Potwierdź adres e-mail",
            "Confirme seu e-mail",
            "Your verification code is 123456",
        ] {
            assert_eq!(
                kind("Foo <noreply@foo.example>", s),
                Some(EvidenceKind::Verification),
                "{s}"
            );
        }
    }

    #[test]
    fn password_reset_multilingual() {
        for s in [
            "Reset your password",
            "Passwort zurücksetzen",
            "Réinitialisation du mot de passe",
            "Restablecer tu contraseña",
            "Reimposta la password",
            "Wachtwoord resetten",
            "Zresetuj hasło",
            "Redefinir senha",
        ] {
            assert_eq!(
                kind("Foo <noreply@foo.example>", s),
                Some(EvidenceKind::PasswordReset),
                "{s}"
            );
        }
    }

    #[test]
    fn login_alert_multilingual() {
        for s in [
            "New sign-in to your account",
            "Neue Anmeldung auf einem neuen Gerät",
            "Nouvelle connexion détectée",
            "Nuevo inicio de sesión",
            "Nuovo accesso al tuo account",
            "Nieuwe aanmelding",
            "Nowe logowanie",
            "Novo login detectado",
            "Sicherheitswarnung",
        ] {
            assert_eq!(
                kind("Foo <noreply@foo.example>", s),
                Some(EvidenceKind::LoginAlert),
                "{s}"
            );
        }
    }

    #[test]
    fn receipts_multilingual() {
        for s in [
            "Your order #123 has shipped",
            "Ihre Bestellung bei Foo",
            "Rechnung 2024-001",
            "Votre commande est confirmée",
            "Tu pedido está en camino",
            "Il tuo ordine è stato spedito",
            "Je bestelling is verzonden",
            "Potwierdzenie zamówienia",
            "Seu pedido foi enviado",
        ] {
            assert_eq!(
                kind("Foo <orders@foo.example>", s),
                Some(EvidenceKind::Receipt),
                "{s}"
            );
        }
    }

    #[test]
    fn password_reset_beats_other_keywords() {
        assert_eq!(
            kind(
                "Foo <noreply@foo.example>",
                "Reset your password for your new account"
            ),
            Some(EvidenceKind::PasswordReset)
        );
    }

    #[test]
    fn list_unsubscribe_is_newsletter() {
        let c = classify(&bulk(
            "Foo News <news@foo.example>",
            "Unsere Themen der Woche",
        ))
        .unwrap();
        assert_eq!(c.kind, EvidenceKind::Newsletter);
        assert_eq!(c.domain, "foo.example");
        assert_eq!(c.name.as_deref(), Some("Foo News"));
    }

    #[test]
    fn precedence_bulk_is_newsletter() {
        let mut m = h("Foo <hello@foo.example>", "Hallo");
        m.precedence = Some("bulk".into());
        assert_eq!(classify(&m).unwrap().kind, EvidenceKind::Newsletter);
    }

    #[test]
    fn welcome_with_unsubscribe_stays_welcome() {
        let c = classify(&bulk("Foo <hello@foo.example>", "Welcome to Foo")).unwrap();
        assert_eq!(c.kind, EvidenceKind::Welcome);
    }

    #[test]
    fn promotional_welcome_is_newsletter() {
        let c = classify(&bulk(
            "Foo <hello@foo.example>",
            "Welcome to our Black Friday Sale: 50% off",
        ))
        .unwrap();
        assert_eq!(c.kind, EvidenceKind::Newsletter);
    }

    #[test]
    fn security_beats_bulk() {
        let c = classify(&bulk("Foo <noreply@foo.example>", "Reset your password")).unwrap();
        assert_eq!(c.kind, EvidenceKind::PasswordReset);
    }

    #[test]
    fn noreply_without_signal_is_other_mail() {
        assert_eq!(
            kind(
                "Foo <noreply@foo.example>",
                "Your monthly statement is ready"
            ),
            Some(EvidenceKind::OtherMail)
        );
    }

    #[test]
    fn personal_mail_ignored() {
        assert_eq!(
            kind(
                "Max Mustermann <max.mustermann@gmail.com>",
                "Treffen morgen?"
            ),
            None
        );
        assert_eq!(kind("Oma <oma@gmx.de>", "Willkommen zurück!"), None);
        assert_eq!(
            kind("Kollege <anna.schmidt@firma.example>", "Protokoll"),
            None
        );
    }

    #[test]
    fn freemail_subdomain_is_a_service() {
        assert_eq!(
            kind("Yahoo <no-reply@mail.yahoo.com>", "Welcome to Yahoo"),
            Some(EvidenceKind::Welcome)
        );
    }

    #[test]
    fn own_address_ignored() {
        let mut m = h("Ich <me@example.org>", "Welcome notes");
        m.to = "me@example.org".into();
        assert!(classify(&m).is_none());
        let m2 = h("Me <me@example.org>", "Willkommen");
        assert!(classify_for(&m2, Some("ME@example.org")).is_none());
    }

    #[test]
    fn mailer_daemon_ignored() {
        assert_eq!(
            kind(
                "Mail Delivery <mailer-daemon@foo.example>",
                "Undelivered Mail Returned to Sender"
            ),
            None
        );
    }

    #[test]
    fn missing_or_broken_from_ignored() {
        assert!(classify(&h("", "Welcome")).is_none());
        assert!(classify(&h("nonsense", "Welcome")).is_none());
        assert!(classify(&h("<a@localhost>", "Welcome")).is_none());
    }

    #[test]
    fn identifier_from_single_to() {
        let mut m = h("Foo <noreply@foo.example>", "Welcome to Foo");
        m.to = "\"Max\" <Max@Example.org>".into();
        assert_eq!(
            classify(&m).unwrap().identifier.as_deref(),
            Some("max@example.org")
        );
        m.to = "a@example.org, b@example.org".into();
        assert_eq!(classify(&m).unwrap().identifier, None);
    }

    #[test]
    fn apple_relay_address() {
        let mut m = h("Foo <noreply@foo.example>", "Welcome to Foo");
        m.to = "abc123xyz@privaterelay.appleid.com".into();
        let c = classify(&m).unwrap();
        assert_eq!(c.sso, Some(SsoProvider::Apple));
        assert_eq!(
            c.identifier.as_deref(),
            Some("abc123xyz@privaterelay.appleid.com")
        );
        assert_eq!(c.domain, "foo.example");
    }

    #[test]
    fn google_security_alert_needs_body() {
        let m = h(
            "Google <no-reply@accounts.google.com>",
            "Sicherheitswarnung",
        );
        assert!(needs_body(&m));
        assert_eq!(classify(&m).unwrap().kind, EvidenceKind::LoginAlert);
        assert!(needs_body(&h(
            "Google <no-reply@accounts.google.com>",
            "Security alert"
        )));
        assert!(!needs_body(&h(
            "Google <no-reply@accounts.google.com>",
            "Your storage is almost full"
        )));
        assert!(!needs_body(&h(
            "Foo <no-reply@foo.example>",
            "Security alert"
        )));
    }

    #[test]
    fn needs_body_other_providers() {
        assert!(needs_body(&h(
            "GitHub <noreply@github.com>",
            "[GitHub] A third-party OAuth application has been added to your account"
        )));
        assert!(needs_body(&h(
            "GitHub <noreply@github.com>",
            "[GitHub] A new GitHub App has been installed on your account"
        )));
        assert!(!needs_body(&h(
            "GitHub <noreply@github.com>",
            "[GitHub] Your weekly digest"
        )));
        assert!(needs_body(&h(
            "Microsoft account team <account-security-noreply@accountprotection.microsoft.com>",
            "Neue App hat Zugriff auf Ihr Konto"
        )));
        assert!(needs_body(&h(
            "Apple <appleid@id.apple.com>",
            "Du verwendest „Mit Apple anmelden“"
        )));
        assert!(needs_body(&h(
            "Facebook <security@facebookmail.com>",
            "You used Facebook to log into Spotify"
        )));
        assert!(needs_body(&h(
            "Discord <noreply@discord.com>",
            "Neue autorisierte App"
        )));
        assert!(needs_body(&h(
            "Slack <feedback@slack.com>",
            "An app was added to your workspace"
        )));
    }

    #[test]
    fn google_body_en() {
        let m = h("Google <no-reply@accounts.google.com>", "Security alert");
        let body = "Security alert\n\nSpotify was granted access to your Google Account\n\nmax@gmail.com\n\nIf you didn't grant access, review it at https://myaccount.google.com/notifications";
        let (p, app, dom) = parse_sso_body(&m, body).unwrap();
        assert_eq!(p, SsoProvider::Google);
        assert_eq!(app, "Spotify");
        assert_eq!(dom, None);
    }

    #[test]
    fn google_body_de_with_url() {
        let m = h(
            "Google <no-reply@accounts.google.com>",
            "Sicherheitswarnung",
        );
        let body = "Sicherheitswarnung\r\n\r\nNotion hat Zugriff auf Ihr Google-Konto erhalten\r\nSiehe https://www.notion.so/login\r\n";
        let (_, app, dom) = parse_sso_body(&m, body).unwrap();
        assert_eq!(app, "Notion");
        assert_eq!(dom.as_deref(), Some("www.notion.so"));
    }

    #[test]
    fn google_body_html() {
        let m = h("Google <no-reply@accounts.google.com>", "Security alert");
        let body = "<html><body><div>Security alert</div><div>Canva was granted access to your Google&nbsp;Account</div><a href=\"https://www.canva.com/\">x</a></body></html>";
        let (_, app, dom) = parse_sso_body(&m, body).unwrap();
        assert_eq!(app, "Canva");
        assert_eq!(dom.as_deref(), Some("www.canva.com"));
    }

    #[test]
    fn google_body_without_app_is_none() {
        let m = h("Google <no-reply@accounts.google.com>", "Security alert");
        assert!(parse_sso_body(&m, "New sign-in on Windows. Check activity.").is_none());
    }

    #[test]
    fn github_body_paren() {
        let m = h(
            "GitHub <noreply@github.com>",
            "[GitHub] A third-party OAuth application has been added to your account",
        );
        let body = "Hey octocat!\n\nA third-party OAuth application (Heroku Dashboard) with repo, user:email scopes was recently authorized to access your account.";
        let (p, app, _) = parse_sso_body(&m, body).unwrap();
        assert_eq!(p, SsoProvider::GitHub);
        assert_eq!(app, "Heroku Dashboard");
    }

    #[test]
    fn github_body_quoted_app() {
        let m = h(
            "GitHub <noreply@github.com>",
            "[GitHub] A new GitHub App has been installed",
        );
        let body = "The GitHub App \"Vercel\" was recently installed on your personal account.";
        assert_eq!(parse_sso_body(&m, body).unwrap().1, "Vercel");
    }

    #[test]
    fn microsoft_body() {
        let m = h(
            "Microsoft account team <account-security-noreply@accountprotection.microsoft.com>",
            "App connected to your Microsoft account",
        );
        let body = "A new app is connected to your Microsoft account.\n\nApp name: Zoom\nPermissions: Read your profile";
        let (p, app, _) = parse_sso_body(&m, body).unwrap();
        assert_eq!(p, SsoProvider::Microsoft);
        assert_eq!(app, "Zoom");
    }

    #[test]
    fn apple_body_de() {
        let m = h(
            "Apple <noreply@email.apple.com>",
            "Du verwendest „Mit Apple anmelden“ für Duolingo",
        );
        let (p, app, _) = parse_sso_body(&m, "Hallo.").unwrap();
        assert_eq!(p, SsoProvider::Apple);
        assert_eq!(app, "Duolingo");
    }

    #[test]
    fn apple_body_quoted_en() {
        let m = h(
            "Apple <appleid@id.apple.com>",
            "You're using Sign in with Apple",
        );
        let body = "You started using Sign in with Apple with \"Strava\".";
        assert_eq!(parse_sso_body(&m, body).unwrap().1, "Strava");
    }

    #[test]
    fn facebook_subject_en_and_de() {
        let m = h(
            "Facebook <security@facebookmail.com>",
            "You used Facebook to log into Spotify",
        );
        let (p, app, _) = parse_sso_body(&m, "").unwrap();
        assert_eq!((p, app.as_str()), (SsoProvider::Facebook, "Spotify"));
        let m = h(
            "Facebook <security@facebookmail.com>",
            "Du hast dich mit Facebook bei Zalando angemeldet",
        );
        assert_eq!(parse_sso_body(&m, "").unwrap().1, "Zalando");
    }

    #[test]
    fn discord_body() {
        let m = h("Discord <noreply@discord.com>", "Authorized app");
        let body = "Authorized app: Spotify\nIf this wasn't you, revoke access.";
        let (p, app, _) = parse_sso_body(&m, body).unwrap();
        assert_eq!((p, app.as_str()), (SsoProvider::Discord, "Spotify"));
    }

    #[test]
    fn app_name_that_is_a_domain() {
        let m = h("Google <no-reply@accounts.google.com>", "Security alert");
        let body = "example-app.io was granted access to your Google Account";
        let (_, app, dom) = parse_sso_body(&m, body).unwrap();
        assert_eq!(app, "example-app.io");
        assert_eq!(dom.as_deref(), Some("example-app.io"));
    }

    #[test]
    fn unrelated_urls_are_not_app_domain() {
        let m = h("Google <no-reply@accounts.google.com>", "Security alert");
        let body = "Spotify was granted access to your Google Account\nhttps://support.google.com/accounts https://tracker.example.net/x";
        assert_eq!(parse_sso_body(&m, body).unwrap().2, None);
    }

    #[test]
    fn classify_sso_builds_grant() {
        let mut m = h("Google <no-reply@accounts.google.com>", "Security alert");
        m.to = "max@gmail.com".into();
        let c = classify_sso(&m, "Spotify was granted access to your Google Account").unwrap();
        assert_eq!(c.kind, EvidenceKind::SsoGrant);
        assert_eq!(c.domain, "unresolved:spotify");
        assert_eq!(c.sso, Some(SsoProvider::Google));
        assert_eq!(c.name.as_deref(), Some("Spotify"));
        assert_eq!(c.identifier.as_deref(), Some("max@gmail.com"));
        let c = classify_sso(
            &m,
            "Dein Foto Studio was granted access to your Google Account",
        )
        .unwrap();
        assert_eq!(c.domain, "unresolved:dein-foto-studio");
    }

    #[test]
    fn categorize_marketing() {
        for s in [
            "20% Rabatt auf alles",
            "Black Friday Deals",
            "Dein Gutschein wartet",
            "Summer SALE starts now",
            "Save 30% today",
            "Jetzt sichern: Angebot der Woche",
            "Soldes d'été",
            "Descuento del 20%",
        ] {
            assert_eq!(
                categorize(&bulk("Shop <news@shop.example>", s)),
                MailCategory::Marketing,
                "{s}"
            );
        }
    }

    #[test]
    fn categorize_social() {
        assert_eq!(
            categorize(&bulk(
                "LinkedIn <updates-noreply@linkedin.com>",
                "Du hast 3 neue Benachrichtigungen"
            )),
            MailCategory::Social
        );
        assert_eq!(
            categorize(&bulk(
                "Facebook <notification@facebookmail.com>",
                "Anna hat dich markiert"
            )),
            MailCategory::Social
        );
    }

    #[test]
    fn categorize_newsletter() {
        assert_eq!(
            categorize(&bulk(
                "Blog <hello@blog.example>",
                "Weekly digest: 5 Artikel"
            )),
            MailCategory::Newsletter
        );
        assert_eq!(
            categorize(&bulk("Autor <autor@substack.com>", "Neuer Beitrag")),
            MailCategory::Newsletter
        );
    }

    #[test]
    fn categorize_transactional() {
        for s in [
            "Ihre Bestellung wurde versendet",
            "Passwort zurücksetzen",
            "Your receipt from Foo",
            "Neue Anmeldung",
        ] {
            assert_eq!(
                categorize(&bulk("Shop <news@shop.example>", s)),
                MailCategory::Transactional,
                "{s}"
            );
        }
        assert_eq!(
            categorize(&bulk("Shop <billing@shop.example>", "Monatsabrechnung")),
            MailCategory::Transactional
        );
    }

    #[test]
    fn categorize_receipt_word_in_promo_is_marketing() {
        assert_eq!(
            categorize(&bulk("Shop <news@shop.example>", "Paket-Deals: 30% Rabatt")),
            MailCategory::Marketing
        );
    }

    #[test]
    fn categorize_spam_and_unknown() {
        assert_eq!(
            categorize(&bulk("x <a@spam.example>", "Online Casino Bonus")),
            MailCategory::Spam
        );
        let mut m = bulk("x <a@spam.example>", "Hi");
        m.precedence = Some("junk".into());
        assert_eq!(categorize(&m), MailCategory::Spam);
        assert_eq!(
            categorize(&bulk("Foo <a@foo.example>", "Ein Update für dich")),
            MailCategory::Unknown
        );
    }

    #[test]
    fn spam_word_requires_word_boundary() {
        assert_eq!(
            categorize(&bulk("Foo <a@foo.example>", "Essex travel update")),
            MailCategory::Unknown
        );
    }

    #[test]
    fn sale_inside_word_is_not_marketing() {
        assert_eq!(
            categorize(&bulk("Foo <a@foo.example>", "Wholesale partners meeting")),
            MailCategory::Unknown
        );
    }

    #[test]
    fn has_word_behaviour() {
        assert!(has_word("online casino bonus", "casino"));
        assert!(!has_word("casinoroyal", "casino"));
        assert!(has_word("you have won a prize", "you have won"));
    }

    #[test]
    fn host_is_matches_subdomains_only() {
        assert!(host_is("mail.linkedin.com", "linkedin.com"));
        assert!(host_is("linkedin.com", "linkedin.com"));
        assert!(!host_is("notlinkedin.com", "linkedin.com"));
    }
}
