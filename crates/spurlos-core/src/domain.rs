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
    let _ = input;
    todo!("implemented by core agent")
}

/// Map aliases so different domains of the same service merge into one account,
/// e.g. `googlemail.com` -> `google.com`, `spotifymail.com` -> `spotify.com`,
/// `facebookmail.com` -> `facebook.com`, `twitter.com` -> `x.com`,
/// `steampowered.com` -> `steamcommunity.com`... Country domains of shops
/// (`amazon.de` vs `amazon.com`) stay separate, they are separate accounts.
/// Input and output are registrable domains.
pub fn canonical_service_domain(registrable: &str) -> String {
    let _ = registrable;
    todo!("implemented by core agent")
}

/// Mail / infrastructure domains that must never become an account by themselves
/// (e.g. `sendgrid.net`, `mailchimp.com`, `mcsv.net`, `amazonses.com`,
/// `mailgun.org`, `gmail.com`, `gmx.de`, `web.de`, `outlook.com` as *sender*
/// of personal mails, `privaterelay.appleid.com`).
pub fn is_infrastructure_domain(registrable: &str) -> bool {
    let _ = registrable;
    todo!("implemented by core agent")
}

/// Best-effort pretty name from a domain: `spotify.com` -> `Spotify`,
/// `deutschebahn.com` -> `Deutsche Bahn` (small built-in table), else capitalised label.
pub fn display_name_for(domain: &str) -> String {
    let _ = domain;
    todo!("implemented by core agent")
}
