# ausgeloggt – Architecture

ausgeloggt finds a user's online accounts (mailbox, password managers, SSO, data exports), payments,
subscriptions and security risks, and helps clean them up (deletion links, GDPR letters, unsubscribing,
deadline tracking). Everything runs locally and is stored in one encrypted vault. Desktop app built with
**Tauri 2** (Rust backend + React/TypeScript/Tailwind UI) for Windows, macOS and Linux.

## Crates

| Crate | Purpose |
|---|---|
| `crates/ausgeloggt-core` | Data model (`model.rs`, **the contract between all crates**), domain normalisation, merging (`merge`), derived insights (`insights`: billing, risk, score, aliases), encrypted vault (`vault`) |
| `crates/ausgeloggt-import` | Password-manager exports (Chrome, Firefox, Safari, Bitwarden, 1Password, KeePass …), Google Takeout, Facebook export, generic lists → `Finding`s; in-memory password audit |
| `crates/ausgeloggt-mail` | IMAP scan (headers only) + classification (welcome, verification, reset, login alert, SSO grant, 2FA, newsletter …), receipts and subscriptions, RFC 8058 unsubscribe |
| `crates/ausgeloggt-catalog` | Deletion catalog (link, difficulty, contact), 2FA directory, credit bureaus / address brokers, data-protection authorities, help links, GDPR erasure letters |
| `crates/ausgeloggt-breach` | Have I Been Pwned: Pwned Passwords (k-anonymity), breach catalogue, optional per-address lookups |
| `crates/ausgeloggt-letters` | Letters as `LetterDoc`, rendered to text or DIN 5008 PDF; legacy kit, privacy report |
| `crates/ausgeloggt-ai` | Optional AI: newsletter classification, uncertain accounts, SSO screenshot reading, receipt fallback. Local via Ollama, or opt-in Claude (Claude Code CLI) and ChatGPT (Codex CLI) on the user's own login, no API keys |
| `desktop/src-tauri` | Tauri shell: holds the unlocked vault, exposes the crates as typed IPC commands, background jobs with progress events, logo cache |
| `desktop/ui` | UI: React + TypeScript + Vite + Tailwind v4 + shadcn/ui (Radix) + lucide-react. Also runs in the browser with a mock API and demo data (screenshots / design iteration) |

Data flow: source → `Vec<Finding>` → `merge::merge_findings(&mut VaultData, …)` → `Vault::save()`.
Newsletters: mail scan → `Vec<SubscriptionObservation>` → `merge::merge_subscriptions` → optional
`ausgeloggt_ai::classify_subscriptions` → unsubscribe via `ausgeloggt_mail::unsubscribe` (only one-click
POST and mailto are automated; plain links are opened in the browser on explicit click).
Payments: mail scan → `Vec<PaymentObservation>` → `merge::merge_payments` → `insights::billing_summary`.

## Rules for all contributions

1. **Cross-platform:** only pure-Rust dependencies or ones that build without system libraries on
   Windows/macOS/Linux. TLS only via `rustls` with the `ring` provider (no OpenSSL, no `native-tls`,
   no `aws-lc-rs`, which needs cmake/nasm). No platform-specific APIs without a `cfg` fallback.
2. **Privacy:** passwords, notes, TOTP secrets and card numbers from exports are never stored.
   Mail: headers only (exceptions: SSO security mails; receipt bodies when the user opts in, keeping only
   amount, plan and card brand + last 4). No telemetry; network requests only to the user's mail servers,
   optional HIBP, optional catalog refresh and logos from the service's own domain. Never log anything sensitive.
3. **Never block:** everything slow (Argon2, IMAP, network, large files) runs in worker threads; the UI
   talks to them via channels / events.
4. **API stability:** public signatures in `lib.rs` / `model.rs` are the contract. Additive changes are
   fine (new fields with `#[serde(default)]`); changing existing signatures needs agreement.
5. **Quality:** `cargo clippy --workspace --all-targets -- -D warnings` clean, `cargo test --workspace`
   green, `cargo fmt`. Unit tests for every parser / classifier with realistic fixtures (fake data only).
6. **Language:** user-facing text (app, README, website) in **German**; code, comments, developer docs,
   issues and commits in English.

## UI (desktop/ui)

- Design rules: `docs/DESIGN.md` (binding).
- Custom titlebar (`decorations: false` on Windows/Linux with own window controls; macOS
  `titleBarStyle: Overlay` with native traffic lights).
- All backend calls go through `src/lib/api.ts` (one interface) with two implementations: `tauri`
  (invoke / channels) and `mock` (demo data, browser).
- No external resources at runtime (fonts, icons and logos bundled), strict CSP.
- Logos: offline from `simple-icons` (CC0); missing ones optionally fetched from the service's own domain by
  the Rust backend (never via third-party favicon services), cached encrypted (`Vault::seal_bytes`).
