# Security policy

ausgeloggt handles very sensitive data (mailbox access, account lists, payment hints), so security reports are very welcome.

## Reporting a vulnerability

Please **do not open a public issue**. Use GitHub's private vulnerability reporting instead:
**Security → Report a vulnerability** on https://github.com/HenrySchwerdt/ausgeloggt.

Include steps to reproduce and the affected version or commit. You will get an answer within 7 days.

## Scope

- Vault encryption and key handling (`crates/ausgeloggt-core/src/vault.rs`)
- Anything that could leak data off the device: network code in `ausgeloggt-mail`, `-breach`, `-catalog`, `-ai` and the Tauri shell
- Importers handling untrusted files (ZIP/CSV/JSON parsing, path traversal, decompression bombs)
- Prompt-injection paths into the optional AI features
- Tauri IPC / CSP issues in `desktop/`

## Design notes

- Vault: Argon2id (64 MiB, t=3) → XChaCha20-Poly1305, header authenticated as AAD, atomic writes, keys zeroized.
- Passwords from imports are used only in memory for reuse/pwned checks and are never stored or logged.
- Pwned Passwords uses the k-anonymity range API (5 hex chars of the SHA-1) with padding.
- TLS only via rustls; no OpenSSL.
