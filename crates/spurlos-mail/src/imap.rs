//! Minimal blocking IMAP client over rustls (ring provider, bundled webpki roots).
//!
//! Only what the scanner needs: CAPABILITY, STARTTLS, LOGIN, LIST, EXAMINE, UID SEARCH,
//! UID FETCH, LOGOUT. Folders are only ever opened read-only (EXAMINE) and bodies are
//! fetched with `BODY.PEEK`, so no flag is ever changed on the server.
//!
//! Nothing that is sent (it contains the password) or received is logged.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use regex::Regex;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{ring, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, RootCertStore, SignatureScheme,
    StreamOwned,
};

use crate::MailError;

const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_LINE: u64 = 1 << 20;
const MAX_LITERAL: usize = 64 << 20;

pub(crate) trait Io: Read + Write + Send {}
impl<T: Read + Write + Send> Io for T {}

#[derive(Debug, Clone)]
pub(crate) enum Chunk {
    Text(String),
    Literal(Vec<u8>),
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Untagged {
    pub chunks: Vec<Chunk>,
}

impl Untagged {
    /// All text parts joined (literals left out).
    pub fn text(&self) -> String {
        self.chunks
            .iter()
            .filter_map(|c| match c {
                Chunk::Text(t) => Some(t.as_str()),
                Chunk::Literal(_) => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

#[derive(Debug)]
pub(crate) struct Reply {
    pub untagged: Vec<Untagged>,
    pub ok: bool,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Folder {
    pub name: String,
    /// Lowercase attributes, e.g. `\all`, `\noselect`.
    pub attrs: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Examined {
    pub uid_validity: u32,
    pub exists: u32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FetchItem {
    pub uid: Option<u32>,
    pub internal_date: Option<String>,
    pub header: Option<Vec<u8>>,
    pub body: Option<Vec<u8>>,
}

pub(crate) struct Client {
    rd: BufReader<Box<dyn Io>>,
    tag: u32,
}

// ------------------------------------------------------------------------------------------
// TLS
// ------------------------------------------------------------------------------------------

fn provider() -> Arc<CryptoProvider> {
    Arc::new(ring::default_provider())
}

static STRICT_CONFIG: LazyLock<Arc<ClientConfig>> = LazyLock::new(|| {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let cfg = ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .expect("ring supports the default TLS versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    Arc::new(cfg)
});

/// Accepts any certificate. Used ONLY for loopback connections (Proton Bridge ships a
/// self-signed certificate).
#[derive(Debug)]
struct AcceptAny(Arc<CryptoProvider>);

impl ServerCertVerifier for AcceptAny {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

fn lax_config() -> Arc<ClientConfig> {
    let p = provider();
    let cfg = ClientConfig::builder_with_provider(p.clone())
        .with_safe_default_protocol_versions()
        .expect("ring supports the default TLS versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAny(p)))
        .with_no_client_auth();
    Arc::new(cfg)
}

fn tls_wrap(tcp: TcpStream, host: &str, lax: bool) -> Result<Box<dyn Io>, MailError> {
    let cfg = if lax {
        lax_config()
    } else {
        STRICT_CONFIG.clone()
    };
    let name = ServerName::try_from(host.to_string())
        .map_err(|_| MailError::Connect(host.to_string(), "Ungültiger Servername".into()))?;
    let conn = ClientConnection::new(cfg, name)
        .map_err(|e| MailError::Connect(host.to_string(), e.to_string()))?;
    Ok(Box::new(StreamOwned::new(conn, tcp)))
}

pub(crate) fn is_loopback(host: &str) -> bool {
    matches!(host.trim(), "127.0.0.1" | "localhost" | "::1" | "[::1]")
}

fn io_err(e: &std::io::Error) -> MailError {
    use std::io::ErrorKind::*;
    match e.kind() {
        WouldBlock | TimedOut => MailError::Imap("Zeitüberschreitung bei der Serverantwort".into()),
        UnexpectedEof | ConnectionReset | ConnectionAborted | BrokenPipe => {
            MailError::Imap("Verbindung vom Server beendet".into())
        }
        _ => MailError::Imap(format!("Netzwerkfehler: {e}")),
    }
}

// ------------------------------------------------------------------------------------------
// Parsing helpers
// ------------------------------------------------------------------------------------------

/// `{123}` / `{123+}` at the end of a line -> literal length.
fn literal_len(line: &str) -> Option<usize> {
    let line = line.trim_end();
    let inner = line.strip_suffix('}')?;
    let open = inner.rfind('{')?;
    let digits = inner[open + 1..].trim_end_matches('+');
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

pub(crate) fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

fn unquote(s: &str) -> Option<(String, &str)> {
    let rest = s.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                let (_, n) = chars.next()?;
                out.push(n);
            }
            '"' => return Some((out, &rest[i + 1..])),
            _ => out.push(c),
        }
    }
    None
}

pub(crate) fn parse_list(u: &Untagged) -> Option<Folder> {
    let text = u.text();
    let rest = text.strip_prefix("* ")?;
    let rest = rest
        .get(..4)
        .filter(|p| p.eq_ignore_ascii_case("LIST"))
        .map(|_| rest[4..].trim_start())?;
    let rest = rest.strip_prefix('(')?;
    let close = rest.find(')')?;
    let attrs: Vec<String> = rest[..close]
        .split_whitespace()
        .map(|a| a.to_ascii_lowercase())
        .collect();
    let mut rest = rest[close + 1..].trim_start();
    // Hierarchy delimiter: quoted char or NIL.
    if rest.starts_with('"') {
        rest = unquote(rest)?.1.trim_start();
    } else if rest.len() >= 3 && rest[..3].eq_ignore_ascii_case("NIL") {
        rest = rest[3..].trim_start();
    } else {
        return None;
    }
    let name = if rest.starts_with('"') {
        unquote(rest)?.0
    } else if let Some(lit) = u.chunks.iter().find_map(|c| match c {
        Chunk::Literal(b) => Some(b),
        _ => None,
    }) {
        String::from_utf8_lossy(lit).into_owned()
    } else {
        rest.trim().to_string()
    };
    if name.is_empty() {
        return None;
    }
    Some(Folder { name, attrs })
}

static UID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\bUID (\d+)").expect("uid"));
static INTERNALDATE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)INTERNALDATE "([^"]+)""#).expect("internaldate"));
static FETCH_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\* \d+ FETCH\b").expect("fetch"));

pub(crate) fn parse_fetch(u: &Untagged) -> Option<FetchItem> {
    let text = u.text();
    if !FETCH_RE.is_match(&text) {
        return None;
    }
    let mut item = FetchItem {
        uid: UID_RE.captures(&text).and_then(|c| c[1].parse().ok()),
        internal_date: INTERNALDATE_RE.captures(&text).map(|c| c[1].to_string()),
        ..Default::default()
    };
    let mut last_text = String::new();
    for chunk in &u.chunks {
        match chunk {
            Chunk::Text(t) => last_text = t.to_ascii_lowercase(),
            Chunk::Literal(bytes) => {
                if last_text.contains("header") {
                    item.header = Some(bytes.clone());
                } else if last_text.contains("text") || last_text.contains("body[]") {
                    item.body = Some(bytes.clone());
                }
            }
        }
    }
    Some(item)
}

// ------------------------------------------------------------------------------------------
// Client
// ------------------------------------------------------------------------------------------

impl Client {
    pub fn new(io: Box<dyn Io>) -> Self {
        Client {
            rd: BufReader::new(io),
            tag: 0,
        }
    }

    /// Open a connection. Implicit TLS for everything except loopback (Proton Bridge: plain,
    /// STARTTLS if offered) and port 143 (STARTTLS required, strict certificate check).
    pub fn connect(host: &str, port: u16) -> Result<Client, MailError> {
        let host = host.trim();
        let connect_err = |m: String| MailError::Connect(host.to_string(), m);
        let addrs: Vec<_> = (host, port)
            .to_socket_addrs()
            .map_err(|_| connect_err("Server nicht gefunden".into()))?
            .collect();
        let mut last = None;
        let mut tcp = None;
        for a in addrs {
            match TcpStream::connect_timeout(&a, TIMEOUT) {
                Ok(s) => {
                    tcp = Some(s);
                    break;
                }
                Err(e) => last = Some(e),
            }
        }
        let tcp = tcp.ok_or_else(|| {
            connect_err(match last {
                Some(e) if e.kind() == std::io::ErrorKind::TimedOut => "Zeitüberschreitung".into(),
                Some(e) => e.to_string(),
                None => "Server nicht gefunden".into(),
            })
        })?;
        tcp.set_read_timeout(Some(TIMEOUT)).ok();
        tcp.set_write_timeout(Some(TIMEOUT)).ok();
        tcp.set_nodelay(true).ok();

        let loopback = is_loopback(host);
        if !loopback && port != 143 {
            let mut c = Client::new(tls_wrap(tcp, host, false)?);
            c.read_greeting()
                .map_err(|e| connect_err(strip_prefix(&e)))?;
            return Ok(c);
        }

        let tcp2 = tcp.try_clone().map_err(|e| connect_err(e.to_string()))?;
        let mut c = Client::new(Box::new(tcp));
        c.read_greeting()
            .map_err(|e| connect_err(strip_prefix(&e)))?;
        let caps = c.capability().map_err(|e| connect_err(strip_prefix(&e)))?;
        if caps.iter().any(|x| x == "STARTTLS") {
            let reply = c
                .command("STARTTLS")
                .map_err(|e| connect_err(strip_prefix(&e)))?;
            if !reply.ok {
                return Err(connect_err("STARTTLS abgelehnt".into()));
            }
            // The server sends nothing before our ClientHello, so the read buffer is empty.
            return Ok(Client::new(tls_wrap(tcp2, host, loopback)?));
        }
        if !loopback {
            return Err(connect_err(
                "Server bietet keine verschlüsselte Verbindung an".into(),
            ));
        }
        Ok(c)
    }

    fn read_greeting(&mut self) -> Result<(), MailError> {
        let chunks = self.read_logical()?;
        let line = chunks.first().map(chunk_text).unwrap_or_default();
        let up = line.to_ascii_uppercase();
        if up.starts_with("* OK") || up.starts_with("* PREAUTH") {
            Ok(())
        } else {
            Err(MailError::Imap("Unerwartete Begrüßung des Servers".into()))
        }
    }

    fn read_logical(&mut self) -> Result<Vec<Chunk>, MailError> {
        let mut chunks = Vec::new();
        loop {
            let mut line = Vec::new();
            let n = (&mut self.rd)
                .take(MAX_LINE)
                .read_until(b'\n', &mut line)
                .map_err(|e| io_err(&e))?;
            if n == 0 {
                return Err(MailError::Imap("Verbindung vom Server beendet".into()));
            }
            if !line.ends_with(b"\n") {
                return Err(MailError::Imap(
                    "Ungültige Serverantwort (Zeile zu lang)".into(),
                ));
            }
            while line.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
                line.pop();
            }
            let text = String::from_utf8_lossy(&line).into_owned();
            let lit = literal_len(&text);
            chunks.push(Chunk::Text(text));
            match lit {
                Some(len) => {
                    if len > MAX_LITERAL {
                        return Err(MailError::Imap("Serverantwort zu groß".into()));
                    }
                    let mut buf = Vec::new();
                    (&mut self.rd)
                        .take(len as u64)
                        .read_to_end(&mut buf)
                        .map_err(|e| io_err(&e))?;
                    if buf.len() != len {
                        return Err(MailError::Imap("Verbindung vom Server beendet".into()));
                    }
                    chunks.push(Chunk::Literal(buf));
                }
                None => return Ok(chunks),
            }
        }
    }

    /// Send one command and collect the untagged responses up to the tagged status.
    pub fn command(&mut self, cmd: &str) -> Result<Reply, MailError> {
        self.tag += 1;
        let tag = format!("S{:04}", self.tag);
        let io = self.rd.get_mut();
        io.write_all(format!("{tag} {cmd}\r\n").as_bytes())
            .map_err(|e| io_err(&e))?;
        io.flush().map_err(|e| io_err(&e))?;
        let tag_prefix = format!("{tag} ");
        let mut untagged = Vec::new();
        loop {
            let chunks = self.read_logical()?;
            let first = chunks.first().map(chunk_text).unwrap_or_default();
            if let Some(rest) = first.strip_prefix(&tag_prefix) {
                let word = rest
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_ascii_uppercase();
                return Ok(Reply {
                    untagged,
                    ok: word == "OK",
                    status: rest.to_string(),
                });
            } else if let Some(rest) = first.strip_prefix("* ") {
                if rest.len() >= 3 && rest[..3].eq_ignore_ascii_case("BYE") {
                    return Err(MailError::Imap(
                        "Der Server hat die Verbindung beendet".into(),
                    ));
                }
                untagged.push(Untagged { chunks });
            }
            // Continuation requests ("+") and anything else are ignored.
        }
    }

    pub fn capability(&mut self) -> Result<Vec<String>, MailError> {
        let reply = self.command("CAPABILITY")?;
        let mut caps = Vec::new();
        for u in &reply.untagged {
            let t = u.text();
            if let Some(rest) = t.strip_prefix("* ") {
                if rest.len() >= 10 && rest[..10].eq_ignore_ascii_case("CAPABILITY") {
                    caps.extend(
                        rest[10..]
                            .split_whitespace()
                            .map(|c| c.to_ascii_uppercase()),
                    );
                }
            }
        }
        Ok(caps)
    }

    pub fn login(&mut self, user: &str, password: &str) -> Result<(), MailError> {
        let bad = |s: &str| s.chars().any(|c| matches!(c, '\r' | '\n' | '\0'));
        if bad(user) || bad(password) {
            return Err(MailError::Auth);
        }
        let reply = self.command(&format!("LOGIN {} {}", quote(user), quote(password)))?;
        if reply.ok {
            Ok(())
        } else if reply.status.to_ascii_uppercase().starts_with("BAD") {
            Err(MailError::Imap(
                "Anmeldung vom Server nicht akzeptiert".into(),
            ))
        } else {
            Err(MailError::Auth)
        }
    }

    pub fn list_folders(&mut self, special_use: bool) -> Result<Vec<Folder>, MailError> {
        let cmd = if special_use {
            r#"LIST "" "*" RETURN (SPECIAL-USE)"#
        } else {
            r#"LIST "" "*""#
        };
        let mut reply = self.command(cmd)?;
        if !reply.ok && special_use {
            reply = self.command(r#"LIST "" "*""#)?;
        }
        if !reply.ok {
            return Err(MailError::Imap(
                "Ordnerliste konnte nicht gelesen werden".into(),
            ));
        }
        Ok(reply.untagged.iter().filter_map(parse_list).collect())
    }

    pub fn examine(&mut self, folder: &str) -> Result<Examined, MailError> {
        let reply = self.command(&format!("EXAMINE {}", quote(folder)))?;
        if !reply.ok {
            return Err(MailError::Imap(format!(
                "Ordner \"{folder}\" konnte nicht geöffnet werden"
            )));
        }
        let mut uid_validity = None;
        let mut exists = 0;
        for u in &reply.untagged {
            let t = u.text();
            let up = t.to_ascii_uppercase();
            if let Some(i) = up.find("[UIDVALIDITY ") {
                let digits: String = up[i + 13..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                uid_validity = digits.parse().ok();
            }
            if up.trim_end().ends_with(" EXISTS") {
                if let Some(n) = up.trim_start_matches("* ").split_whitespace().next() {
                    exists = n.parse().unwrap_or(0);
                }
            }
        }
        Ok(Examined {
            uid_validity: uid_validity.unwrap_or(0),
            exists,
        })
    }

    /// UIDs >= `start`, ascending.
    pub fn uid_search_from(&mut self, start: u32) -> Result<Vec<u32>, MailError> {
        let reply = self.command(&format!("UID SEARCH UID {start}:*"))?;
        if !reply.ok {
            return Err(MailError::Imap("Suche im Ordner fehlgeschlagen".into()));
        }
        let mut uids = Vec::new();
        for u in &reply.untagged {
            let t = u.text();
            if let Some(rest) = t.strip_prefix("* ") {
                if rest.len() >= 6 && rest[..6].eq_ignore_ascii_case("SEARCH") {
                    uids.extend(
                        rest[6..]
                            .split_whitespace()
                            .filter_map(|n| n.parse::<u32>().ok()),
                    );
                }
            }
        }
        // `start:*` always yields the highest UID, even when it is below `start`.
        uids.retain(|u| *u >= start);
        uids.sort_unstable();
        uids.dedup();
        Ok(uids)
    }

    pub fn uid_fetch(&mut self, set: &str, items: &str) -> Result<Vec<FetchItem>, MailError> {
        let reply = self.command(&format!("UID FETCH {set} {items}"))?;
        if !reply.ok {
            return Err(MailError::Imap("Abruf der Mails fehlgeschlagen".into()));
        }
        Ok(reply.untagged.iter().filter_map(parse_fetch).collect())
    }

    pub fn logout(&mut self) {
        let _ = self.command("LOGOUT");
    }
}

fn chunk_text(c: &Chunk) -> String {
    match c {
        Chunk::Text(t) => t.clone(),
        Chunk::Literal(_) => String::new(),
    }
}

fn strip_prefix(e: &MailError) -> String {
    match e {
        MailError::Imap(m) => m.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(lines: &[&str]) -> Untagged {
        Untagged {
            chunks: lines
                .iter()
                .map(|l| Chunk::Text((*l).to_string()))
                .collect(),
        }
    }

    #[test]
    fn literal_markers() {
        assert_eq!(literal_len("* 1 FETCH (BODY[] {342}"), Some(342));
        assert_eq!(literal_len("A1 LOGIN {5+}"), Some(5));
        assert_eq!(literal_len("* OK done"), None);
        assert_eq!(literal_len("text {abc}"), None);
        assert_eq!(literal_len("{}"), None);
    }

    #[test]
    fn quoting() {
        assert_eq!(quote(r#"pa"ss\w"#), r#""pa\"ss\\w""#);
        assert_eq!(
            unquote(r#""a\"b" rest"#),
            Some(("a\"b".to_string(), " rest"))
        );
        assert_eq!(unquote("no quote"), None);
    }

    #[test]
    fn list_gmail_special_use() {
        let f = parse_list(&u(&[
            r#"* LIST (\HasNoChildren \All) "/" "[Gmail]/All Mail""#,
        ]))
        .unwrap();
        assert_eq!(f.name, "[Gmail]/All Mail");
        assert_eq!(f.attrs, vec![r"\hasnochildren", r"\all"]);
    }

    #[test]
    fn list_variants() {
        let f = parse_list(&u(&[r#"* LIST () "." INBOX"#])).unwrap();
        assert_eq!(f.name, "INBOX");
        let f = parse_list(&u(&[r#"* LIST (\Noselect) NIL "Shared""#])).unwrap();
        assert_eq!(f.name, "Shared");
        assert!(f.attrs.contains(&r"\noselect".to_string()));
        let f = parse_list(&u(&[r#"* list (\Archive) "/" "Archiv\\x""#])).unwrap();
        assert_eq!(f.name, r"Archiv\x");
        assert!(parse_list(&u(&["* OK nothing"])).is_none());
        assert!(parse_list(&u(&["* 3 EXISTS"])).is_none());
    }

    #[test]
    fn list_with_literal_name() {
        let un = Untagged {
            chunks: vec![
                Chunk::Text(r#"* LIST () "/" {6}"#.into()),
                Chunk::Literal(b"Ordner".to_vec()),
                Chunk::Text(String::new()),
            ],
        };
        assert_eq!(parse_list(&un).unwrap().name, "Ordner");
    }

    #[test]
    fn fetch_header_literal_and_uid_after() {
        let un = Untagged {
            chunks: vec![
                Chunk::Text(
                    r#"* 4 FETCH (INTERNALDATE " 7-Mar-2024 10:00:00 +0100" BODY[HEADER.FIELDS (FROM SUBJECT)] {10}"#
                        .into(),
                ),
                Chunk::Literal(b"From: a@b\r\n".to_vec()),
                Chunk::Text(" UID 77)".into()),
            ],
        };
        let item = parse_fetch(&un).unwrap();
        assert_eq!(item.uid, Some(77));
        assert_eq!(
            item.internal_date.as_deref(),
            Some(" 7-Mar-2024 10:00:00 +0100")
        );
        assert_eq!(item.header.as_deref(), Some(&b"From: a@b\r\n"[..]));
        assert!(item.body.is_none());
    }

    #[test]
    fn fetch_header_and_text() {
        let un = Untagged {
            chunks: vec![
                Chunk::Text("* 1 FETCH (UID 5 BODY[HEADER.FIELDS (CONTENT-TYPE)] {2}".into()),
                Chunk::Literal(b"\r\n".to_vec()),
                Chunk::Text(" BODY[TEXT]<0> {3}".into()),
                Chunk::Literal(b"hey".to_vec()),
                Chunk::Text(")".into()),
            ],
        };
        let item = parse_fetch(&un).unwrap();
        assert_eq!(item.uid, Some(5));
        assert_eq!(item.body.as_deref(), Some(&b"hey"[..]));
        assert!(item.header.is_some());
    }

    #[test]
    fn non_fetch_ignored() {
        assert!(parse_fetch(&u(&["* 3 EXISTS"])).is_none());
        assert!(parse_fetch(&u(&["* OK [UIDVALIDITY 5]"])).is_none());
    }

    #[test]
    fn loopback_detection() {
        assert!(is_loopback("127.0.0.1"));
        assert!(is_loopback("localhost"));
        assert!(!is_loopback("imap.gmail.com"));
        assert!(!is_loopback("127.0.0.1.evil.example"));
    }
}
