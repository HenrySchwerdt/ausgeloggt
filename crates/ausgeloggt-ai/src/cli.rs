//! Claude and OpenAI through their local agent CLIs: Claude Code (`claude`, the engine of the
//! Claude Agent SDK) and Codex (`codex`, the engine of the Codex SDK). Both run on the user's
//! own login (Claude subscription / ChatGPT account); ausgeloggt never sees or stores a key.
//!
//! Every call is one headless, tool-less, sessionless turn in an empty working directory:
//! - Claude: `claude -p --input-format stream-json --output-format stream-json --json-schema …
//!   --tools "" --no-session-persistence --strict-mcp-config --setting-sources ""`, the user
//!   message (text plus optional image block) on stdin, the answer in `structured_output` of
//!   the `result` event.
//! - Codex: `codex exec --ephemeral --ignore-user-config --sandbox read-only --output-schema F
//!   -o OUT [-i IMG] -`, prompt on stdin, the final JSON message in OUT.
//!
//! API key variables are removed from the child environment so the CLIs use the login and
//! never bill an API account. Argument building and output parsing are pure and tested; the
//! process runner is the only impure part.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::AiError;

/// Model used for Claude when none is set: the fast, cheap tier (alias stays current).
pub const DEFAULT_CLAUDE_MODEL: &str = "haiku";
/// Models offered in the settings (Claude Code aliases).
pub const CLAUDE_MODELS: [&str; 3] = ["haiku", "sonnet", "opus"];

/// Environment variables that would make the CLIs bill an API account instead of the login,
/// or confuse a nested Claude Code session.
const STRIPPED_ENV: [&str; 6] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "OPENAI_API_KEY",
    "CODEX_API_KEY",
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cli {
    Claude,
    Codex,
}

impl Cli {
    pub fn label(self) -> &'static str {
        match self {
            Cli::Claude => "Claude Code",
            Cli::Codex => "Codex",
        }
    }
    fn bin(self) -> &'static str {
        match self {
            Cli::Claude => "claude",
            Cli::Codex => "codex",
        }
    }
    fn install_hint(self) -> &'static str {
        match self {
            Cli::Claude => "Claude Code ist nicht installiert. Installiere es von claude.com/code und melde dich einmal mit claude an.",
            Cli::Codex => "Codex ist nicht installiert. Installiere es von openai.com/codex und melde dich einmal mit codex login an.",
        }
    }
    fn login_hint(self) -> &'static str {
        match self {
            Cli::Claude => "Claude Code ist nicht angemeldet. Öffne ein Terminal, starte claude und melde dich mit deinem Claude Konto an.",
            Cli::Codex => "Codex ist nicht angemeldet. Öffne ein Terminal und führe codex login aus.",
        }
    }
}

/// One image for the model.
pub struct Image<'a> {
    pub bytes: &'a [u8],
    /// `image/png` or `image/jpeg`.
    pub mime: &'a str,
    pub b64: &'a str,
}

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

/// Strict variant of a schema: every object gets `additionalProperties: false`
/// (required by Codex / OpenAI structured outputs, harmless for Claude).
pub fn strict_schema(schema: &Value) -> Value {
    let mut s = schema.clone();
    fn walk(v: &mut Value) {
        match v {
            Value::Object(m) => {
                if m.get("type").and_then(Value::as_str) == Some("object") {
                    m.entry("additionalProperties").or_insert(Value::Bool(false));
                }
                for (_, c) in m.iter_mut() {
                    walk(c);
                }
            }
            Value::Array(a) => a.iter_mut().for_each(walk),
            _ => {}
        }
    }
    walk(&mut s);
    s
}

// ---------------------------------------------------------------------------
// Claude Code
// ---------------------------------------------------------------------------

pub fn claude_args(model: &str, system: &str, schema: &Value) -> Vec<String> {
    let model = if model.trim().is_empty() { DEFAULT_CLAUDE_MODEL } else { model.trim() };
    [
        "-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose",
        "--json-schema", &strict_schema(schema).to_string(),
        "--tools", "", "--no-session-persistence", "--strict-mcp-config", "--setting-sources", "",
        "--system-prompt", system, "--model", model,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// The single user message on stdin (stream-json input).
pub fn claude_input(user: &str, image: Option<&Image>) -> String {
    let mut content = Vec::new();
    if let Some(img) = image {
        content.push(json!({"type": "image", "source": {"type": "base64", "media_type": img.mime, "data": img.b64}}));
    }
    content.push(json!({"type": "text", "text": user}));
    let msg = json!({"type": "user", "message": {"role": "user", "content": content}});
    format!("{msg}\n")
}

/// Find the `result` event and return its structured output.
pub fn parse_claude_output(stdout: &str) -> Result<Value, AiError> {
    let result = stdout
        .lines()
        .rev()
        .filter_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
        .find(|v| v.get("type").and_then(Value::as_str) == Some("result"))
        .ok_or_else(|| AiError::BadResponse("Claude Code hat kein Ergebnis geliefert".into()))?;
    let is_error = result.get("is_error").and_then(Value::as_bool).unwrap_or(false)
        || result.get("subtype").and_then(Value::as_str).is_some_and(|s| s != "success");
    if is_error {
        let text = result.get("result").and_then(Value::as_str).unwrap_or("").to_string();
        return Err(classify_failure(Cli::Claude, &text));
    }
    if let Some(v) = result.get("structured_output").filter(|v| v.is_object()) {
        return Ok(v.clone());
    }
    // Older CLIs without --json-schema support answer in `result` text.
    let text = result.get("result").and_then(Value::as_str).unwrap_or("");
    crate::extract_json(text)
}

// ---------------------------------------------------------------------------
// Codex
// ---------------------------------------------------------------------------

pub fn codex_args(model: &str, schema_file: &Path, out_file: &Path, image_file: Option<&Path>) -> Vec<String> {
    let mut a: Vec<String> = [
        "exec", "--skip-git-repo-check", "--ephemeral", "--ignore-user-config", "--ignore-rules",
        "--sandbox", "read-only", "--color", "never",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    a.push("--output-schema".into());
    a.push(schema_file.display().to_string());
    a.push("-o".into());
    a.push(out_file.display().to_string());
    if !model.trim().is_empty() {
        a.push("-m".into());
        a.push(model.trim().to_string());
    }
    if let Some(img) = image_file {
        a.push("-i".into());
        a.push(img.display().to_string());
    }
    a.push("-".into());
    a
}

/// Codex has no system prompt flag: instructions and task go into one prompt.
pub fn codex_prompt(system: &str, user: &str) -> String {
    format!("{system}\n\nAufgabe:\n{user}\n\nNutze keine Werkzeuge und führe keine Befehle aus. Antworte nur mit dem JSON.")
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Turn CLI error text into a clear German message.
pub fn classify_failure(cli: Cli, text: &str) -> AiError {
    let t = text.to_lowercase();
    if t.contains("not logged in") || t.contains("/login") || t.contains("login") && t.contains("please")
        || t.contains("unauthorized") || t.contains("401") || t.contains("authentication")
    {
        return AiError::Request(cli.login_hint().into());
    }
    if t.contains("usage limit") || t.contains("rate limit") || t.contains("limit reached") || t.contains("429") {
        return AiError::Request(format!("{} meldet: Nutzungslimit erreicht. Versuche es später erneut.", cli.label()));
    }
    let first = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("unbekannter Fehler");
    let first: String = first.chars().take(200).collect();
    AiError::Request(format!("{} fehlgeschlagen: {first}", cli.label()))
}

// ---------------------------------------------------------------------------
// Process runner
// ---------------------------------------------------------------------------

/// Locate the CLI: PATH first, then the usual install locations (a GUI app often starts
/// with a shorter PATH than a terminal).
pub fn find(cli: Cli) -> Option<PathBuf> {
    let exts: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ""] } else { &[""] };
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    if let Some(h) = &home {
        dirs.push(h.join(".local").join("bin"));
        dirs.push(h.join(".claude").join("local"));
        dirs.push(h.join(".npm-global").join("bin"));
        dirs.push(h.join(".bun").join("bin"));
    }
    if cfg!(windows) {
        if let Some(a) = std::env::var_os("APPDATA") {
            dirs.push(PathBuf::from(a).join("npm"));
        }
        if let Some(l) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(&l).join("Programs").join("OpenAI").join("Codex").join("bin"));
            dirs.push(PathBuf::from(&l).join("Programs").join("claude"));
        }
    } else {
        dirs.push("/opt/homebrew/bin".into());
        dirs.push("/usr/local/bin".into());
    }
    for d in dirs {
        for e in exts {
            let p = d.join(format!("{}{e}", cli.bin()));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

struct Output {
    status_ok: bool,
    stdout: String,
    stderr: String,
}

fn run(cli: Cli, args: &[String], stdin: &str, cwd: &Path, timeout: Duration) -> Result<Output, AiError> {
    let bin = find(cli).ok_or_else(|| AiError::Request(cli.install_hint().into()))?;
    let mut cmd = Command::new(&bin);
    cmd.args(args).current_dir(cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    for k in STRIPPED_ENV {
        cmd.env_remove(k);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| AiError::Request(format!("{} ließ sich nicht starten: {e}", cli.label())))?;
    if let Some(mut si) = child.stdin.take() {
        let data = stdin.to_string();
        std::thread::spawn(move || {
            let _ = si.write_all(data.as_bytes());
        });
    }
    let mut so = child.stdout.take().expect("piped");
    let mut se = child.stderr.take().expect("piped");
    let out_t = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = so.read_to_string(&mut s);
        s
    });
    let err_t = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = se.read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait().map_err(|e| AiError::Request(e.to_string()))? {
            break st;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AiError::Request(format!("{} hat nicht rechtzeitig geantwortet", cli.label())));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    Ok(Output {
        status_ok: status.success(),
        stdout: out_t.join().unwrap_or_default(),
        stderr: err_t.join().unwrap_or_default(),
    })
}

/// A fresh, empty working directory per call, removed afterwards.
struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Result<Self, AiError> {
        let p = std::env::temp_dir().join(format!("ausgeloggt-ai-{}-{}", std::process::id(), nanos()));
        std::fs::create_dir_all(&p).map_err(|e| AiError::Request(format!("Temporärer Ordner: {e}")))?;
        Ok(Self(p))
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn nanos() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
}

/// Structured completion through one of the CLIs.
pub fn complete(
    cli: Cli,
    model: &str,
    system: &str,
    user: &str,
    schema: &Value,
    image: Option<&Image>,
    timeout: Duration,
) -> Result<Value, AiError> {
    let dir = TempDir::new()?;
    match cli {
        Cli::Claude => {
            let out = run(cli, &claude_args(model, system, schema), &claude_input(user, image), &dir.0, timeout)?;
            match parse_claude_output(&out.stdout) {
                Err(AiError::BadResponse(_)) if !out.status_ok && !out.stderr.trim().is_empty() => Err(classify_failure(cli, &out.stderr)),
                r => r,
            }
        }
        Cli::Codex => {
            let schema_file = dir.0.join("schema.json");
            let out_file = dir.0.join("answer.json");
            std::fs::write(&schema_file, strict_schema(schema).to_string()).map_err(|e| AiError::Request(e.to_string()))?;
            let image_file = match image {
                Some(img) => {
                    let p = dir.0.join(if img.mime == "image/png" { "bild.png" } else { "bild.jpg" });
                    std::fs::write(&p, img.bytes).map_err(|e| AiError::Request(e.to_string()))?;
                    Some(p)
                }
                None => None,
            };
            let args = codex_args(model, &schema_file, &out_file, image_file.as_deref());
            let out = run(cli, &args, &codex_prompt(system, user), &dir.0, timeout)?;
            let answer = std::fs::read_to_string(&out_file).unwrap_or_default();
            if answer.trim().is_empty() {
                let text = if out.stderr.trim().is_empty() { &out.stdout } else { &out.stderr };
                return Err(classify_failure(cli, text));
            }
            crate::extract_json(&answer)
        }
    }
}

/// Login check for the settings: `claude auth status` / `codex login status`.
pub fn status(cli: Cli) -> Result<String, AiError> {
    let dir = TempDir::new()?;
    let args: Vec<String> = match cli {
        Cli::Claude => vec!["auth".into(), "status".into()],
        Cli::Codex => vec!["login".into(), "status".into()],
    };
    let out = run(cli, &args, "", &dir.0, Duration::from_secs(30))?;
    parse_status(cli, &out.stdout, &out.stderr, out.status_ok)
}

pub fn parse_status(cli: Cli, stdout: &str, stderr: &str, ok: bool) -> Result<String, AiError> {
    match cli {
        Cli::Claude => {
            let v: Value = serde_json::from_str(stdout.trim()).unwrap_or(Value::Null);
            if v.get("loggedIn").and_then(Value::as_bool) == Some(true) {
                let api = v.get("authMethod").and_then(Value::as_str).is_some_and(|m| m.contains("api") || m.contains("key"));
                if api {
                    return Ok("Claude Code ist mit einem API Schlüssel angemeldet. Für dein Abo melde dich in Claude Code mit deinem Claude Konto an.".into());
                }
                Ok("Claude Code ist mit deinem Claude Konto angemeldet".into())
            } else {
                Err(AiError::Request(cli.login_hint().into()))
            }
        }
        Cli::Codex => {
            let text = format!("{stdout}\n{stderr}");
            let t = text.to_lowercase();
            if ok && t.contains("logged in") && !t.contains("not logged in") {
                if t.contains("api key") {
                    Ok("Codex ist mit einem API Schlüssel angemeldet. Für dein Abo führe codex login mit deinem ChatGPT Konto aus.".into())
                } else {
                    Ok("Codex ist mit deinem ChatGPT Konto angemeldet".into())
                }
            } else {
                Err(AiError::Request(cli.login_hint().into()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_schema_closes_every_object() {
        let s = strict_schema(&crate::classification_schema());
        assert_eq!(s["additionalProperties"], false);
        assert_eq!(s["properties"]["items"]["items"]["additionalProperties"], false);
        // Explicit values are kept.
        let t = strict_schema(&json!({"type": "object", "additionalProperties": true}));
        assert_eq!(t["additionalProperties"], true);
    }

    #[test]
    fn claude_args_are_headless_and_toolless() {
        let a = claude_args("", "sys", &crate::judge_schema());
        let pos = |f: &str| a.iter().position(|x| x == f).unwrap();
        assert_eq!(a[pos("--tools") + 1], "");
        assert_eq!(a[pos("--setting-sources") + 1], "");
        assert_eq!(a[pos("--model") + 1], DEFAULT_CLAUDE_MODEL);
        assert_eq!(a[pos("--system-prompt") + 1], "sys");
        assert!(a.contains(&"--no-session-persistence".to_string()));
        assert!(a[pos("--json-schema") + 1].contains("\"additionalProperties\":false"));
        assert_eq!(claude_args(" sonnet ", "s", &json!({}))[pos("--model") + 1], "sonnet");
    }

    #[test]
    fn claude_input_with_image() {
        let img = Image { bytes: b"x", mime: "image/png", b64: "eA==" };
        let line = claude_input("hallo", Some(&img));
        assert!(line.ends_with('\n'));
        let v: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(v["type"], "user");
        assert_eq!(v["message"]["content"][0]["source"]["data"], "eA==");
        assert_eq!(v["message"]["content"][1]["text"], "hallo");
        let v: Value = serde_json::from_str(claude_input("x", None).trim()).unwrap();
        assert_eq!(v["message"]["content"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn claude_output_parsing() {
        let ok = "{\"type\":\"system\"}\n{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"\",\"structured_output\":{\"probability\":0.9,\"reason\":\"x\"}}\n";
        assert_eq!(parse_claude_output(ok).unwrap()["probability"], 0.9);
        let text = "{\"type\":\"result\",\"subtype\":\"success\",\"result\":\"```json\\n{\\\"a\\\":1}\\n```\"}";
        assert_eq!(parse_claude_output(text).unwrap()["a"], 1);
        let login = "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":true,\"result\":\"Not logged in · Please run /login\"}";
        assert!(parse_claude_output(login).unwrap_err().to_string().contains("nicht angemeldet"));
        assert!(parse_claude_output("garbage").is_err());
    }

    #[test]
    fn codex_args_shape() {
        let a = codex_args("", Path::new("s.json"), Path::new("o.json"), Some(Path::new("b.png")));
        assert_eq!(a[0], "exec");
        for f in ["--ephemeral", "--ignore-user-config", "--skip-git-repo-check"] {
            assert!(a.contains(&f.to_string()), "{f}");
        }
        assert_eq!(a[a.iter().position(|x| x == "--sandbox").unwrap() + 1], "read-only");
        assert!(!a.contains(&"-m".to_string()));
        assert_eq!(a.last().unwrap(), "-");
        assert!(codex_args("gpt-5.5", Path::new("s"), Path::new("o"), None).contains(&"gpt-5.5".to_string()));
    }

    #[test]
    fn failures_are_explained() {
        assert!(classify_failure(Cli::Codex, "Error: 401 Unauthorized").to_string().contains("codex login"));
        assert!(classify_failure(Cli::Claude, "You've hit your usage limit").to_string().contains("Nutzungslimit"));
        assert!(classify_failure(Cli::Claude, "\n  boom\nmore").to_string().contains("boom"));
    }

    #[test]
    fn status_parsing() {
        assert!(parse_status(Cli::Claude, "{\"loggedIn\":true,\"authMethod\":\"claude.ai\"}", "", true).unwrap().contains("Claude Konto"));
        assert!(parse_status(Cli::Claude, "{\"loggedIn\":true,\"authMethod\":\"api_key\"}", "", true).unwrap().contains("API Schlüssel"));
        assert!(parse_status(Cli::Claude, "{\"loggedIn\":false}", "", false).is_err());
        assert!(parse_status(Cli::Codex, "Logged in using ChatGPT", "", true).unwrap().contains("ChatGPT"));
        assert!(parse_status(Cli::Codex, "Not logged in", "", false).is_err());
    }

    /// Real calls; run with `cargo test -p ausgeloggt-ai -- --ignored live_cli`.
    #[test]
    #[ignore]
    fn live_cli_both() {
        for cli in [Cli::Claude, Cli::Codex] {
            println!("{:?}", status(cli));
            let v = complete(cli, "", "Du bewertest Accounts.", "Hat jemand mit einer Willkommensmail von Spotify dort einen Account? Wahrscheinlichkeit 0 bis 1.",
                &crate::judge_schema(), None, Duration::from_secs(180)).unwrap();
            println!("{cli:?}: {v}");
            assert!(crate::parse_judgement(&v).is_ok());
        }
    }
}
