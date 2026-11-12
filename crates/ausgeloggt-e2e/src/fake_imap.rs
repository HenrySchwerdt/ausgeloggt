//! Multi-connection fake IMAP server on loopback (plain, no STARTTLS) that serves a
//! [`RawMail`] list. The mail crate treats loopback hosts as trusted plain connections
//! (Proton Bridge style), so the real `scan_mailbox_with` code path runs unmodified.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

use ausgeloggt_core::MailboxConfig;
use uuid::Uuid;

use crate::corpus::RawMail;

#[derive(Debug, Default)]
pub struct ServerState {
    pub mails: Vec<RawMail>,
    pub uid_validity: u32,
    /// Close the connection abruptly once this many `UID FETCH` commands were answered
    /// (counted across connections; `None` = never).
    pub drop_after_fetches: Option<usize>,
    pub fetches: usize,
    /// Total number of accepted connections.
    pub connections: usize,
    /// Every command the client sent (only the verb, never credentials).
    pub verbs: Vec<String>,
    /// Inject random hostile untagged lines before every reply (seed for variety).
    pub chaos: Option<u64>,
}

#[derive(Clone)]
pub struct FakeImap {
    pub port: u16,
    pub state: Arc<Mutex<ServerState>>,
}

const HEADER_ITEMS: &str = "HEADER.FIELDS (FROM SUBJECT DATE MESSAGE-ID TO DELIVERED-TO LIST-UNSUBSCRIBE LIST-UNSUBSCRIBE-POST LIST-ID PRECEDENCE SENDER RETURN-PATH)";

impl FakeImap {
    pub fn start(mails: Vec<RawMail>) -> FakeImap {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let state = Arc::new(Mutex::new(ServerState {
            mails,
            uid_validity: 4242,
            ..Default::default()
        }));
        let st = state.clone();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                st.lock().unwrap().connections += 1;
                let st = st.clone();
                thread::spawn(move || serve(stream, st));
            }
        });
        FakeImap { port, state }
    }

    pub fn config(&self, address: &str) -> MailboxConfig {
        MailboxConfig {
            id: Uuid::nil(),
            address: address.into(),
            imap_host: "127.0.0.1".into(),
            imap_port: self.port,
            username: address.into(),
            password: "secret".into(),
            folders: vec![],
            progress: vec![],
            last_scan: None,
        }
    }

    pub fn add_mails(&self, mails: Vec<RawMail>) {
        self.state.lock().unwrap().mails.extend(mails);
    }

    pub fn max_uid(&self) -> u32 {
        self.state
            .lock()
            .unwrap()
            .mails
            .iter()
            .map(|m| m.uid)
            .max()
            .unwrap_or(0)
    }
}

fn serve(stream: std::net::TcpStream, st: Arc<Mutex<ServerState>>) {
    let mut out = match stream.try_clone() {
        Ok(o) => o,
        Err(_) => return,
    };
    let mut rd = BufReader::new(stream);
    let mut rng = crate::rng::Rng::new(st.lock().unwrap().connections as u64 * 7919 + 1);
    if out.write_all(b"* OK fake imap ready\r\n").is_err() {
        return;
    }
    loop {
        let mut raw = Vec::new();
        if rd.read_until(b'\n', &mut raw).unwrap_or(0) == 0 {
            return;
        }
        let line = String::from_utf8_lossy(&raw).trim_end().to_string();
        let (tag, cmd) = line.split_once(' ').unwrap_or((&line, ""));
        let up = cmd.to_ascii_uppercase();
        {
            let verb = up.split_whitespace().take(2).collect::<Vec<_>>().join(" ");
            st.lock().unwrap().verbs.push(verb);
            let chaos = st.lock().unwrap().chaos;
            if let Some(seed) = chaos {
                let mut buf = Vec::new();
                for _ in 0..rng.below(4) {
                    buf.extend_from_slice(&chaos_line(&mut rng, seed));
                }
                if out.write_all(&buf).is_err() {
                    return;
                }
            }
        }
        let ok = if up.starts_with("CAPABILITY") {
            write!(
                out,
                "* CAPABILITY IMAP4rev1 SPECIAL-USE\r\n{tag} OK done\r\n"
            )
        } else if up.starts_with("LOGIN") {
            if cmd.contains("wrongpw") {
                write!(
                    out,
                    "{tag} NO [AUTHENTICATIONFAILED] Invalid credentials\r\n"
                )
            } else {
                write!(out, "{tag} OK logged in\r\n")
            }
        } else if up.starts_with("LIST") {
            write!(
                out,
                "* LIST (\\HasNoChildren) \"/\" \"INBOX\"\r\n* LIST (\\HasNoChildren \\Sent) \"/\" \"Sent\"\r\n* LIST (\\HasNoChildren \\Trash) \"/\" \"Trash\"\r\n* LIST (\\HasNoChildren \\Junk) \"/\" \"Spam\"\r\n* LIST (\\Noselect \\HasChildren) \"/\" \"[Gmail]\"\r\n{tag} OK done\r\n"
            )
        } else if up.starts_with("EXAMINE") {
            let s = st.lock().unwrap();
            write!(
                out,
                "* {} EXISTS\r\n* OK [UIDVALIDITY {}] ok\r\n{tag} OK [READ-ONLY] done\r\n",
                s.mails.len(),
                s.uid_validity
            )
        } else if up.starts_with("UID SEARCH") {
            let start: u32 = up
                .split("UID ")
                .nth(2)
                .and_then(|s| s.split(':').next())
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(1);
            let s = st.lock().unwrap();
            let mut uids: Vec<u32> = s
                .mails
                .iter()
                .map(|m| m.uid)
                .filter(|u| *u >= start)
                .collect();
            if uids.is_empty() {
                // Real servers return the highest UID for `n:*` even if below n.
                if let Some(m) = s.mails.last() {
                    uids.push(m.uid);
                }
            }
            let list: Vec<String> = uids.iter().map(u32::to_string).collect();
            write!(out, "* SEARCH {}\r\n{tag} OK done\r\n", list.join(" "))
        } else if up.starts_with("UID FETCH") {
            assert!(
                up.contains("BODY.PEEK"),
                "client must never fetch without PEEK: {up}"
            );
            let set = up.split_whitespace().nth(2).unwrap_or("");
            let (a, b) = set.split_once(':').unwrap_or((set, set));
            let (a, b): (u32, u32) = (a.parse().unwrap_or(0), b.parse().unwrap_or(u32::MAX));
            let body_request = up.contains("BODY.PEEK[TEXT]");
            let mut s = st.lock().unwrap();
            s.fetches += 1;
            if s.drop_after_fetches.is_some_and(|n| s.fetches > n) {
                return;
            }
            let mut buf: Vec<u8> = Vec::new();
            for (i, m) in s.mails.iter().enumerate() {
                if m.uid >= a && m.uid <= b {
                    write_fetch(&mut buf, i + 1, m, body_request);
                }
            }
            buf.extend_from_slice(format!("{tag} OK done\r\n").as_bytes());
            out.write_all(&buf)
        } else if up.starts_with("LOGOUT") {
            let _ = write!(out, "* BYE bye\r\n{tag} OK logout\r\n");
            return;
        } else {
            write!(out, "{tag} BAD unknown\r\n")
        };
        if ok.is_err() || out.flush().is_err() {
            return;
        }
    }
}

fn write_fetch(out: &mut Vec<u8>, seq: usize, m: &RawMail, body_request: bool) {
    if body_request {
        let hdr = format!("{}\r\n\r\n", m.body_headers);
        let body = m.body.clone().unwrap_or_default();
        out.extend_from_slice(
            format!(
                "* {seq} FETCH (UID {} BODY[HEADER.FIELDS (CONTENT-TYPE CONTENT-TRANSFER-ENCODING)] {{{}}}\r\n",
                m.uid,
                hdr.len()
            )
            .as_bytes(),
        );
        out.extend_from_slice(hdr.as_bytes());
        out.extend_from_slice(format!(" BODY[TEXT]<0> {{{}}}\r\n", body.len()).as_bytes());
        out.extend_from_slice(&body);
        out.extend_from_slice(b")\r\n");
    } else {
        out.extend_from_slice(
            format!(
                "* {seq} FETCH (UID {} INTERNALDATE \"{}\" BODY[{HEADER_ITEMS}] {{{}}}\r\n",
                m.uid,
                m.internal_date,
                m.header.len()
            )
            .as_bytes(),
        );
        out.extend_from_slice(&m.header);
        out.extend_from_slice(b")\r\n");
    }
}

/// A hostile / buggy server line: multi-byte characters at every position, odd literals,
/// BYE, huge numbers, random bytes.
fn chaos_line(rng: &mut crate::rng::Rng, seed: u64) -> Vec<u8> {
    const T: &[&str] = &[
        "* ÄÄÄ\r\n",
        "* ÄÖ\r\n",
        "* Ä\r\n",
        "* 日本語\r\n",
        "* SEARCH 1 ÄÄ 3 99999999999 -4\r\n",
        "* SEARCH\r\n",
        "* CAPABILITY ÄÄÄÄÄÄÄÄÄÄÄ\r\n",
        "* CAPABILITY\r\n",
        "* 5 EXISTS\r\n",
        "* ÄÄ EXISTS\r\n",
        "* OK [UIDVALIDITY ÄÄ] x\r\n",
        "* OK [UIDVALIDITY 99999999999999999999] x\r\n",
        "* LIST (\\Noselect) \"/\" ÄÄ\r\n",
        "* LIST (\\Ä) \"/\" \"Ä\"\r\n",
        "* LIST\r\n",
        "* LIST (\r\n",
        "* LIST () \"/\" {3}\r\nÄÄ\r\n",
        "* 1 FETCH (UID 1 BODY[HEADER.FIELDS (X)] {5}\r\nabcde)\r\n",
        "* 1 FETCH (UID ÄÄ INTERNALDATE \"Ä\" BODY[HEADER.FIELDS (X)] {2}\r\nÄ)\r\n",
        "* 1 FETCH ({999999999}\r\n",
        "* 1 FETCH (UID 4294967295 BODY[TEXT] {0}\r\n)\r\n",
        "* 1 FETCH\r\n",
        "* 1\r\n",
        "* \r\n",
        "*\r\n",
        "+ ÄÄ continue\r\n",
        "+\r\n",
        "S0001 ÄÄ\r\n",
        "\r\n",
        "Ä\r\n",
    ];
    if rng.chance(0.2) {
        let n = rng.below(60) as usize + 1;
        let mut v = rng.bytes(n);
        v.retain(|b| *b != b'\n');
        v.extend_from_slice(b"\r\n");
        return v;
    }
    let _ = seed;
    T[rng.below(T.len() as u64) as usize].as_bytes().to_vec()
}
