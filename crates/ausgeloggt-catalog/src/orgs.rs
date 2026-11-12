//! Curated reference data beyond per-service deletion info:
//! 2FA support (2fa.directory), credit bureaus / address brokers / opt-out lists,
//! German data-protection authorities, search-engine removal links, payment hubs.
//! All bundled (offline); sources + licenses in `data/ATTRIBUTION.md`.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TwoFactorInfo {
    /// e.g. ["totp", "sms", "u2f", "email", "custom-software"] as in 2fa.directory.
    pub methods: Vec<String>,
    /// Link to the service's 2FA documentation.
    pub documentation: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrgKind {
    /// Auskunftei (SCHUFA, Boniversum/Creditreform, CRIF, Experian, infoscore ...).
    CreditBureau,
    /// Adresshändler / list brokers (Deutsche Post Direkt, AZ Direct, Schober ...).
    AddressBroker,
    /// Opt-out list (Robinsonliste) — sign-up link only.
    OptOutList,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub legal_name: String,
    pub kind: OrgKind,
    pub address_lines: Vec<String>,
    pub email: Option<String>,
    /// Official online form for self-disclosure / objection, if any.
    pub web_form: Option<String>,
    /// German one-liner: what they hold and what to request.
    pub description_de: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Authority {
    /// ISO 3166-2 code (`DE-BY`) or `DE` for the federal BfDI.
    pub state: String,
    pub name: String,
    pub address_lines: Vec<String>,
    pub email: Option<String>,
    /// Online complaint form.
    pub complaint_form: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelpLink {
    pub title_de: String,
    pub url: String,
    pub description_de: String,
}

const BUNDLED_2FA: &str = include_str!("../data/2fa.json");

fn two_factor_map() -> &'static HashMap<String, TwoFactorInfo> {
    static MAP: OnceLock<HashMap<String, TwoFactorInfo>> = OnceLock::new();
    MAP.get_or_init(|| serde_json::from_str(BUNDLED_2FA).unwrap_or_default())
}

/// 2FA support for a canonical registrable domain (bundled 2fa.directory snapshot,
/// additional domains already expanded). Subdomains (`gist.github.com`) fall back
/// to their parent domains.
pub fn two_factor(domain: &str) -> Option<&'static TwoFactorInfo> {
    let d = domain.trim().trim_end_matches('.').to_lowercase();
    let map = two_factor_map();
    let mut cur = d.as_str();
    loop {
        if let Some(info) = map.get(cur) {
            return Some(info);
        }
        match cur.split_once('.') {
            Some((_, rest)) if rest.contains('.') => cur = rest,
            _ => return None,
        }
    }
}

/// Domains known to support any 2FA method (for `insights::RiskContext`).
pub fn two_factor_domains() -> Vec<String> {
    let mut v: Vec<String> = two_factor_map().keys().cloned().collect();
    v.sort();
    v
}

fn lines(s: &[&str]) -> Vec<String> {
    s.iter().map(|x| x.to_string()).collect()
}

fn org(
    (id, name, legal, kind): (&str, &str, &str, OrgKind),
    addr: &[&str],
    email: Option<&str>,
    form: Option<&str>,
    desc: &str,
) -> Organization {
    Organization {
        id: id.into(),
        name: name.into(),
        legal_name: legal.into(),
        kind,
        address_lines: lines(addr),
        email: email.map(Into::into),
        web_form: form.map(Into::into),
        description_de: desc.into(),
    }
}

/// Curated credit bureaus, address brokers and opt-out lists (Germany first; addresses
/// cross-checked against the Datenanfragen.de database, sources in ATTRIBUTION.md).
pub fn organizations() -> &'static [Organization] {
    static V: OnceLock<Vec<Organization>> = OnceLock::new();
    V.get_or_init(|| {
        use OrgKind::*;
        vec![
            org(
                ("schufa", "SCHUFA", "SCHUFA Holding AG", CreditBureau),
                &["SCHUFA Holding AG", "Verbraucherservice", "Postfach 10 34 41", "50474 Köln"],
                Some("datenschutz@schufa.de"),
                Some("https://www.meineschufa.de/de/datenkopie"),
                "Auskunftei mit Bonitätsdaten; kostenlose Datenkopie nach Art. 15 DS-GVO anfordern und falsche Einträge berichtigen lassen.",
            ),
            org(
                ("boniversum", "Boniversum", "Creditreform Boniversum GmbH", CreditBureau),
                &["Creditreform Boniversum GmbH", "Hellersbergstraße 11", "41460 Neuss"],
                None,
                Some("https://www.boniversum.de/selbstauskunft/"),
                "Auskunftei der Creditreform-Gruppe; kostenlose Selbstauskunft (Datenkopie) für Verbraucher.",
            ),
            org(
                ("crif", "CRIF", "CRIF GmbH", CreditBureau),
                &["CRIF GmbH", "Leopoldstraße 244", "80807 München"],
                Some("datenschutz.de@crif.com"),
                None,
                "Auskunftei (ehemals Bürgel); Auskunft nach Art. 15 DS-GVO schriftlich oder per E-Mail anfordern.",
            ),
            org(
                ("experian", "Experian", "Experian GmbH", CreditBureau),
                &["Experian GmbH", "Rheinstraße 99", "76532 Baden-Baden"],
                Some("businessdach@experian.com"),
                None,
                "Wirtschaftsauskunftei; Auskunft nach Art. 15 DS-GVO anfordern.",
            ),
            org(
                ("infoscore", "infoscore / Riverty", "infoscore Consumer Data GmbH", CreditBureau),
                &["infoscore Consumer Data GmbH", "Rheinstraße 99", "76532 Baden-Baden"],
                Some("ICD-Datenschutz@experian.com"),
                None,
                "Auskunftei (heute zu Experian gehörend); Selbstauskunft anfordern. Inkasso/Zahlungsgeschäft läuft unter Riverty.",
            ),
            org(
                ("deutsche-post-direkt", "Deutsche Post Direkt", "Deutsche Post Direkt GmbH", AddressBroker),
                &["Deutsche Post Direkt GmbH", "Junkersring 57", "53844 Troisdorf"],
                Some("datenschutz@postdirekt.de"),
                None,
                "Adresshändler der Deutschen Post; Auskunft über gespeicherte Adress-/Zielgruppendaten verlangen und Werbewidersprüche einlegen.",
            ),
            org(
                ("az-direct", "AZ Direct", "AZ Direct GmbH", AddressBroker),
                &["AZ Direct GmbH", "Carl-Bertelsmann-Straße 161S", "33311 Gütersloh"],
                Some("datenschutz@az-direct.com"),
                None,
                "Adress- und Datenhändler (Arvato/Bertelsmann); Auskunft und Löschung bzw. Werbewiderspruch verlangen.",
            ),
            org(
                ("schober", "Schober / Capaneo", "Capaneo GmbH (vormals Schober Information Group)", AddressBroker),
                &["Capaneo GmbH", "Meisenweg 37", "70771 Leinfelden-Echterdingen"],
                Some("datenschutz@capaneo.de"),
                None,
                "Adress- und Zielgruppenhändler (Schober-Gruppe); Auskunft und Löschung bzw. Werbewiderspruch verlangen.",
            ),
            org(
                ("acxiom", "Acxiom", "Acxiom Deutschland GmbH", AddressBroker),
                &["Acxiom Deutschland GmbH", "Speicherstraße 57-59", "60327 Frankfurt am Main"],
                Some("datenschutz@acxiom.com"),
                None,
                "Datenhändler für Marketingprofile; Auskunft, Löschung und Werbewiderspruch verlangen.",
            ),
            org(
                ("robinsonliste", "Robinsonliste", "Deutscher Dialogmarketing Verband e.V. (DDV)", OptOutList),
                &["Deutscher Dialogmarketing Verband e.V.", "Robinsonliste", "Hahnstraße 70", "60528 Frankfurt am Main"],
                None,
                Some("https://www.robinsonliste.de/"),
                "Kostenlose Sperrliste gegen adressierte Werbung bei teilnehmenden Unternehmen; nur Eintragung, kein Löschantrag.",
            ),
        ]
    })
}

fn auth(state: &str, name: &str, addr: &[&str], email: &str, form: &str) -> Authority {
    Authority {
        state: state.into(),
        name: name.into(),
        address_lines: lines(addr),
        email: Some(email.into()),
        complaint_form: Some(form.into()),
    }
}

/// The 16 state authorities + BfDI with postal address and complaint form.
/// For Bavaria the BayLDA (private sector) is listed; the BayLfD only covers public bodies.
pub fn authorities() -> &'static [Authority] {
    static V: OnceLock<Vec<Authority>> = OnceLock::new();
    V.get_or_init(|| {
        vec![
            auth("DE", "Die Bundesbeauftragte für den Datenschutz und die Informationsfreiheit (BfDI)",
                &["Graurheindorfer Straße 153", "53117 Bonn"], "poststelle@bfdi.bund.de",
                "https://www.bfdi.bund.de/SharedDocs/Downloads/DE/DokumenteBfDI/Formular-Beschwerde.html"),
            auth("DE-BW", "Der Landesbeauftragte für den Datenschutz und die Informationsfreiheit Baden-Württemberg",
                &["Heilbronner Straße 35", "70191 Stuttgart"], "poststelle@lfdi.bwl.de",
                "https://www.baden-wuerttemberg.datenschutz.de/beschwerde/"),
            auth("DE-BY", "Bayerisches Landesamt für Datenschutzaufsicht (BayLDA)",
                &["Promenade 18", "91522 Ansbach"], "poststelle@lda.bayern.de",
                "https://www.lda.bayern.de/de/beschwerde.html"),
            auth("DE-BE", "Berliner Beauftragte für Datenschutz und Informationsfreiheit",
                &["Alt-Moabit 59-61", "10555 Berlin"], "mailbox@datenschutz-berlin.de",
                "https://www.datenschutz-berlin.de/beschwerde"),
            auth("DE-BB", "Die Landesbeauftragte für den Datenschutz und für das Recht auf Akteneinsicht Brandenburg",
                &["Stahnsdorfer Damm 77", "14532 Kleinmachnow"], "poststelle@lda.brandenburg.de",
                "https://www.lda.brandenburg.de/lda/de/service/formulare-und-musterschreiben/beschwerdeformular/"),
            auth("DE-HB", "Der Landesbeauftragte für Datenschutz und Informationsfreiheit der Freien Hansestadt Bremen",
                &["Georgstraße 122-124", "27570 Bremerhaven"], "office@datenschutz.bremen.de",
                "https://www.datenschutz.bremen.de/wir-ueber-uns/online-meldungen/beschwerdeformular-15253"),
            auth("DE-HH", "Der Hamburgische Beauftragte für Datenschutz und Informationsfreiheit",
                &["Ludwig-Erhard-Straße 22, 7. OG", "20459 Hamburg"], "mailbox@datenschutz.hamburg.de",
                "https://datenschutz-hamburg.de/service-information/beschwerde-oder-hinweis-einreichen"),
            auth("DE-HE", "Der Hessische Beauftragte für Datenschutz und Informationsfreiheit",
                &["Wilhelmstraße 7", "65185 Wiesbaden"], "poststelle@datenschutz.hessen.de",
                "https://datenschutz.hessen.de/service/beschwerde-uebermitteln"),
            auth("DE-MV", "Der Landesbeauftragte für Datenschutz und Informationsfreiheit Mecklenburg-Vorpommern",
                &["Schloss Schwerin", "Lennéstraße 1", "19053 Schwerin"], "info@datenschutz-mv.de",
                "https://www.datenschutz-mv.de/kontakt/online-beschwerdeformular-mitteilung-ueber-einen-datenschutzverstoss/"),
            auth("DE-NI", "Die Landesbeauftragte für den Datenschutz Niedersachsen",
                &["Prinzenstraße 5", "30159 Hannover"], "poststelle@lfd.niedersachsen.de",
                "https://www.lfd.niedersachsen.de/startseite/meldeformulare/online_beschwerde/beschwerdeformular-191364.html"),
            auth("DE-NW", "Landesbeauftragte für Datenschutz und Informationsfreiheit Nordrhein-Westfalen",
                &["Kavalleriestraße 2-4", "40213 Düsseldorf"], "poststelle@ldi.nrw.de",
                "https://www.ldi.nrw.de/beschwerde"),
            auth("DE-RP", "Der Landesbeauftragte für den Datenschutz und die Informationsfreiheit Rheinland-Pfalz",
                &["Hintere Bleiche 34", "55116 Mainz"], "poststelle@datenschutz.rlp.de",
                "https://www.datenschutz.rlp.de/themen/online-services/beschwerdeformular"),
            auth("DE-SL", "Unabhängiges Datenschutzzentrum Saarland",
                &["Fritz-Dobisch-Straße 12", "66111 Saarbrücken"], "poststelle@datenschutz.saarland.de",
                "https://www.datenschutz.saarland.de/online-dienste/beschwerde-kontrollanregung/beschwerdeformular"),
            auth("DE-SN", "Sächsische Datenschutz- und Transparenzbeauftragte",
                &["Maternistraße 17", "01067 Dresden"], "post@sdtb.sachsen.de",
                "https://www.datenschutz.sachsen.de/beschwerde-einreichen.html"),
            auth("DE-ST", "Landesbeauftragter für den Datenschutz Sachsen-Anhalt",
                &["Otto-von-Guericke-Straße 34a", "39104 Magdeburg"], "poststelle@lfd.sachsen-anhalt.de",
                "https://datenschutz.sachsen-anhalt.de/service/online-formulare/beschwerde"),
            auth("DE-SH", "Unabhängiges Landeszentrum für Datenschutz Schleswig-Holstein (ULD)",
                &["Holstenstraße 98", "24103 Kiel"], "mail@datenschutzzentrum.de",
                "https://www.datenschutzzentrum.de/formular/beschwerde.php"),
            auth("DE-TH", "Thüringer Landesbeauftragter für den Datenschutz und die Informationsfreiheit",
                &["Häßlerstraße 8", "99096 Erfurt"], "poststelle@datenschutz.thueringen.de",
                "https://tlfdi.de/kontakt#c1812"),
        ]
    })
}

/// Coarse PLZ -> state table keyed by the first two digits. APPROXIMATE: many border
/// regions straddle states (e.g. 63xxx HE/BY, 89xxx BW/BY, 19xxx MV/BB, 27xxx NI/HB);
/// the UI must let the user override the choice.
fn state_for_plz(plz: &str) -> Option<&'static str> {
    let plz = plz.trim();
    if plz.len() != 5 || !plz.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // Bremerhaven exception inside the 27xxx (Niedersachsen) block.
    if plz.starts_with("275") {
        return Some("DE-HB");
    }
    let two: u32 = plz[..2].parse().ok()?;
    Some(match two {
        1 | 2 | 4 | 8 | 9 => "DE-SN",
        3 | 14..=16 => "DE-BB",
        5 => "DE-SN",
        6 => "DE-ST",
        7 | 98 | 99 => "DE-TH",
        10..=13 => "DE-BE",
        17..=19 => "DE-MV",
        20..=22 => "DE-HH",
        23..=25 => "DE-SH",
        26 | 27 | 29 | 30 | 31 | 37 | 38 | 49 => "DE-NI",
        28 => "DE-HB",
        32 | 33 | 40..=48 | 50..=53 | 57..=59 => "DE-NW",
        34..=36 | 60..=65 => "DE-HE",
        39 => "DE-ST",
        54..=56 | 67 => "DE-RP",
        66 => "DE-SL",
        68..=79 | 88 | 89 => "DE-BW",
        80..=87 | 90..=97 => "DE-BY",
        _ => return None,
    })
}

/// Competent authority for a state code (`DE-BY`), or by German postal code
/// (coarse, approximate leading-digits table) when `state` is empty.
pub fn authority_for(state: &str, postal_code: &str) -> Option<&'static Authority> {
    let want = {
        let s = state.trim().to_uppercase();
        if s.is_empty() {
            state_for_plz(postal_code)?.to_string()
        } else {
            s
        }
    };
    authorities().iter().find(|a| a.state == want)
}

fn link(title: &str, url: &str, desc: &str) -> HelpLink {
    HelpLink {
        title_de: title.into(),
        url: url.into(),
        description_de: desc.into(),
    }
}

/// Search engines / people-search removal (Google "Ergebnisse über dich", Google
/// right-to-be-forgotten form, Bing, DuckDuckGo info, ...).
pub fn search_removal_links() -> &'static [HelpLink] {
    static V: OnceLock<Vec<HelpLink>> = OnceLock::new();
    V.get_or_init(|| {
        vec![
            link("Google: Ergebnisse über dich", "https://myactivity.google.com/results-about-you",
                "Findet Suchergebnisse mit deiner Adresse, Telefonnummer oder E-Mail und bittet Google, sie zu entfernen."),
            link("Google: Antrag auf Entfernung (Recht auf Vergessenwerden)", "https://reportcontent.google.com/forms/rtbf",
                "EU-Formular, um Links mit veralteten oder unangemessenen Informationen über dich aus der Google-Suche löschen zu lassen."),
            link("Google: Persönliche Informationen und Bilder entfernen", "https://support.google.com/websearch/troubleshooter/3111061",
                "Hilfeassistent für Bilder, Doxxing, nicht einvernehmliche Inhalte und andere Entfernungsanträge."),
            link("Bing: Recht auf Vergessenwerden", "https://www.bing.com/webmaster/tools/eu-privacy-request",
                "Formular von Microsoft, um Suchergebnisse in Bing entfernen zu lassen."),
            link("DuckDuckGo: Hinweis zur Entfernung", "https://duckduckgo.com/duckduckgo-help-pages/",
                "DuckDuckGo bezieht Ergebnisse u. a. von Bing; Entfernung daher bei Bing und an der Quellseite beantragen."),
            link("Google-Konto: Daten & Datenschutz", "https://myaccount.google.com/data-and-privacy",
                "Zentrale Übersicht über gespeicherte Aktivitäten, Standortverlauf und Datenschutzeinstellungen."),
        ]
    })
}

/// Central places where payment methods / authorisations live.
/// Note: the Chrome entry is a browser-internal `chrome://` URL, not a clickable web link.
pub fn payment_hubs() -> &'static [HelpLink] {
    static V: OnceLock<Vec<HelpLink>> = OnceLock::new();
    V.get_or_init(|| {
        vec![
            link("PayPal: Automatische Zahlungen", "https://www.paypal.com/myaccount/autopay/",
                "Genehmigte Abbuchungen und Abos widerrufen."),
            link("Apple: Abonnements", "https://apps.apple.com/account/subscriptions",
                "Alle über die Apple-ID abgeschlossenen Abos verwalten und kündigen."),
            link("Apple: Zahlungsmethoden", "https://account.apple.com/",
                "Apple-Account: Zahlungsmethoden und Rechnungsdaten bearbeiten."),
            link("Google Play: Abos", "https://play.google.com/store/account/subscriptions",
                "Abos über Google Play ansehen und kündigen."),
            link("Google Pay: Zahlungsmethoden", "https://pay.google.com/gp/w/home/paymentmethods",
                "Gespeicherte Karten und Konten bei Google entfernen."),
            link("Amazon: Zahlungsarten (Wallet)", "https://www.amazon.de/cpe/yourpayments/wallet",
                "Gespeicherte Karten und Bankkonten bei Amazon verwalten."),
            link("Klarna", "https://app.klarna.com/",
                "Klarna-Konto: Zahlungsarten und offene Abbuchungen prüfen."),
            link("Shop Pay", "https://shop.app/account",
                "Bei Shopify-Shops hinterlegte Karten und Adressen verwalten."),
            link("Stripe Link", "https://link.com/",
                "Link-Konto mit gespeicherten Zahlungsdaten verwalten oder löschen."),
            link("Chrome: Gespeicherte Zahlungsmethoden", "chrome://settings/payments",
                "Browser-interne Adresse: in die Chrome-Adressleiste kopieren; sie lässt sich nicht als Link anklicken."),
            link("Safari / Apple Wallet: Automatisches Ausfüllen", "https://support.apple.com/de-de/guide/safari/ibrw1103/mac",
                "Gespeicherte Kreditkarten für Safari-AutoFill ansehen und entfernen."),
            link("Microsoft-Konto: Zahlung & Abrechnung", "https://account.microsoft.com/billing/payments",
                "Zahlungsoptionen und Abos des Microsoft-Kontos verwalten."),
        ]
    })
}

/// Alias services for the alias recommendations and their relay domains.
pub fn alias_services() -> &'static [(HelpLink, &'static [&'static str])] {
    static V: OnceLock<Vec<(HelpLink, &'static [&'static str])>> = OnceLock::new();
    V.get_or_init(|| {
        vec![
            (
                link(
                    "Apple: E-Mail-Adresse verbergen",
                    "https://support.apple.com/de-de/102567",
                    "Zufällige Weiterleitungsadressen für Apple-Account-Inhaber (iCloud+).",
                ),
                &["privaterelay.appleid.com"][..],
            ),
            (
                link(
                    "SimpleLogin",
                    "https://simplelogin.io/",
                    "Open-Source-Aliasdienst (Proton) mit eigenen Domains.",
                ),
                &[
                    "simplelogin.com",
                    "simplelogin.co",
                    "aleeas.com",
                    "slmail.me",
                    "simplelogin.fr",
                    "8alias.com",
                    "dralias.com",
                ][..],
            ),
            (
                link(
                    "addy.io",
                    "https://addy.io/",
                    "Open-Source-Aliasdienst (ehemals AnonAddy).",
                ),
                &["anonaddy.com", "anonaddy.me", "addy.io"][..],
            ),
            (
                link(
                    "Firefox Relay",
                    "https://relay.firefox.com/",
                    "E-Mail-Masken von Mozilla.",
                ),
                &["mozmail.com"][..],
            ),
            (
                link(
                    "DuckDuckGo Email Protection",
                    "https://duckduckgo.com/email/",
                    "@duck.com-Adressen, die Tracker entfernen und weiterleiten.",
                ),
                &["duck.com"][..],
            ),
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_factor_lookups() {
        assert!(two_factor("github.com").is_some());
        assert!(two_factor("GitHub.com").is_some());
        assert!(two_factor("gist.github.com").is_some());
        assert!(two_factor("youtube.com").is_some());
        assert!(two_factor("no-such-domain-xyz.invalid").is_none());
        assert!(two_factor_domains().len() > 1000);
        assert!(!two_factor("github.com").unwrap().methods.is_empty());
    }

    #[test]
    fn authorities_complete() {
        assert_eq!(authorities().len(), 17);
        for a in authorities() {
            assert!(
                !a.address_lines.is_empty() && a.email.is_some(),
                "{}",
                a.state
            );
            assert!(a.complaint_form.as_deref().unwrap().starts_with("https://"));
        }
        assert_eq!(authority_for("DE-BY", "").unwrap().state, "DE-BY");
        assert!(authority_for("DE-BY", "").unwrap().name.contains("BayLDA"));
        for (plz, st) in [
            ("80331", "DE-BY"),
            ("10115", "DE-BE"),
            ("20095", "DE-HH"),
            ("50667", "DE-NW"),
            ("01067", "DE-SN"),
            ("70173", "DE-BW"),
            ("28195", "DE-HB"),
        ] {
            assert_eq!(authority_for("", plz).unwrap().state, st, "{plz}");
        }
        assert!(authority_for("", "abc").is_none());
    }

    #[test]
    fn orgs_have_addresses() {
        assert!(organizations().len() >= 10);
        for o in organizations() {
            assert!(
                !o.address_lines.is_empty() && !o.description_de.is_empty(),
                "{}",
                o.id
            );
        }
        assert!(organizations()
            .iter()
            .any(|o| o.kind == OrgKind::OptOutList));
    }

    #[test]
    fn links_are_https() {
        for l in search_removal_links().iter().chain(payment_hubs()) {
            assert!(
                l.url.starts_with("https://") || l.url.starts_with("chrome://"),
                "{}",
                l.url
            );
        }
        for (l, d) in alias_services() {
            assert!(l.url.starts_with("https://"));
            assert!(!d.is_empty());
        }
        for o in organizations() {
            if let Some(f) = &o.web_form {
                assert!(f.starts_with("https://"));
            }
        }
    }
}
