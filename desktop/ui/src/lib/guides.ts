// Plain German setup help for mailboxes and password exports. The export texts mirror
// crates/ausgeloggt-import/src/instructions.rs (no IPC command exposes them yet).
import type { PasswordFormat } from "./types";

export interface MailProvider {
  id: string;
  name: string;
  domain: string; // for the logo
  /** Brand glyph key when it differs from the name. */
  logo?: string;
  /** Used when the backend preset list has no entry. */
  host?: string;
  port?: number;
  soon?: boolean;
  /** Address must end with one of these, empty = anything. */
  domains: string[];
  steps: string[];
  /** Direct link for the first step. */
  link?: { url: string; label: string };
  /** Shown when the login is rejected. */
  authHint: string;
  /** Username is the part before the @. */
  userIsLocalPart?: boolean;
}

export const MAIL_PROVIDERS: MailProvider[] = [
  {
    id: "gmail", name: "Gmail", domain: "gmail.com", logo: "google", domains: ["gmail.com", "googlemail.com"],
    steps: [
      "Schalte bei Google die Bestätigung in zwei Schritten ein, falls noch nicht geschehen.",
      "Öffne die Seite für App Passwörter, vergib den Namen ausgeloggt und klicke auf Erstellen.",
      "Kopiere die 16 Zeichen und füge sie unten ein.",
    ],
    link: { url: "https://myaccount.google.com/apppasswords", label: "App Passwörter öffnen" },
    authHint: "Gmail nimmt nur ein App Passwort, nicht dein normales Passwort. Erzeuge eines und füge es ohne Leerzeichen ein.",
  },
  {
    id: "webde", name: "WEB.DE", domain: "web.de", domains: ["web.de"],
    steps: [
      "Melde dich bei web.de an und öffne Einstellungen, dann POP3 und IMAP.",
      "Aktiviere „IMAP und POP3 Zugriff erlauben“ und speichere.",
      "Trage unten dein normales Passwort ein. Mit Zwei Faktor Anmeldung brauchst du ein App Passwort unter Sicherheit.",
    ],
    link: { url: "https://web.de", label: "web.de öffnen" },
    authHint: "Bei WEB.DE ist IMAP oft ausgeschaltet. Öffne Einstellungen, POP3 und IMAP und erlaube den Zugriff für externe Programme. Prüfe danach das Passwort.",
  },
  {
    id: "gmx", name: "GMX", domain: "gmx.net", domains: ["gmx.de", "gmx.net", "gmx.at", "gmx.ch"],
    steps: [
      "Melde dich bei GMX an und öffne Einstellungen, dann E-Mail, dann POP3 und IMAP.",
      "Aktiviere „Zugriff auf das Postfach über POP3 und IMAP“ und speichere.",
      "Trage unten dein normales Passwort ein. Mit Zwei Faktor Anmeldung brauchst du ein App Passwort.",
    ],
    link: { url: "https://gmx.net", label: "GMX öffnen" },
    authHint: "Bei GMX ist IMAP oft ausgeschaltet. Öffne Einstellungen, E-Mail, POP3 und IMAP und erlaube den Zugriff. Prüfe danach das Passwort.",
  },
  {
    id: "outlook", name: "Outlook", domain: "outlook.com", logo: "microsoft", soon: true, domains: ["outlook.com", "outlook.de", "hotmail.com", "hotmail.de", "live.de"],
    steps: [], authHint: "",
  },
  {
    id: "icloud", name: "iCloud", domain: "icloud.com", logo: "apple", domains: ["icloud.com", "me.com", "mac.com"], userIsLocalPart: true,
    steps: [
      "Öffne die Apple Kontoseite und gehe zu Anmelden und Sicherheit, dann App spezifische Passwörter.",
      "Erstelle ein Passwort mit dem Namen ausgeloggt.",
      "Füge es unten ein. Als Benutzername gilt nur der Teil vor dem @.",
    ],
    link: { url: "https://account.apple.com/account/manage", label: "Apple Konto öffnen" },
    authHint: "iCloud braucht ein App spezifisches Passwort und als Benutzername nur den Teil vor dem @.",
  },
  {
    id: "yahoo", name: "Yahoo", domain: "yahoo.com", domains: ["yahoo.com", "yahoo.de"],
    steps: [
      "Öffne bei Yahoo die Kontosicherheit.",
      "Erzeuge ein App Passwort für „Andere App“.",
      "Füge es unten ein.",
    ],
    link: { url: "https://login.yahoo.com/account/security", label: "Kontosicherheit öffnen" },
    authHint: "Yahoo nimmt nur ein App Passwort. Erzeuge eines in der Kontosicherheit.",
  },
  {
    id: "tonline", name: "T-Online", domain: "t-online.de", logo: "deutschetelekom", domains: ["t-online.de"],
    steps: [
      "Öffne das Kundencenter und gehe zu E-Mail Einstellungen.",
      "Lege ein Passwort für E-Mail Programme an.",
      "Füge es unten ein. Das normale Passwort funktioniert nicht.",
    ],
    link: { url: "https://www.telekom.de/hilfe/festnetz-internet-tv/e-mail/e-mail-programme", label: "Anleitung der Telekom" },
    authHint: "T-Online verlangt ein eigenes Passwort für Mailprogramme. Lege es im Kundencenter an.",
  },
  {
    id: "posteo", name: "Posteo", domain: "posteo.de", domains: ["posteo.de", "posteo.net"],
    steps: ["Es gilt das normale Passwort deines Postfachs.", "Trage unten Adresse und Passwort ein."],
    authHint: "Prüfe Adresse und Passwort. Bei Posteo gilt das normale Passwort.",
  },
  {
    id: "mailbox", name: "mailbox.org", domain: "mailbox.org", domains: ["mailbox.org"],
    steps: [
      "Öffne die Einstellungen von mailbox.org und gehe zu Sicherheit.",
      "Mit aktiver Zwei Faktor Anmeldung legst du ein App Passwort an, sonst gilt das normale Passwort.",
      "Trage es unten ein.",
    ],
    link: { url: "https://login.mailbox.org", label: "mailbox.org öffnen" },
    authHint: "Mit aktiver Zwei Faktor Anmeldung brauchst du ein App Passwort. Lege es unter Sicherheit an.",
  },
  {
    id: "proton", name: "Proton", domain: "proton.me", host: "127.0.0.1", port: 1143, domains: [],
    steps: [
      "Installiere Proton Mail Bridge und melde dich dort an. Die Bridge muss laufen.",
      "Öffne in der Bridge die Konto Einstellungen und kopiere Benutzername und Passwort für Mailprogramme.",
      "Füge beides unten ein. Der Server ist 127.0.0.1, Port 1143.",
    ],
    link: { url: "https://proton.me/mail/bridge", label: "Proton Bridge laden" },
    authHint: "Nimm das Passwort aus der Bridge, nicht dein Proton Passwort. Die Bridge muss laufen.",
  },
  {
    id: "other", name: "Andere", domain: "", domains: [],
    steps: [
      "Suche in der Hilfe deines Anbieters nach „IMAP Server“ und notiere Servername und Port.",
      "Falls dein Anbieter App Passwörter anbietet, erzeuge eines.",
      "Trage alles unten ein.",
    ],
    authHint: "Prüfe Benutzername und Passwort. Manche Anbieter verlangen ein App Passwort oder müssen IMAP in den Einstellungen freigeben.",
  },
];

export interface PasswordManager {
  id: string;
  name: string;
  domain: string;
  logo?: string;
  format: PasswordFormat | null;
  steps: string[];
  link?: { url: string; label: string };
}

export const PASSWORD_MANAGERS: PasswordManager[] = [
  { id: "chrome", name: "Chrome", domain: "google.com/chrome", logo: "googlechrome", format: "Chrome", steps: [
    "Öffne chrome://password-manager/settings in der Adresszeile.",
    "Klicke bei „Passwörter exportieren“ auf „Datei herunterladen“ und bestätige mit deinem Gerätepasswort.",
    "Wähle die gespeicherte Datei (Passwörter.csv) unten aus.",
  ] },
  { id: "firefox", name: "Firefox", domain: "firefox.com", logo: "firefoxbrowser", format: "Firefox", steps: [
    "Öffne about:logins in der Adresszeile.",
    "Klicke oben rechts auf die drei Punkte, dann auf „Logins exportieren“, und bestätige.",
    "Wähle die gespeicherte Datei (logins.csv) unten aus.",
  ] },
  { id: "safari", name: "Safari / Apple Passwörter", domain: "apple.com", logo: "apple", format: "Safari", steps: [
    "Öffne auf dem Mac die App „Passwörter“ (macOS 15 oder neuer) oder Safari, Einstellungen, Passwörter.",
    "Wähle im Menü Ablage „Alle Passwörter exportieren“ und bestätige mit Touch ID.",
    "Wähle die gespeicherte CSV Datei unten aus. Auf dem iPhone ist kein Export vorgesehen.",
  ] },
  { id: "edge", name: "Edge", domain: "microsoft.com/edge", logo: "microsoft", format: "Edge", steps: [
    "Öffne edge://wallet/passwords in der Adresszeile.",
    "Klicke bei „Gespeicherte Kennwörter“ auf die drei Punkte, dann auf „Kennwörter exportieren“.",
    "Wähle die gespeicherte Datei unten aus. Ein Dateiname mit „Edge“ hilft beim Erkennen.",
  ] },
  { id: "bitwarden", name: "Bitwarden", domain: "bitwarden.com", format: "Bitwarden", link: { url: "https://vault.bitwarden.com", label: "Tresor öffnen" }, steps: [
    "Öffne den Webtresor unter vault.bitwarden.com, dann Tools, dann Tresor exportieren.",
    "Wähle das Format .csv oder .json, nicht „.json (verschlüsselt)“, und gib dein Masterpasswort ein.",
    "Wähle die gespeicherte Datei unten aus.",
  ] },
  { id: "1password", name: "1Password", domain: "1password.com", format: "OnePassword", steps: [
    "Öffne die 1Password App und wähle das gewünschte Konto.",
    "Wähle Datei, Exportieren, Format CSV, und bestätige mit dem Masterpasswort.",
    "Wähle die gespeicherte CSV Datei unten aus.",
  ] },
  { id: "keepass", name: "KeePass", domain: "keepassxc.org", logo: "keepassxc", format: "KeePass", steps: [
    "Öffne die Datenbank in KeePassXC.",
    "Wähle Datenbank, Exportieren, „In CSV Datei“. In KeePass: Datei, Exportieren, KeePass CSV.",
    "Wähle die gespeicherte CSV Datei unten aus.",
  ] },
  { id: "lastpass", name: "LastPass", domain: "lastpass.com", format: "LastPass", link: { url: "https://lastpass.com", label: "LastPass öffnen" }, steps: [
    "Melde dich im LastPass Tresor an und öffne Erweiterte Optionen.",
    "Wähle „Exportieren“ und bestätige mit deinem Masterpasswort.",
    "Speichere den angezeigten Text als CSV Datei und wähle sie unten aus.",
  ] },
  { id: "proton", name: "Proton Pass", domain: "proton.me", format: "Proton", steps: [
    "Öffne Proton Pass und gehe zu Einstellungen, dann Exportieren.",
    "Wähle das Format CSV, nicht PGP, und bestätige mit deinem Passwort.",
    "Wähle die gespeicherte CSV Datei unten aus.",
  ] },
  { id: "dashlane", name: "Dashlane", domain: "dashlane.com", format: "Dashlane", steps: [
    "Öffne Dashlane und gehe zu Einstellungen, dann Meine Daten exportieren.",
    "Wähle CSV und bestätige mit deinem Masterpasswort.",
    "Wähle die gespeicherte Datei unten aus.",
  ] },
  { id: "other", name: "Andere", domain: "", format: null, steps: [
    "Die meisten Passwortmanager bieten einen Export als CSV Datei an, meist unter Einstellungen.",
    "Eine Textdatei mit einer Webadresse pro Zeile geht auch.",
    "Wähle die Datei unten aus.",
  ] },
];
