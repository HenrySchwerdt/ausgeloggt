<div align="center">

# ausgeloggt

**Finde jeden deiner Accounts im Netz und melde dich für immer ab.**

Eine App für Windows, macOS und Linux. Sie findet deine Accounts, Abos und hinterlegten Zahlungsdaten,
prüft Sicherheitsrisiken und hilft dir beim Löschen, Abmelden, Kündigen und Durchsetzen deiner Rechte nach der DSGVO.
Alles bleibt auf deinem Rechner.

[Website](https://henryschwerdt.github.io/ausgeloggt/) · [Sicherheit](SECURITY.md) · [Mitmachen](CONTRIBUTING.md)

![Plattformen](https://img.shields.io/badge/Plattformen-Windows%20%7C%20macOS%20%7C%20Linux-18181b)
![Lizenz](https://img.shields.io/badge/Lizenz-MIT-18181b)
[![CI](https://github.com/HenrySchwerdt/ausgeloggt/actions/workflows/ci.yml/badge.svg)](https://github.com/HenrySchwerdt/ausgeloggt/actions/workflows/ci.yml)

</div>

> **Status: frühe Entwicklung.** Das Backend ist fertig und getestet. Die Oberfläche wird gerade angebunden
> und läuft noch mit Beispieldaten. Installer gibt es noch nicht.

## Was die App kann

**Finden**
- Durchsucht dein Postfach per IMAP. Gelesen werden nur Kopfzeilen, und nichts wird verändert.
- Erkennt Mails zur Begrüßung, Bestätigung, Anmeldung, Passwortänderung und 2FA.
- Liest Passwortexporte aus Chrome, Firefox, Safari, Apple Passwörter, Edge, Bitwarden, 1Password, KeePass, LastPass, Proton Pass und Dashlane. Passwörter werden **nie** gespeichert.
- Erkennt Anmeldungen über Google, Apple, Microsoft, GitHub und Facebook, auch Adressen von Apples „E-Mail-Adresse verbergen“.
- Liest Screenshots von Listen ohne Export, etwa „Mit Apple anmelden“, mit lokaler KI über Ollama.
- Importiert Datenexporte von Facebook und Google Takeout.

**Verstehen**
- Wo deine **Zahlungsdaten** liegen: „Visa •••• 1234 ist bei 14 Diensten hinterlegt“.
- **Bezahlte Abos** mit Betrag, Rhythmus, nächster Abbuchung und Preiserhöhungen. Auch Rechnungen von Apple, Google Play und PayPal werden pro App aufgeschlüsselt.
- **Sicherheit:** wiederverwendete und geleakte Passwörter, Datenlecks bei Diensten, fehlende 2FA und Risiken für eine Übernahme deiner Accounts. Für die Prüfung auf Leaks gehen nur 5 Zeichen eines Hashes an Have I Been Pwned.
- Eine **Rangliste der Risiken** mit dem nächsten konkreten Schritt für jeden Account.

**Handeln**
- Löschlinks und Schwierigkeitsgrad für rund 2.700 Dienste.
- Newsletter mit einem Klick abbestellen (RFC 8058). Auf Wunsch sortiert eine KI Werbung von echten Newslettern.
- Belege, wenn nach der Abmeldung weiter Mails kommen.
- **Briefe als PDF nach DIN 5008**, passend für Fensterkuverts:
  - Löschung, Auskunft und Widerspruch nach DSGVO
  - Kündigung
  - Unterlassungsaufforderung mit Belegen
  - Selbstauskunft bei SCHUFA und anderen Auskunfteien
  - Beschwerde bei der Datenschutzbehörde
- Fristen für jeden Antrag im Blick.

## Datenschutz von Anfang an

| | |
|---|---|
| **Nur lokal** | Kein Account, kein Server, keine Telemetrie. Deine Daten liegen verschlüsselt in einer Tresordatei (Argon2id, XChaCha20Poly1305). |
| **Wenig lesen** | Von Mails nur die Kopfzeilen. Inhalte nur bei Rechnungen und nur, wenn du zustimmst. Davon bleiben nur Betrag, Tarif, Kartentyp und die letzten 4 Ziffern. |
| **Netzwerk** | Nur dein eigener Mailserver, auf Wunsch Have I Been Pwned und Updates des Katalogs, und Logos direkt von der Seite des Dienstes. Keine Tracker, keine fremden Dienste. |
| **KI nach Wahl** | Am besten lokal über Ollama. Die Claude API ist nur auf Wunsch aktiv und bekommt nur Absender und Betreffzeilen, nie den Inhalt einer Mail. |

## Selbst bauen

Du brauchst Rust (stable), Node.js 20 oder neuer und die [Voraussetzungen für Tauri](https://v2.tauri.app/start/prerequisites/) auf deinem System.

```bash
git clone https://github.com/HenrySchwerdt/ausgeloggt.git
cd ausgeloggt
cargo test --workspace          # backend tests

cd desktop/ui
npm ci
npm run dev                     # UI in the browser with demo data: http://localhost:1420/?mock
npm run tauri dev               # desktop app
```

## Aufbau

| Crate | Aufgabe |
|---|---|
| `ausgeloggt-core` | Datenmodell, verschlüsselter Tresor, Zusammenführung, Auswertungen zu Abos, Risiken und Score |
| `ausgeloggt-mail` | IMAP Client mit rustls, Erkennung von Mails, Rechnungen, Abmelden nach RFC 8058 |
| `ausgeloggt-import` | Passwortexporte und Datenexporte, Prüfung der Passwörter nur im Arbeitsspeicher |
| `ausgeloggt-breach` | Have I Been Pwned: Pwned Passwords und Katalog der Datenlecks |
| `ausgeloggt-catalog` | Löschkatalog, 2FA Verzeichnis, Auskunfteien, Behörden, Briefe zur Löschung |
| `ausgeloggt-letters` | Briefe als PDF nach DIN 5008, digitaler Nachlass, Bericht zum Datenschutz |
| `ausgeloggt-ai` | KI auf Wunsch über Ollama, Claude oder ChatGPT (über Claude Code und Codex, ohne API Schlüssel), Import von Screenshots |
| `desktop/` | Tauri 2 mit Oberfläche aus React, TypeScript und Tailwind |

Mehr dazu in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) und [docs/DESIGN.md](docs/DESIGN.md).

## Rechtlicher Hinweis

Die Briefe sind **Vorlagen und keine Rechtsberatung**. Bitte prüfe sie vor dem Versand.
Bei größeren Ansprüchen helfen die Verbraucherzentrale oder ein Anwalt.

## Danke

- **Löschdaten:** [JustDeleteMe](https://github.com/jdm-contrib/jdm) (MIT)
- **Firmenkontakte:** [Datenanfragen.de](https://github.com/datenanfragen/data) (CC0)
- **2FA Daten:** [2fa.directory](https://github.com/2factorauth/twofactorauth) (MIT)
- **Logos:** [Simple Icons](https://simpleicons.org) (CC0)
- **Datenlecks:** [Have I Been Pwned](https://haveibeenpwned.com)
- **Schriften:** Inter, Liberation (OFL)

Markennamen und Logos gehören ihren Inhabern und dienen nur dazu, die Dienste zu erkennen.

## Lizenz

[MIT](LICENSE)
