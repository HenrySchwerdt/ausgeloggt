# Contributing

Thanks for helping! Issues and pull requests are welcome.

## Easy ways to help

- **Missing service or wrong deletion link?** Open an issue with the "Service / deletion info" template.
- **A mail type isn't recognised** (welcome, receipt, 2FA …)? Open an issue with an *anonymised* subject line and sender domain. Never paste real personal data.
- **New password-manager export format?** Attach a header row with fake data.

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cd desktop/ui && npm ci && npm run build
```

Rules (see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)):
- Cross-platform, pure-Rust dependencies; TLS via rustls only.
- Privacy first: no telemetry, no third-party requests, never log sensitive data.
- The UI follows [docs/DESIGN.md](docs/DESIGN.md).
- User-facing text (app, README, website) in German; code, comments, developer docs and commits in English.
- Tests for every parser / classifier change, with fake data only.
