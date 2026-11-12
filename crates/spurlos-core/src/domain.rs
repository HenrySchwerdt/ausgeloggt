//! Domain normalisation: hosts / URLs / e-mail addresses -> canonical registrable domain.

/// Normalise any host, URL or e-mail address to its registrable domain
/// ("eTLD+1", using the Public Suffix List), lowercase, without `www.`.
///
/// - `https://accounts.spotify.com/de/login` -> `spotify.com`
/// - `noreply@mail.spotify.com` -> `spotify.com`
/// - `"Spotify" <no-reply@spotify.com>` -> `spotify.com`
/// - `shop.example.co.uk` -> `example.co.uk`
/// - `android://<hash>@com.spotify.music/` (Chrome Android entries) -> `spotify.com`
///   (reverse the package name, take eTLD+1) if plausible
///
/// Returns `None` for IPs, localhost, empty or unparsable input.
pub fn registrable_domain(input: &str) -> Option<String> {
    let mut s = input.trim();
    if s.is_empty() || s.len() > 2048 {
        return None;
    }

    // RFC 5322 display-name form: take the part inside the last <...>.
    if let (Some(open), Some(close)) = (s.rfind('<'), s.rfind('>')) {
        if open < close {
            s = s[open + 1..close].trim();
        }
    }
    let s = s
        .trim_matches(|c| c == '"' || c == '\'' || c == '<' || c == '>')
        .trim();
    if s.is_empty() || s.chars().any(char::is_whitespace) {
        return None;
    }

    // Chrome Android: android://<hash>@com.spotify.music/
    if let Some(rest) = strip_prefix_ci(s, "android://") {
        let auth = rest.split(['/', '?', '#']).next().unwrap_or("");
        let package = auth.rsplit('@').next().unwrap_or("");
        let reversed: Vec<&str> = package.split('.').rev().collect();
        if reversed.len() < 2 {
            return None;
        }
        return from_host(&reversed.join("."));
    }

    let s = strip_prefix_ci(s, "mailto:").unwrap_or(s);
    let s = match s.find("://") {
        Some(i) => &s[i + 3..],
        None => s.strip_prefix("//").unwrap_or(s),
    };
    // Authority = up to the first path / query / fragment delimiter.
    let authority = s.split(['/', '?', '#']).next().unwrap_or("");
    // Drop userinfo (also covers plain e-mail addresses).
    let hostport = authority.rsplit('@').next().unwrap_or("");
    if hostport.starts_with('[') {
        return None; // IPv6 literal
    }
    let host = match hostport.rsplit_once(':') {
        Some((h, port)) if port.chars().all(|c| c.is_ascii_digit()) => h,
        Some(_) => return None,
        None => hostport,
    };
    from_host(host)
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len()
        && s.is_char_boundary(prefix.len())
        && s[..prefix.len()].eq_ignore_ascii_case(prefix)
    {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

fn from_host(host: &str) -> Option<String> {
    let host = host.trim().trim_end_matches('.').to_lowercase();
    if host.is_empty() || host.len() > 253 || host == "localhost" {
        return None;
    }
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2 {
        return None;
    }
    for l in &labels {
        if l.is_empty()
            || l.len() > 63
            || l.starts_with('-')
            || l.ends_with('-')
            || !l
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return None;
        }
    }
    // IPv4 (all-numeric TLD) or similar: not a domain.
    if labels
        .last()
        .is_some_and(|l| l.chars().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    let domain = psl::domain_str(&host)?;
    if !domain.contains('.') {
        return None;
    }
    Some(domain.to_string())
}

/// Alias table: (alias, canonical). Both are registrable domains.
///
/// Microsoft: `microsoftonline.com` (the Entra / Microsoft-account login host) maps to
/// `microsoft.com`. `outlook.com`, `hotmail.*` and `live.*` are mailbox providers and are
/// treated as infrastructure (see [`is_infrastructure_domain`]); they are deliberately
/// *not* aliased, so a personal mail from `x@outlook.com` never creates an account.
/// YouTube stays separate from Google (own settings/deletion flow).
/// Infrastructure is checked on the registrable domain *before* aliasing, so
/// `googlemail.com` (a mailbox provider) is dropped by merge when it is a sender,
/// while the alias still helps for service-style uses.
const ALIASES: &[(&str, &str)] = &[
    ("googlemail.com", "google.com"),
    ("facebookmail.com", "facebook.com"),
    ("fb.com", "facebook.com"),
    ("fb.me", "facebook.com"),
    ("messenger.com", "facebook.com"),
    ("instagrammail.com", "instagram.com"),
    ("twitter.com", "x.com"),
    ("t.co", "x.com"),
    ("spotifymail.com", "spotify.com"),
    ("scdn.co", "spotify.com"),
    ("steampowered.com", "steamcommunity.com"),
    ("linkedinmail.com", "linkedin.com"),
    ("paypal-communication.com", "paypal.com"),
    ("paypal-mail.com", "paypal.com"),
    ("microsoftonline.com", "microsoft.com"),
    ("microsoftonline-p.com", "microsoft.com"),
    ("xboxlive.com", "xbox.com"),
    ("redditmail.com", "reddit.com"),
    ("discordapp.com", "discord.com"),
    ("pinterestmail.com", "pinterest.com"),
    ("airbnbmail.com", "airbnb.com"),
    ("dropboxmail.com", "dropbox.com"),
    ("ebay-kleinanzeigen.de", "kleinanzeigen.de"),
    ("zalando-lounge.de", "zalando.de"),
    ("adobesign.com", "adobe.com"),
    ("slackhq.com", "slack.com"),
    ("twitchmail.com", "twitch.tv"),
];

/// Map aliases so different domains of the same service merge into one account,
/// e.g. `googlemail.com` -> `google.com`, `spotifymail.com` -> `spotify.com`,
/// `facebookmail.com` -> `facebook.com`, `twitter.com` -> `x.com`,
/// `steampowered.com` -> `steamcommunity.com`... Country domains of shops
/// (`amazon.de` vs `amazon.com`) stay separate, they are separate accounts.
/// Input and output are registrable domains.
pub fn canonical_service_domain(registrable: &str) -> String {
    let d = registrable.trim().to_lowercase();
    for (alias, canonical) in ALIASES {
        if d == *alias {
            return (*canonical).to_string();
        }
    }
    d
}

const INFRA_EXACT: &[&str] = &[
    // ESPs / transactional mail
    "sendgrid.net",
    "sendgrid.com",
    "mailchimp.com",
    "mcsv.net",
    "mcdlv.net",
    "list-manage.com",
    "rsgsv.net",
    "amazonses.com",
    "mailgun.org",
    "mailgun.net",
    "mailgun.com",
    "sparkpostmail.com",
    "mandrillapp.com",
    "mandrill.com",
    "exacttarget.com",
    "marketingcloudapis.com",
    "klaviyomail.com",
    "klaviyo.com",
    "hubspotemail.net",
    "hubspotlinks.com",
    "sendinblue.com",
    "brevo.com",
    "sibmail.com",
    "mailjet.com",
    "mjt.lu",
    "postmarkapp.com",
    "pm-bounces.com",
    "createsend.com",
    "campaign-archive.com",
    "constantcontact.com",
    "ctctcdn.com",
    "rs6.net",
    "getresponse.com",
    "emarsys.net",
    "cleverreach.com",
    "cleverreach.de",
    "rapidmail.com",
    "newsletter2go.com",
    "nl2go.com",
    "inxmail.com",
    "inxmail.de",
    "optivo.com",
    "mailingwork.de",
    "responsys.net",
    "eloqua.com",
    "mktomail.com",
    "customeriomail.com",
    "mailerlite.com",
    "mlsend.com",
    "convertkit.com",
    "substack.com",
    "beehiiv.com",
    "intercom-mail.com",
    "zendesk.com",
    "freshdesk.com",
    "braze.com",
    "appboy.com",
    "sailthru.com",
    "sendpulse.com",
    "elasticemail.com",
    "mimecast.com",
    "pphosted.com",
    // free-mail / mailbox providers
    "gmail.com",
    "googlemail.com",
    "gmx.de",
    "gmx.net",
    "gmx.at",
    "gmx.ch",
    "gmx.com",
    "web.de",
    "t-online.de",
    "outlook.com",
    "icloud.com",
    "me.com",
    "mac.com",
    "posteo.de",
    "posteo.net",
    "mailbox.org",
    "proton.me",
    "protonmail.com",
    "protonmail.ch",
    "pm.me",
    "aol.com",
    "freenet.de",
    "online.de",
    "arcor.de",
    "yandex.ru",
    "yandex.com",
    "mail.ru",
    "zoho.com",
    "fastmail.com",
    "fastmail.fm",
    "tutanota.com",
    "tuta.com",
    "tutamail.com",
    "tuta.io",
    "privaterelay.appleid.com",
];

/// Brand names that exist under many TLDs and are all mailbox providers.
const INFRA_BRANDS: &[&str] = &["yahoo", "hotmail", "live", "ymail", "rocketmail", "msn"];

/// Mail / infrastructure domains that must never become an account by themselves
/// (e.g. `sendgrid.net`, `mailchimp.com`, `mcsv.net`, `amazonses.com`,
/// `mailgun.org`, `gmail.com`, `gmx.de`, `web.de`, `outlook.com` as *sender*
/// of personal mails, `privaterelay.appleid.com`).
pub fn is_infrastructure_domain(registrable: &str) -> bool {
    let d = registrable.trim().trim_end_matches('.').to_lowercase();
    if d.is_empty() {
        return false;
    }
    if INFRA_EXACT.contains(&d.as_str()) {
        return true;
    }
    let Some((label, rest)) = d.split_once('.') else {
        return false;
    };
    if INFRA_BRANDS.contains(&label) && !rest.is_empty() {
        return true;
    }
    // Numbered hosts: Campaign Monitor (cmail19.com), Responsys (rsys2.net),
    // Sendinblue (sendibt3.com), SparkPost (sparkpostmail1.com), createsend1.com.
    let numbered = |prefix: &str, tld: &str| {
        rest == tld
            && label
                .strip_prefix(prefix)
                .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    };
    numbered("cmail", "com")
        || numbered("rsys", "net")
        || numbered("sendibt", "com")
        || numbered("sendibm", "com")
        || numbered("sparkpostmail", "com")
        || numbered("createsend", "com")
}

const BRANDS: &[(&str, &str)] = &[
    ("spotify.com", "Spotify"),
    ("google.com", "Google"),
    ("youtube.com", "YouTube"),
    ("facebook.com", "Facebook"),
    ("instagram.com", "Instagram"),
    ("x.com", "X (Twitter)"),
    ("linkedin.com", "LinkedIn"),
    ("github.com", "GitHub"),
    ("gitlab.com", "GitLab"),
    ("paypal.com", "PayPal"),
    ("paypal.de", "PayPal"),
    ("amazon.de", "Amazon Deutschland"),
    ("amazon.com", "Amazon"),
    ("amazon.co.uk", "Amazon UK"),
    ("ebay.de", "eBay"),
    ("ebay.com", "eBay"),
    ("kleinanzeigen.de", "Kleinanzeigen"),
    ("netflix.com", "Netflix"),
    ("disneyplus.com", "Disney+"),
    ("microsoft.com", "Microsoft"),
    ("apple.com", "Apple"),
    ("dropbox.com", "Dropbox"),
    ("discord.com", "Discord"),
    ("reddit.com", "Reddit"),
    ("twitch.tv", "Twitch"),
    ("tiktok.com", "TikTok"),
    ("pinterest.com", "Pinterest"),
    ("snapchat.com", "Snapchat"),
    ("whatsapp.com", "WhatsApp"),
    ("steamcommunity.com", "Steam"),
    ("epicgames.com", "Epic Games"),
    ("xbox.com", "Xbox"),
    ("playstation.com", "PlayStation"),
    ("adobe.com", "Adobe"),
    ("notion.so", "Notion"),
    ("slack.com", "Slack"),
    ("zoom.us", "Zoom"),
    ("airbnb.com", "Airbnb"),
    ("booking.com", "Booking.com"),
    ("uber.com", "Uber"),
    ("deutschebahn.com", "Deutsche Bahn"),
    ("bahn.de", "Deutsche Bahn"),
    ("lieferando.de", "Lieferando"),
    ("otto.de", "Otto"),
    ("zalando.de", "Zalando"),
    ("zalando.com", "Zalando"),
    ("sparkasse.de", "Sparkasse"),
    ("dhl.de", "DHL"),
    ("dhl.com", "DHL"),
    ("dpd.de", "DPD"),
    ("hermesworld.com", "Hermes"),
    ("ikea.com", "IKEA"),
    ("mediamarkt.de", "MediaMarkt"),
    ("saturn.de", "Saturn"),
    ("lidl.de", "Lidl"),
    ("rewe.de", "REWE"),
    ("check24.de", "CHECK24"),
    ("dm.de", "dm"),
    ("telekom.de", "Telekom"),
    ("vodafone.de", "Vodafone"),
    ("ing.de", "ING"),
    ("n26.com", "N26"),
    ("wolt.com", "Wolt"),
    ("flixbus.de", "FlixBus"),
    ("klarna.com", "Klarna"),
    ("shopify.com", "Shopify"),
    ("wordpress.com", "WordPress.com"),
    ("canva.com", "Canva"),
    ("figma.com", "Figma"),
    ("trello.com", "Trello"),
    ("atlassian.com", "Atlassian"),
    ("openai.com", "OpenAI"),
    ("duolingo.com", "Duolingo"),
    ("strava.com", "Strava"),
    ("tripadvisor.com", "Tripadvisor"),
];

/// Best-effort pretty name from a domain: `spotify.com` -> `Spotify`,
/// `deutschebahn.com` -> `Deutsche Bahn` (small built-in table), else capitalised label.
pub fn display_name_for(domain: &str) -> String {
    let d = registrable_domain(domain).unwrap_or_else(|| domain.trim().to_lowercase());
    let d = canonical_service_domain(&d);
    if let Some((_, name)) = BRANDS.iter().find(|(k, _)| *k == d) {
        return (*name).to_string();
    }
    let label = d.split('.').next().unwrap_or(&d);
    let mut chars = label.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rd(s: &str) -> Option<String> {
        registrable_domain(s)
    }

    #[test]
    fn urls() {
        assert_eq!(
            rd("https://accounts.spotify.com/de/login"),
            Some("spotify.com".into())
        );
        assert_eq!(rd("spotify.com"), Some("spotify.com".into()));
        assert_eq!(rd("WWW.Spotify.COM"), Some("spotify.com".into()));
        assert_eq!(
            rd("http://user:pw@login.example.org:8080/x?y=1#z"),
            Some("example.org".into())
        );
        assert_eq!(rd("//cdn.example.com/a"), Some("example.com".into()));
        assert_eq!(rd("shop.example.co.uk"), Some("example.co.uk".into()));
        assert_eq!(
            rd("https://foo.bar.example.com.au/"),
            Some("example.com.au".into())
        );
        assert_eq!(rd("example.com."), Some("example.com".into()));
    }

    #[test]
    fn emails() {
        assert_eq!(rd("noreply@mail.spotify.com"), Some("spotify.com".into()));
        assert_eq!(
            rd("\"Spotify\" <no-reply@mail.spotify.com>"),
            Some("spotify.com".into())
        );
        assert_eq!(rd("Deutsche Bahn <info@bahn.de>"), Some("bahn.de".into()));
        assert_eq!(rd("<a@b.example.de>"), Some("example.de".into()));
        assert_eq!(rd("mailto:a@example.com"), Some("example.com".into()));
    }

    #[test]
    fn android() {
        assert_eq!(
            rd("android://c2hhMjU2fGFiY2RlZg==@com.spotify.music/"),
            Some("spotify.com".into())
        );
        assert_eq!(
            rd("android://hash@de.zalando.mobile/"),
            Some("zalando.de".into())
        );
        assert_eq!(rd("android://hash@single/"), None);
    }

    #[test]
    fn idn() {
        assert_eq!(rd("https://www.Bücher.de/x"), Some("bücher.de".into()));
        assert_eq!(rd("xn--bcher-kva.de"), Some("xn--bcher-kva.de".into()));
    }

    #[test]
    fn rejects() {
        for s in [
            "",
            "   ",
            "localhost",
            "http://localhost:3000/",
            "192.168.1.1",
            "http://10.0.0.1:8080/x",
            "[::1]",
            "http://[2001:db8::1]:80/",
            "intranet",
            "com",
            "co.uk",
            "not a domain",
            "a@b",
            "foo..com",
        ] {
            assert_eq!(rd(s), None, "{s:?}");
        }
    }

    #[test]
    fn aliases() {
        assert_eq!(canonical_service_domain("googlemail.com"), "google.com");
        assert_eq!(canonical_service_domain("twitter.com"), "x.com");
        assert_eq!(canonical_service_domain("facebookmail.com"), "facebook.com");
        assert_eq!(canonical_service_domain("spotifymail.com"), "spotify.com");
        assert_eq!(
            canonical_service_domain("steampowered.com"),
            "steamcommunity.com"
        );
        assert_eq!(
            canonical_service_domain("paypal-communication.com"),
            "paypal.com"
        );
        assert_eq!(
            canonical_service_domain("microsoftonline.com"),
            "microsoft.com"
        );
        assert_eq!(canonical_service_domain("amazon.de"), "amazon.de");
        assert_eq!(canonical_service_domain("youtube.com"), "youtube.com");
        assert_eq!(canonical_service_domain("Example.COM"), "example.com");
    }

    #[test]
    fn infra() {
        for d in [
            "sendgrid.net",
            "mcsv.net",
            "amazonses.com",
            "gmail.com",
            "gmx.de",
            "web.de",
            "yahoo.com",
            "yahoo.co.jp",
            "hotmail.de",
            "live.com",
            "outlook.com",
            "cmail19.com",
            "rsys2.net",
            "privaterelay.appleid.com",
        ] {
            assert!(is_infrastructure_domain(d), "{d}");
        }
        for d in [
            "spotify.com",
            "microsoft.com",
            "cmailer.com",
            "amazon.de",
            "google.com",
        ] {
            assert!(!is_infrastructure_domain(d), "{d}");
        }
    }

    #[test]
    fn names() {
        assert_eq!(display_name_for("spotify.com"), "Spotify");
        assert_eq!(display_name_for("deutschebahn.com"), "Deutsche Bahn");
        assert_eq!(display_name_for("mail.zalando.de"), "Zalando");
        assert_eq!(display_name_for("twitter.com"), "X (Twitter)");
        assert_eq!(display_name_for("foobar.io"), "Foobar");
        assert_eq!(display_name_for(""), "");
    }

    #[test]
    fn table_sanity() {
        assert!(BRANDS.len() >= 40);
        for (alias, canonical) in ALIASES {
            assert_eq!(
                rd(alias).as_deref(),
                Some(*alias),
                "alias {alias} must be registrable"
            );
            assert_eq!(rd(canonical).as_deref(), Some(*canonical));
            assert!(!is_infrastructure_domain(canonical));
        }
    }
}
