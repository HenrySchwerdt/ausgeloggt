# Mitmachen

Danke, dass du helfen willst! Issues und Pull Requests sind willkommen.

## Einfache Wege zu helfen

- **Ein Dienst fehlt oder der Lösch-Link stimmt nicht?** Lege ein Issue mit der Vorlage „Dienst / Lösch-Info“ an.
- **Eine Mail wird nicht erkannt** (Willkommen, Rechnung, 2FA …)? Lege ein Issue mit *anonymisierter* Betreffzeile und Absender-Domain an. Bitte nie echte persönliche Daten einfügen.
- **Neues Exportformat eines Passwort-Managers?** Hänge die Kopfzeile mit erfundenen Daten an.

## Entwicklung

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cd desktop/ui && npm ci && npm run build
```

Die Regeln stehen in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md):
- Plattformübergreifende Abhängigkeiten in reinem Rust, TLS nur über rustls.
- Datenschutz zuerst: keine Telemetrie, keine Anfragen an Dritte, nie sensible Daten loggen.
- Die Oberfläche folgt [docs/DESIGN.md](docs/DESIGN.md).
- Texte für Nutzer auf Deutsch, Code und Kommentare auf Englisch.
- Tests für jede Änderung an Parsern und an der Erkennung, nur mit erfundenen Daten.
