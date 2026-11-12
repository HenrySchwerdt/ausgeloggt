<div align="center">

# ausgeloggt

**Finde jeden deiner Online-Accounts und melde dich für immer ab.**

Eine Desktop-App, die deine Accounts, Abos und hinterlegten Zahlungsdaten findet, Sicherheitsrisiken prüft
und dir beim Löschen, Abmelden, Kündigen und Durchsetzen deiner DSGVO-Rechte hilft.
Alles bleibt auf deinem Rechner.

[Website](https://henryschwerdt.github.io/ausgeloggt/) · [Sicherheit](SECURITY.md) · [Mitmachen](CONTRIBUTING.md)

![Plattformen](https://img.shields.io/badge/Plattformen-Windows%20%7C%20macOS%20%7C%20Linux-18181b)
![Lizenz](https://img.shields.io/badge/Lizenz-MIT-18181b)
[![CI](https://github.com/HenrySchwerdt/ausgeloggt/actions/workflows/ci.yml/badge.svg)](https://github.com/HenrySchwerdt/ausgeloggt/actions/workflows/ci.yml)

</div>

> **Status: frühe Entwicklung.** Das Backend ist fertig und getestet, die Oberfläche wird gerade angebunden
> und läuft noch mit Beispieldaten. Installer gibt es noch nicht. Folge dem Repo, um das erste Release nicht zu verpassen.

## Was die App kann

**Finden**
- Postfach-Scan per IMAP: **nur Kopfzeilen**, nur lesend. Erkennt Willkommens-, Bestätigungs-, Reset-, Login- und 2FA-Mails.
- Import von Passwort-Exporten aus Chrome, Firefox, Safari/Apple Passwörter, Edge, Bitwarden, 1Password, KeePass, LastPass, Proton Pass und Dashlane. Passwörter werden **nie** gespeichert.
- Erkennt „Mit Google / Apple / Microsoft / GitHub / Facebook anmelden“, auch Adressen von Apples „E-Mail-Adresse verbergen“.
- Liest Screenshots von SSO-Listen ohne Export, etwa „Mit Apple anmelden“, mit einem **lokalen** KI-Modell über Ollama.
- Importiert Datenexporte von Facebook und Google Takeout.

**Verstehen**
- Wo deine **Zahlungsdaten** liegen: „Visa •••• 1234 ist bei 14 Diensten hinterlegt“.
- **Bezahlte Abos** mit Betrag, Rhythmus, nächster Abbuchung und Preiserhöhungen, auch aus den Sammelrechnungen von Apple, Google Play und PayPal.
- **Sicherheit:**
  - Wiederverwendete und geleakte Passwörter, anonym über Have I Been Pwned geprüft. Nur 5 Zeichen des Hashes verlassen dein Gerät.
  - Datenlecks bei Diensten, fehlende 2FA und Übernahme-Risiken.
- **Risiko-Ranking** mit dem nächsten konkreten Schritt pro Account, dazu ein Datenschutz-Score.

**Handeln**
- Lösch-Links und Schwierigkeitsgrad für rund 2.700 Dienste.
- Newsletter mit einem Klick abbestellen (RFC 8058), optional mit KI-Sortierung in Werbung und echte Newsletter.
- Belege, wenn nach der Abmeldung weiter Mails kommen.
- **Briefe als PDF nach DIN 5008:**
  - Löschung, Auskunft und Widerspruch nach DSGVO
  - Kündigung
  - Unterlassungsaufforderung mit Beleg
  - Selbstauskunft bei SCHUFA & Co.
  - Beschwerde bei der Datenschutzbehörde
- Fristen-Tracking für jeden Antrag.

## Datenschutz von Anfang an

| | |
|---|---|
| **Nur lokal** | Kein Account, kein Server, keine Telemetrie. Deine Daten liegen in einer verschlüsselten Tresor-Datei (Argon2id + XChaCha20-Poly1305). |
| **Minimal lesen** | Der Mail-Scan liest nur Kopfzeilen. Inhalte nur bei Rechnungen und nur, wenn du zustimmst. Behalten werden dann nur Betrag, Tarif sowie Kartentyp und die letzten 4 Ziffern. |
| **Netzwerk** | Nur dein eigener Mailserver, optional Have I Been Pwned, optional die Katalog-Aktualisierung und Logos direkt von der Domain des Dienstes. Nie Tracker oder fremde Favicon-Dienste. |
| **KI ist optional** | Empfohlen lokal über Ollama. Die Claude-API ist nur auf Wunsch aktiv und bekommt ausschließlich Absender und Betreffzeilen, geschwärzt, nie Mail-Inhalte. |

## Selbst bauen

Du brauchst Rust (stable), Node.js 20+ und die [Tauri-Voraussetzungen](https://v2.tauri.app/start/prerequisites/) für dein Betriebssystem.

```bash
git clone https://github.com/HenrySchwerdt/ausgeloggt.git
cd ausgeloggt
cargo test --workspace          # Backend-Tests

cd desktop/ui
npm ci
npm run dev                     # Oberfläche im Browser mit Beispieldaten: http://localhost:1420/?mock
npm run tauri dev               # Desktop-App
```

## Aufbau

| Crate | Aufgabe |
|---|---|
| `ausgeloggt-core` | Datenmodell, verschlüsselter Tresor, Zusammenführung, Auswertungen (Abos, Risiko, Score) |
| `ausgeloggt-mail` | IMAP-Client mit rustls, Klassifizierung, Rechnungen, Abmelden nach RFC 8058 |
| `ausgeloggt-import` | Passwort- und Datenexporte, Passwort-Prüfung nur im Arbeitsspeicher |
| `ausgeloggt-breach` | Have I Been Pwned (Pwned-Passwords-Range-API, Datenleck-Katalog) |
| `ausgeloggt-catalog` | Lösch-Katalog, 2FA-Verzeichnis, Auskunfteien, Behörden, DSGVO-Briefe |
| `ausgeloggt-letters` | PDF-Briefe nach DIN 5008, digitaler Nachlass, Datenschutz-Bericht |
| `ausgeloggt-ai` | Optionale KI über Ollama oder Anthropic, Screenshot-Import |
| `desktop/` | Tauri-2-Shell und Oberfläche mit React, TypeScript und Tailwind |

Mehr dazu in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) und [docs/DESIGN.md](docs/DESIGN.md).

## Rechtlicher Hinweis

Die Briefe sind **Vorlagen und keine Rechtsberatung**. Bitte prüfe sie vor dem Versand.
Bei größeren Ansprüchen helfen die Verbraucherzentrale oder ein Anwalt.

## Danke

- **Lösch-Daten:** [JustDeleteMe](https://github.com/jdm-contrib/jdm) (MIT)
- **Firmenkontakte:** [Datenanfragen.de](https://github.com/datenanfragen/data) (CC0)
- **2FA-Daten:** [2fa.directory](https://github.com/2factorauth/twofactorauth) (MIT)
- **Logos:** [Simple Icons](https://simpleicons.org) (CC0)
- **Datenlecks:** [Have I Been Pwned](https://haveibeenpwned.com)
- **Schriften:** Inter, Liberation (OFL)

Markennamen und Logos gehören ihren Inhabern und dienen nur zur Kennzeichnung der Dienste.

## Lizenz

[MIT](LICENSE)
