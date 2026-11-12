# Sicherheit

ausgeloggt verarbeitet sehr sensible Daten: Postfach-Zugang, Account-Listen und Hinweise auf Zahlungsdaten. Meldungen zu Sicherheitslücken sind deshalb sehr willkommen.

## Sicherheitslücke melden

Bitte **kein öffentliches Issue** anlegen. Nutze stattdessen die private Meldung von GitHub:
**Security → Report a vulnerability** auf https://github.com/HenrySchwerdt/ausgeloggt.

Beschreibe, wie sich das Problem nachstellen lässt, und nenne Version oder Commit. Du bekommst innerhalb von 7 Tagen eine Antwort.

## Was besonders interessiert

- Verschlüsselung und Schlüsselverwaltung des Tresors (`crates/ausgeloggt-core/src/vault.rs`)
- Alles, wodurch Daten das Gerät verlassen könnten: Netzwerk-Code in `ausgeloggt-mail`, `-breach`, `-catalog`, `-ai` und in der Tauri-Shell
- Importer für fremde Dateien: ZIP, CSV, JSON, Path Traversal, Zip-Bomben
- Prompt-Injection in den optionalen KI-Funktionen
- Tauri-IPC und CSP in `desktop/`

## So ist es gebaut

- **Tresor:**
  - Schlüssel per Argon2id (64 MiB, t=3), verschlüsselt mit XChaCha20-Poly1305.
  - Der Header ist als AAD mit authentifiziert.
  - Gespeichert wird atomar, Schlüssel werden im Speicher gelöscht.
- **Passwörter aus Importen** werden nur im Arbeitsspeicher für die Prüfung auf Wiederverwendung und Leaks benutzt. Sie werden nie gespeichert oder geloggt.
- **Pwned Passwords** wird über die k-Anonymitäts-API abgefragt, also nur mit 5 Hex-Zeichen des SHA-1-Hashes und mit Padding.
- **TLS** ausschließlich über rustls, kein OpenSSL.
