# Spurlos – Architektur

Spurlos findet deine Online-Accounts (Postfach, Passwort-Manager, SSO, Datenexporte)
und hilft beim Löschen (Direktlink, DSGVO-Antrag, Fristen-Tracking). Alles lokal,
alles in einem verschlüsselten Tresor. Native App mit egui/eframe für Windows, macOS und Linux.

## Crates

| Crate | Aufgabe |
|---|---|
| `crates/spurlos-core` | Datenmodell (`model.rs`, **der Vertrag zwischen allen Crates**), Domain-Normalisierung, Zusammenführung (`merge`), verschlüsselter Tresor (`vault`) |
| `crates/spurlos-import` | Passwort-Exporte (Chrome, Firefox, Safari, Bitwarden, 1Password, KeePass …), Google Takeout, Facebook-Export, generische Listen → `Finding`s |
| `crates/spurlos-mail` | IMAP-Scan (nur Header) + Klassifizierung (Willkommen, Verifizierung, Reset, Login-Alert, SSO-Grant, Newsletter …) → `Finding`s |
| `crates/spurlos-catalog` | Lösch-Katalog (Link, Schwierigkeit, Kontakt) + DSGVO-Briefe (Art. 17), `mailto:`, Fristen |
| `crates/spurlos-ai` | Optionale KI-Einordnung von Newslettern (Werbung/Newsletter/Spam/wichtig) und unsicheren Accounts, lokal per Ollama oder opt-in per Anthropic API |
| `app` | egui-Oberfläche, Hintergrund-Jobs, Zustandsverwaltung |

Datenfluss: Quelle → `Vec<Finding>` → `merge::merge_findings(&mut VaultData, …)` → `Vault::save()`.
Newsletter: Mail-Scan → `Vec<SubscriptionObservation>` → `merge::merge_subscriptions` → optional `spurlos_ai::classify_subscriptions` → Abmelden über `spurlos_mail::unsubscribe` (nur One-Click-POST und mailto automatisch, reine Links nur per Klick im Browser).

## Regeln für alle Beiträge

1. **Plattformübergreifend:** Nur Pure-Rust-Abhängigkeiten oder solche, die ohne Systembibliotheken
   auf Windows/macOS/Linux bauen. TLS nur über `rustls` mit dem `ring`-Provider (kein OpenSSL,
   kein `native-tls`, kein `aws-lc-rs`, weil das cmake/nasm braucht). Keine plattformspezifischen APIs ohne
   `cfg`-Fallback.
2. **Datenschutz:** Passwörter, Notizen, TOTP-Secrets und Kartendaten aus Exporten werden nie gespeichert.
   Von Mails werden nur Header gelesen (Ausnahme: SSO-Sicherheitsmails). Keine Telemetrie, keine
   Netzwerkanfragen außer an IMAP-Server des Nutzers und (abschaltbar) den Katalog-Refresh.
   Nichts Sensibles loggen.
3. **Nicht blockieren:** Alles Langsame (Argon2, IMAP, Netzwerk, große Dateien) läuft in Worker-Threads;
   die UI kommuniziert per Kanal.
4. **API-Stabilität:** Die öffentlichen Signaturen in den `lib.rs`/`model.rs`-Stubs sind der Vertrag.
   Hinzufügen ist erlaubt, Ändern bestehender Signaturen nur nach Absprache.
5. **Qualität:** `cargo clippy --workspace --all-targets` ohne Warnungen, `cargo test --workspace` grün,
   `cargo fmt`. Unit-Tests für jede Parser-/Klassifizierungslogik mit realistischen Fixtures
   (keine echten personenbezogenen Daten).
6. Nutzer-sichtbare Texte auf **Deutsch**, Code/Kommentare auf Englisch.
