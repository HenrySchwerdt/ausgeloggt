//! German export instructions shown in the UI.

pub(crate) fn all() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "Chrome",
            "1. Öffne in Chrome chrome://password-manager/settings (oder Menü > Google Passwort-Manager > Einstellungen).\n\
             2. Klicke bei „Passwörter exportieren“ auf „Datei herunterladen“ und bestätige mit deinem Gerätepasswort.\n\
             3. Speichere die Datei (Passwörter.csv) und ziehe sie in ausgeloggt.\n\
             4. Lösche die CSV-Datei danach sicher: Sie enthält deine Passwörter im Klartext.",
        ),
        (
            "Firefox",
            "1. Öffne about:logins.\n\
             2. Klicke oben rechts auf die drei Punkte > „Logins exportieren …“ und bestätige.\n\
             3. Speichere die CSV-Datei (logins.csv) und importiere sie in ausgeloggt.\n\
             4. Lösche die Datei danach: Sie enthält Passwörter im Klartext.",
        ),
        (
            "Safari / Apple Passwörter (macOS)",
            "1. Öffne die App „Passwörter“ (macOS 15 oder neuer) oder Safari > Einstellungen > Passwörter.\n\
             2. Menü Ablage > „Alle Passwörter exportieren …“ (in der Passwörter-App) und mit Touch ID bestätigen.\n\
             3. Speichere die CSV-Datei und importiere sie in ausgeloggt.\n\
             4. Lösche die Datei danach: Sie enthält Passwörter im Klartext.",
        ),
        (
            "Apple Passwörter (iPhone/iPad)",
            "1. Der Export ist ab iOS 18 nur auf dem Mac möglich. Nutze dort die App „Passwörter“ wie oben beschrieben.\n\
             2. Alternativ: Einstellungen > Passwörter zeigt die Einträge nur an; ein CSV-Export ist auf dem iPhone nicht vorgesehen.\n\
             3. Lösche exportierte Dateien nach dem Import.",
        ),
        (
            "Edge",
            "1. Öffne edge://wallet/passwords (oder Einstellungen > Profile > Kennwörter).\n\
             2. Klicke bei „Gespeicherte Kennwörter“ auf die drei Punkte > „Kennwörter exportieren“ und bestätige.\n\
             3. Speichere die CSV-Datei (Dateiname mit „Edge“ hilft ausgeloggt beim Erkennen) und importiere sie.\n\
             4. Lösche die Datei danach: Sie enthält Passwörter im Klartext.",
        ),
        (
            "Bitwarden",
            "1. Öffne den Web-Tresor (vault.bitwarden.com) > Tools > Tresor exportieren.\n\
             2. Wähle das Format .csv oder .json (nicht „.json (verschlüsselt)“) und gib dein Master-Passwort ein.\n\
             3. Importiere die Datei in ausgeloggt.\n\
             4. Lösche die Datei danach: Sie enthält Passwörter im Klartext.",
        ),
        (
            "1Password",
            "1. Öffne die 1Password-App für Windows/Mac/Linux und wähle das gewünschte Konto.\n\
             2. Datei > Exportieren > Konto-Name wählen, Format CSV, mit Master-Passwort bestätigen.\n\
             3. Importiere die CSV-Datei in ausgeloggt.\n\
             4. Lösche die Datei danach: Sie enthält Passwörter im Klartext.",
        ),
        (
            "KeePassXC / KeePass",
            "1. Öffne die Datenbank in KeePassXC.\n\
             2. Menü Datenbank > Exportieren > „In CSV-Datei …“ (KeePass: Datei > Exportieren > KeePass CSV).\n\
             3. Importiere die CSV-Datei in ausgeloggt.\n\
             4. Lösche die Datei danach: Sie enthält Passwörter im Klartext.",
        ),
        (
            "Proton Pass",
            "1. Öffne Proton Pass (Browser-Erweiterung oder App) > Einstellungen > Exportieren.\n\
             2. Wähle das Format CSV (nicht PGP-verschlüsselt) und bestätige mit deinem Passwort.\n\
             3. Importiere die CSV-Datei in ausgeloggt.\n\
             4. Lösche die Datei danach: Sie enthält Passwörter im Klartext.",
        ),
        (
            "Google Takeout",
            "1. Öffne takeout.google.com und wähle „Auswahl aufheben“, dann die Dienste, die dir wichtig sind (z. B. Aktivitäten/„Mein Aktivitätsprotokoll“).\n\
             2. Wähle als Format JSON (bei Aktivitäten) und als Zustellung „Downloadlink per E-Mail“, ZIP.\n\
             3. Lade das Archiv herunter und ziehe die ZIP-Datei (oder den entpackten Ordner) in ausgeloggt.\n\
             Hinweis: Takeout enthält keine gespeicherten Passwörter und keine vollständige Liste der „Mit Google anmelden“-Verknüpfungen. \
             ausgeloggt wertet nur Hinweise auf angemeldete Drittanbieter aus. Die vollständige Liste findest du unter myaccount.google.com/connections.",
        ),
        (
            "Facebook / Meta",
            "1. Öffne Einstellungen > Konten-Center > Deine Informationen und Berechtigungen > „Deine Informationen herunterladen“.\n\
             2. Wähle „Bestimmte Informationen“ > „Apps und Websites außerhalb von Facebook“ (bzw. „Verbundene Apps und Websites“).\n\
             3. Wähle als Format JSON (nicht HTML), Zeitraum „Gesamte Zeit“, und fordere den Download an.\n\
             4. Lade die ZIP-Datei herunter und ziehe sie (oder den entpackten Ordner) in ausgeloggt.",
        ),
        (
            "Eigene Liste",
            "Eine Textdatei mit einer Domain oder URL pro Zeile (# für Kommentare), eine CSV-Datei mit einer Spalte „url“ oder „domain“ oder eine JSON-Liste ist möglich.",
        ),
    ]
}
