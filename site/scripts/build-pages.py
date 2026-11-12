"""Generates site/index.html (German only), robots.txt and sitemap.xml.
Run: python site/scripts/build-pages.py   (output is committed; the deployed site needs no build step)"""
import os, re, json, html

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SITE = "https://henryschwerdt.github.io/ausgeloggt/"
REPO = "https://github.com/HenrySchwerdt/ausgeloggt"
REL = REPO + "/releases"

# ---------- logo pieces (from build-brand.py output) ----------
wm = open(os.path.join(ROOT, "assets/brand/wordmark.svg"), encoding="utf-8").read()
FRAME = re.search(r'<path d="(M49 24[^"]+)"', wm).group(1)
TEXT = re.findall(r'<path d="([^"]+)" fill', wm)[-1]
WMW = re.search(r'viewBox="0 0 (\d+) 64"', wm).group(1)
LOGO = (f'<svg viewBox="0 0 {WMW} 64" role="img" aria-label="ausgeloggt"><path d="{FRAME}" fill="none" stroke="currentColor" stroke-width="6.5" stroke-linecap="round" stroke-linejoin="round"/>'
        f'<circle class="dot" cx="57" cy="32" r="5.5"/><path d="{TEXT}" fill="currentColor"/></svg>')
MARK = (f'<svg viewBox="0 0 64 64" aria-hidden="true"><path d="{FRAME}" fill="none" stroke="currentColor" stroke-width="6.5" stroke-linecap="round" stroke-linejoin="round"/><circle class="dot" cx="57" cy="32" r="5.5"/></svg>')

def ic(body, extra=""):
    return f'<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"{extra}>{body}</svg>'
I = {
 "search": ic('<circle cx="11" cy="11" r="7"/><path d="M20 20l-3.5-3.5"/>'),
 "card": ic('<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M3 10h18M7 15h4"/>'),
 "shield": ic('<path d="M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z"/><path d="M9 12l2 2 4-4"/>'),
 "mail": ic('<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M3 7l9 6 9-6"/>'),
 "alert": ic('<path d="M12 3l10 18H2z"/><path d="M12 10v4.5M12 17.5h.01"/>'),
 "file": ic('<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5M9 13h6M9 17h6"/>'),
 "clock": ic('<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>'),
 "cpu": ic('<rect x="6" y="6" width="12" height="12" rx="2"/><path d="M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3"/>'),
 "lock": ic('<rect x="4" y="11" width="16" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/>'),
 "key": ic('<circle cx="8" cy="15" r="4"/><path d="M11 12l9-9M16 7l3 3"/>'),
 "inbox": ic('<path d="M3 13l3-8h12l3 8v6H3z"/><path d="M3 13h5l1 3h6l1-3h5"/>'),
 "users": ic('<circle cx="9" cy="8" r="3.5"/><path d="M2.5 20c.6-3.5 3.2-5.5 6.5-5.5s5.9 2 6.5 5.5M16 4.8a3.5 3.5 0 0 1 0 6.4M18 14.8c2 .6 3.2 2.3 3.5 5.2"/>'),
 "image": ic('<rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="9" cy="10" r="1.6"/><path d="M21 16l-5-5-8 9"/>'),
 "download": ic('<path d="M12 4v11M7 11l5 5 5-5M5 20h14"/>'),
 "win": ic('<rect x="3.5" y="3.5" width="8" height="8" rx="1"/><rect x="12.5" y="3.5" width="8" height="8" rx="1"/><rect x="3.5" y="12.5" width="8" height="8" rx="1"/><rect x="12.5" y="12.5" width="8" height="8" rx="1"/>'),
 "mac": ic('<rect x="4" y="5" width="16" height="11" rx="1.5"/><path d="M2 19h20"/>'),
 "linux": ic('<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M7 10l3 2-3 2M12 15h5"/>'),
 "gh": '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 .3a12 12 0 0 0-3.8 23.4c.6.1.8-.3.8-.6v-2c-3.3.7-4-1.6-4-1.6-.6-1.4-1.4-1.8-1.4-1.8-1-.7.1-.7.1-.7 1.2.1 1.8 1.2 1.8 1.2 1.1 1.8 2.8 1.3 3.5 1 .1-.8.4-1.3.8-1.6-2.7-.3-5.5-1.3-5.5-5.9 0-1.3.5-2.4 1.2-3.2-.1-.3-.5-1.5.1-3.2 0 0 1-.3 3.3 1.2a11.5 11.5 0 0 1 6 0c2.3-1.5 3.3-1.2 3.3-1.2.7 1.7.2 2.9.1 3.2.8.8 1.2 1.9 1.2 3.2 0 4.6-2.8 5.6-5.5 5.9.4.4.8 1.1.8 2.2v3.3c0 .3.2.7.8.6A12 12 0 0 0 12 .3"/></svg>',
 "arrow": '<svg viewBox="0 0 56 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2 8h50M45 3l7 5-7 5"/></svg>',
 "blocked": '<svg viewBox="0 0 56 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"><path d="M2 8h18M36 8h18" stroke-dasharray="3 4"/><path d="M24 4l8 8M32 4l-8 8"/></svg>',
}

PEOPLE = [("Spotify", "spotify.com", 0), ("Zalando", "zalando.de", 0), ("Netflix", "netflix.com", 2), ("Adobe", "adobe.com", 2), ("Dropbox", "dropbox.com", 0),
          ("Lieferando", "lieferando.de", 1), ("TikTok", "tiktok.com", 0), ("Booking.com", "booking.com", 1), ("Discord", "discord.com", 0), ("Airbnb", "airbnb.de", 0),
          ("Notion", "notion.so", 0), ("Reddit", "reddit.com", 0), ("IKEA", "ikea.com", 1), ("Uber", "uber.com", 0), ("Canva", "canva.com", 2), ("Steam", "steampowered.com", 0)]

# ---------- copy ----------
DE = dict(
 lang="de", root="", url=SITE,
 title="ausgeloggt – Finde, prüfe und lösche deine Online-Accounts. Lokal & kostenlos.",
 desc="ausgeloggt findet alle deine Online-Accounts, zeigt wo Zahlungsdaten und Abos gespeichert sind, prüft die Sicherheit und hilft beim Löschen, Abmelden und Kündigen. Open Source, lokal, ohne Konto, MIT.",
 og_locale="de_DE",
 skip="Zum Inhalt", nav=[("#funktionen", "Funktionen"), ("#datenschutz", "Datenschutz"), ("#ablauf", "So geht’s"), ("#briefe", "Briefe"), ("#faq", "FAQ")],
 dl="Download", dl_soon="Bald verfügbar", github="GitHub",
 pill="Open Source · MIT · lokal",
 h1='Log dich aus.<br><span class="dim">Überall.</span>',
 lead="ausgeloggt findet jeden Online-Account von dir, zeigt, wo deine Zahlungsdaten liegen, was dich Geld kostet und wo du offen für Übernahmen bist – und räumt auf. Löschen, abmelden, kündigen. Alles auf deinem Rechner.",
 cta1="Für Windows, macOS &amp; Linux", cta2="Quellcode ansehen",
 meta=["Kein Konto", "Keine Cloud", "Keine Telemetrie"],
 led_label="Noch angemeldet", led_state="Aufräumen läuft", led_done="Ausgeloggt.", led_foot_l="Beispieldaten", led_foot_r="Demo",
 led_tags=["Löschung gesendet", "Abgemeldet", "Gekündigt"],
 hero_alt="Die Übersicht von ausgeloggt: 73 gefundene Accounts, Fristen, zuletzt gefundene Dienste.",
 strip=[("0 Bytes", "verlassen deinen Rechner"), ("MIT", "Open Source, lesbar, forkbar"), ("0 €", "kostenlos, ohne Konto"), ("3", "Windows, macOS, Linux")],
 show_k="Die App", show_h="Ein ruhiger Ort für ein unruhiges Netz.", show_l="Tabellen statt Dashboards, Haarlinien statt Karten. ausgeloggt zeigt dir, was du hast, und lässt dich in zwei Klicks handeln.",
 shots=[("dashboard", "Übersicht", "Dein digitaler Fußabdruck.", "Gefundene Accounts, offene Fristen und ein Fortschrittsbalken fürs Aufräumen. Alles, was Aufmerksamkeit braucht, steht oben.", "Übersicht mit Kennzahlen, Fristen und zuletzt gefundenen Accounts"),
        ("accounts", "Accounts", "Jeder Account, jede Quelle.", "Aus Postfach, Passwort-Manager, SSO und Exporten zusammengeführt. Du siehst, woher ein Fund stammt und wie sicher er ist.", "Account-Liste mit Kennung, Quelle, Status und Sicherheitswert"),
        ("detail", "Details", "Belege statt Bauchgefühl.", "Zu jedem Dienst: Kennungen, Hinweise aus deinen Mails, der einfachste Weg zur Löschung und deine Notizen.", "Detailansicht eines Accounts mit Kennungen, Hinweisen und Lösch-Aktionen"),
        ("letter", "Briefe", "Der Antrag ist schon geschrieben.", "DSGVO-Löschantrag, Auskunft oder Kündigung: fertig formuliert, mit deinen Kennungen, als Mail oder druckfertiger Brief.", "DSGVO-Löschantrag als Vorschau"),
        ("palette", "Tastatur", "Strg K. Und los.", "Suchen, filtern, handeln, ohne die Hände von der Tastatur zu nehmen.", "Befehlspalette über der Account-Liste")],
 priv_k="Datenschutz", priv_h="Was verlässt deinen Rechner? <span style=\"color:var(--text-3)\">Nichts.</span>",
 priv_l="Eine App, die dein ganzes digitales Leben kennt, darf nichts davon weitergeben. Deshalb gibt es bei ausgeloggt keinen Server, kein Konto und keine Telemetrie. Du musst uns nicht vertrauen, du kannst es nachlesen.",
 d_src="Deine Quellen", d_src_items=[("mail", "Postfach", "nur Header, per IMAP"), ("key", "Passwort-Export", "Passwörter werden nie gespeichert"), ("users", "SSO &amp; Datenexporte", "Google, Apple, Facebook …"), ("image", "Screenshots", "gelesen von lokaler KI")],
 d_box="Dein Rechner", d_vault="Verschlüsselter Tresor", d_vault_t="Eine Datei auf deiner Platte. Ohne dein Passwort ist sie Rauschen.",
 d_kv=["Argon2id", "XChaCha20-Poly1305", "kein Konto"], d_ai="Lokale KI (optional)", d_ai_t="Ollama, auf deinem Gerät. Nie ein Cloud-Dienst.",
 d_out="Cloud, Anbieter, wir", d_out_items=["Deine Account-Liste", "Deine Passwörter", "Inhalte deiner Mails", "Nutzungsdaten"],
 d_zero="0", d_zero_u="Bytes",
 conns_h="Die einzigen Verbindungen, die ausgeloggt selbst aufbaut",
 conns=[("Dein Mail-Server", "IMAP, nur Header. Direkt von deinem Rechner, ohne Umweg."), ("Have I Been Pwned", "k-Anonymität: Es gehen nur die ersten 5 Zeichen eines SHA-1-Hashs raus, nie ein Passwort."),
        ("Abmelde-Links", "Nur, wenn du auf „Abmelden“ klickst: ein One-Click-POST an den Absender (RFC 8058)."), ("Katalog-Update", "Lösch-Katalog aktualisieren. Lässt sich in den Einstellungen abschalten.")],
 how_k="So geht’s", how_h="Drei Schritte aus dem Netz.", how_l=None,
 how=[("Quellen verbinden", "Gib ausgeloggt, was du ohnehin hast. Es braucht nie alles auf einmal.", ["Postfach (IMAP)", "Passwort-Export", "SSO", "Datenexport", "Screenshot"]),
      ("Prüfen", "Alle Funde an einem Ort: Wo hast du ein Konto, wo liegen Karte und PayPal, was kostet dich Geld, wo ist dein Passwort geleakt?", ["Accounts", "Zahlungen", "Abos", "Sicherheit"]),
      ("Löschen, abmelden, kündigen", "Ein Klick für Newsletter. Fertige Briefe für alles andere. Und jede Frist im Blick, bis der Account wirklich weg ist.", ["One-Click", "DSGVO-Brief", "Fristen"])],
 feat_k="Funktionen", feat_h="Alles, was zwischen dir und „ausgeloggt“ steht.", feat_l="Eine App statt zehn Tabs, Tabellen und Mailentwürfe.",
 feats=[("users", "Alle Accounts finden", "Aus Postfach (nur Header), Passwort-Manager-Exporten, „Mit Google, Apple, … anmelden“, Datenexporten und Screenshots, gelesen von lokaler KI.", "Chrome · Firefox · Safari · Bitwarden · 1Password · KeePass"),
        ("card", "Zahlungsdaten &amp; Abos", "Wo ist deine Karte, dein PayPal, dein IBAN hinterlegt? Welche Abos laufen und was kosten sie? Inklusive Apple-, Google-Play- und PayPal-Belegen.", "Beträge pro Monat und Jahr"),
        ("shield", "Sicherheitscheck", "Geleakte und mehrfach verwendete Passwörter (HIBP, k-Anonymität), bekannte Datenlecks, 2FA-Unterstützung und Übernahme-Risiken.", "Dein Passwort verlässt nie den Rechner"),
        ("mail", "Newsletter in einem Klick abmelden", "Wo der Absender RFC 8058 unterstützt, genügt ein Klick. Der Rest läuft über mailto oder den Link im Browser.", "RFC 8058 One-Click"),
        ("alert", "Mails trotz Abmeldung", "ausgeloggt merkt, wenn nach der Abmeldung weiter Mails kommen, und legt die Beweise gleich dazu.", "Die Basis für die Unterlassungsaufforderung"),
        ("file", "Briefe nach DIN 5008", "Unterlassung mit Beleg, DSGVO-Löschung und -Auskunft, Kündigung, Selbstauskunft bei Auskunfteien, Beschwerde bei der Aufsichtsbehörde. Als druckfertiges PDF.", "Druckfertig, Falzmarken inklusive"),
        ("clock", "Jede Frist im Blick", "Ein Monat für die Antwort, eine Kündigungsfrist, eine Erinnerung. ausgeloggt zählt mit und meldet sich, wenn nachgefasst werden muss.", "Mit Überfälligkeits-Hinweis"),
        ("cpu", "Lokale KI, wenn du willst", "Ollama ordnet Newsletter ein und liest Screenshots. Ohne KI funktioniert alles weiter, nur etwas manueller.", "Optional · lokal · Ollama"),
        ("lock", "Verschlüsselter Tresor", "Alles liegt in einer Datei, geschützt mit Argon2id und XChaCha20-Poly1305. Kein Konto, kein Sync, keine Cloud.", "Passwörter &amp; Kartendaten werden nie gespeichert")],
 let_k="Briefe", let_h="Ein Brief, den du nur noch unterschreibst.", let_l="Wenn eine Mail trotz Abmeldung weiter kommt, schreibt ausgeloggt die Aufforderung dazu: mit Daten und Beweisen, sauber nach DIN 5008, als PDF zum Ausdrucken.",
 kinds=[("Unterlassung mit Beleg", "wenn Mails trotz Abmeldung kommen"), ("Löschung nach Art. 17 DSGVO", "Account und Daten"), ("Auskunft nach Art. 15 DSGVO", "was gespeichert ist"), ("Kündigung", "Abo und Vertrag"), ("Selbstauskunft", "bei Auskunfteien"), ("Beschwerde", "bei der Aufsichtsbehörde")],
 let_note="Die Briefe sind Entwürfe nach deutschem Recht und keine Rechtsberatung. Du prüfst und unterschreibst sie selbst.",
 paper=dict(ret="Lena Hartmann · Beispielweg 12 · 10115 Berlin", addr="Beispiel-Versand GmbH<br>Datenschutz<br>Musterstraße 1<br>12345 Musterstadt",
   info=[("Datum", "12.11.2026"), ("Zeichen", "AG-0173"), ("Frist", "26.11.2026")],
   subj="Aufforderung zur Unterlassung: Werbe-E-Mails nach Abmeldung",
   p1="Sehr geehrte Damen und Herren,<br>ich habe mich am 02.10.2026 von Ihrem Newsletter abgemeldet. Die Abmeldung wurde bestätigt. Trotzdem erhalte ich weiter Werbe-E-Mails:",
   th=("Datum", "Absender", "Betreff"), tr=[("05.10.2026", "news@beispiel-versand.de", "Nur heute: 20 % auf alles"), ("19.10.2026", "news@beispiel-versand.de", "Dein Warenkorb wartet"), ("03.11.2026", "angebote@beispiel-versand.de", "Black Week startet")],
   p2="Ich fordere Sie auf, den Versand bis zum 26.11.2026 einzustellen und dies schriftlich zu bestätigen. Zudem bitte ich um Löschung meiner Daten nach Art. 17 DSGVO.",
   p3="Mit freundlichen Grüßen<br><br>Lena Hartmann",
   foot=("Erstellt mit ausgeloggt", "Seite 1 von 1")),
 cmp_k="Vergleich", cmp_h="Warum soll ausgerechnet ein Dienst deine Daten haben, der sie löschen will?",
 cmp_l="Bezahlte Löschdienste brauchen dein Postfach, deine Identität und dein Vertrauen. ausgeloggt braucht nur deinen Rechner.",
 cmp_cols=("", "ausgeloggt", "Bezahlte Löschdienste"),
 cmp=[("Preis", "Kostenlos, für immer", "Abo, oft jährlich", 1, 0), ("Wohin gehen deine Daten?", "Nirgendwohin. Alles lokal.", "Auf die Server des Anbieters", 1, 0), ("Konto nötig?", "Nein", "Ja, samt Identitätsnachweis", 1, 0),
      ("Zugriff aufs Postfach", "Nur Header, von dir aus", "Meist Vollzugriff über deren Server", 1, 0), ("Quellcode", "Offen, MIT-Lizenz", "Geschlossen", 1, 0), ("Beweise &amp; Briefe", "Bleiben bei dir", "Beim Anbieter", 1, 0), ("Telemetrie", "Keine", "Üblich", 1, 0)],
 dl_k="Download", dl_h="Kostenlos. Ohne Konto. Ohne Haken.", dl_l="Die ersten Versionen sind in Arbeit. Sobald sie fertig sind, findest du sie hier und auf GitHub.",
 os=[("win", "Windows", "Windows 10 und 11, 64 Bit"), ("mac", "macOS", "Apple Silicon und Intel"), ("linux", "Linux", "AppImage und .deb")],
 dl_btn="Zu den Releases",
 faq_k="FAQ", faq_h="Gute Fragen.",
 faq=[("Ist das legal?", "<p>Ja. Nach der DSGVO hast du ein Recht auf Auskunft (Art. 15), Löschung (Art. 17) und Widerspruch (Art. 21). Newsletter dürfen per One-Click-Link abbestellt werden (RFC 8058). ausgeloggt nimmt dir nur das Schreiben und Klicken ab; absenden tust du selbst.</p><p>Die Briefe sind Entwürfe, keine Rechtsberatung.</p>"),
      ("Was passiert mit meinen Passwörtern?", "<p>Passwort-Exporte werden nur auf deinem Rechner gelesen, um zu erkennen, wo du Accounts hast und ob Passwörter schwach, geleakt oder mehrfach verwendet sind. Passwörter, Notizen, TOTP-Secrets und Kartendaten werden <b>nie</b> gespeichert.</p><p>Für den Leak-Check geht nur ein 5-stelliger Hash-Präfix an Have I Been Pwned (k-Anonymität). Das Passwort selbst verlässt den Rechner nie.</p>"),
      ("Welche Mail-Anbieter funktionieren?", "<p>Jeder Anbieter mit IMAP, zum Beispiel Gmail, GMX, web.de, iCloud, Yahoo, Fastmail oder dein eigener Server. Gelesen werden nur die Header (Absender, Betreff, Datum), nicht die Inhalte. Ausnahme sind Sicherheitsmails zu „Mit Google/Apple anmelden“.</p>"),
      ("Und Outlook?", "<p>Outlook.com und Microsoft 365 lassen Passwort-Zugriff per IMAP nicht mehr zu und verlangen OAuth. Diese Anmeldung ist noch in Arbeit. Bis dahin kannst du Outlook-Konten über Passwort-Exporte und Datenexporte einbinden.</p>"),
      ("Kostet das Geld?", "<p>Nein. ausgeloggt ist Open Source unter der MIT-Lizenz, ohne Konto, ohne Abo, ohne Werbung. Der Quellcode liegt auf <a href=\"" + REPO + "\">GitHub</a>.</p>"),
      ("Brauche ich die KI?", "<p>Nein. Die KI ist optional und läuft lokal über Ollama. Sie hilft, Newsletter einzuordnen und Screenshots zu lesen. Alles andere funktioniert ohne.</p>"),
      ("Wann gibt es den Download?", "<p>Bald. Die Releases erscheinen auf <a href=\"" + REL + "\">GitHub Releases</a>, für Windows, macOS und Linux.</p>")],
 close_h="Dein Netz.<br>Deine Regeln.", close_l="Log dich aus, wo du nicht mehr sein willst.",
 f_product="Produkt", f_project="Projekt", f_links_p=[("#funktionen", "Funktionen"), ("#datenschutz", "Datenschutz"), ("#briefe", "Briefe"), ("#faq", "FAQ")],
 f_links_r=[(REPO, "GitHub"), (REL, "Releases"), (REPO + "/issues", "Fehler melden"), (REPO + "/blob/main/LICENSE", "MIT-Lizenz")], f_lang="Sprache",
 f_tag="Finde, prüfe und lösche deine Online-Accounts. Lokal, offen, kostenlos.",
 f_priv="Diese Seite lädt keine Schriften, Skripte oder Bilder von Dritten, setzt keine Cookies und nutzt keine Analyse. Sie tut, was die App verspricht.",
 f_copy="© 2026 ausgeloggt · MIT-Lizenz",
)

ID = {"de": dict(feat="funktionen", priv="datenschutz", how="ablauf", let="briefe")}

def pic(root, name, alt, lazy=True, cls=""):
    load = 'loading="lazy" decoding="async"' if lazy else 'fetchpriority="high" decoding="async"'
    return (f'<picture{cls}><source srcset="{root}assets/shots/{name}.webp" type="image/webp">'
            f'<img src="{root}assets/shots/{name}.png" width="1440" height="900" alt="{html.escape(alt)}" {load}></picture>')

def page(t):
    r, L, ids = t["root"], t["lang"], ID[t["lang"]]
    e = html.escape
    nav = "".join(f'<a href="{h}">{l}</a>' for h, l in t["nav"])
    ledger_data = json.dumps([{"n": n, "d": d, "t": k} for n, d, k in PEOPLE], ensure_ascii=False)
    # hero
    hero_pic = (f'<picture class="theme-dark"><source srcset="{r}assets/shots/dashboard.webp" type="image/webp"><img src="{r}assets/shots/dashboard.png" width="1440" height="900" alt="{e(t["hero_alt"])}" fetchpriority="high"></picture>'
                f'<picture class="theme-light"><source srcset="{r}assets/shots/dashboard-light.webp" type="image/webp"><img src="{r}assets/shots/dashboard-light.png" width="1440" height="900" alt="{e(t["hero_alt"])}" loading="lazy"></picture>')
    strip = "".join(f"<li><b>{a}</b>{b}</li>" for a, b in t["strip"])
    steps = "".join(
        f'<article class="step" data-shot="{k}"><p class="n">{i+1:02d} · {lab}</p><h3>{h}</h3><p>{p}</p><div class="win">{pic(r, k, alt)}</div></article>'
        for i, (k, lab, h, p, alt) in enumerate(t["shots"]))
    stage = "".join(f'<picture data-shot="{k}"><source srcset="{r}assets/shots/{k}.webp" type="image/webp"><img src="{r}assets/shots/{k}.png" width="1440" height="900" alt="{e(alt)}" {"" if k == "dashboard" else "loading=lazy"}></picture>' for k, lab, h, p, alt in t["shots"])
    src = "".join(f'<div class="chip">{I[i]}<span>{n}<small>{s}</small></span></div>' for i, n, s in t["d_src_items"])
    outs = "".join(f"<li>{x}</li>" for x in t["d_out_items"])
    kv = "".join(f"<code>{x}</code>" for x in t["d_kv"])
    conns = "".join(f"<li><b>{a}</b>{b}</li>" for a, b in t["conns"])
    how = "".join(f'<li class="rv"><div class="bignum">{i+1}</div><h3>{h}</h3><p>{p}</p><div class="srcs">{"".join(f"<span class=tagx>{x}</span>" for x in tg)}</div></li>' for i, (h, p, tg) in enumerate(t["how"]))
    feats = "".join(f'<article class="feat">{I[i]}<h3>{h}</h3><p>{p}</p><p class="meta">{m}</p></article>' for i, h, p, m in t["feats"])
    kinds = "".join(f"<li><b>{a}</b><span>{b}</span></li>" for a, b in t["kinds"])
    P = t["paper"]
    info = "".join(f"<div><span>{a}</span>{b}</div>" for a, b in P["info"])
    rows = "".join(f"<tr><td>{a}</td><td>{b}</td><td>{c}</td></tr>" for a, b, c in P["tr"])
    paper = (f'<div class="paper-wrap"><div class="paper" role="img" aria-label="{e(P["subj"])}">'
             '<i class="fm" style="top:calc(105*var(--mm))"></i><i class="fm" style="top:calc(148.5*var(--mm));width:calc(7*var(--mm))"></i><i class="fm" style="top:calc(210*var(--mm))"></i>'
             f'<div class="stamp"><i></i>ausgeloggt</div><p class="ret">{P["ret"]}</p><p class="addr">{P["addr"]}</p><div class="info">{info}</div>'
             f'<p class="subj">{P["subj"]}</p><div class="body"><p>{P["p1"]}</p><table><thead><tr><th>{P["th"][0]}</th><th>{P["th"][1]}</th><th>{P["th"][2]}</th></tr></thead><tbody>{rows}</tbody></table><p>{P["p2"]}</p><p>{P["p3"]}</p></div>'
             f'<div class="foot"><span>{P["foot"][0]}</span><span>{P["foot"][1]}</span></div></div></div>')
    cmp = "".join(f'<div class="cmp-row"><div>{a}</div><div class="yes">{b}</div><div class="no">{c}</div></div>' for a, b, c, _, _ in t["cmp"])
    c0, c1, c2 = t["cmp_cols"]
    cmp_head = f'<div class="cmp-row cmp-head"><div>{c0}</div><div>{c1}</div><div>{c2}</div></div>'
    os_ = "".join(f'<div class="os">{I[i]}<b>{n}</b><span>{s}</span><span class="soon">{t["dl_soon"]}</span></div>' for i, n, s in t["os"])
    faq = "".join(f'<details><summary>{q}</summary><div class="a">{a}</div></details>' for q, a in t["faq"])
    flp = "".join(f'<li><a href="{h}">{l}</a></li>' for h, l in t["f_links_p"])
    flr = "".join(f'<li><a href="{h}">{l}</a></li>' for h, l in t["f_links_r"])
    ld = json.dumps({"@context": "https://schema.org", "@type": "SoftwareApplication", "name": "ausgeloggt", "operatingSystem": "Windows, macOS, Linux",
        "applicationCategory": "SecurityApplication", "description": t["desc"], "url": t["url"], "inLanguage": L, "image": SITE + "assets/brand/og-image.png",
        "offers": {"@type": "Offer", "price": "0", "priceCurrency": "EUR"}, "license": "https://opensource.org/licenses/MIT", "isAccessibleForFree": True,
        "codeRepository": REPO, "sameAs": [REPO]}, ensure_ascii=False)
    return f'''<!doctype html>
<html lang="{L}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{e(t["title"])}</title>
<meta name="description" content="{e(t["desc"])}">
<meta name="color-scheme" content="dark light">
<meta name="theme-color" content="#09090b" media="(prefers-color-scheme: dark)">
<meta name="theme-color" content="#ffffff" media="(prefers-color-scheme: light)">
<link rel="canonical" href="{t["url"]}">
<meta property="og:type" content="website">
<meta property="og:site_name" content="ausgeloggt">
<meta property="og:title" content="{e(t["title"])}">
<meta property="og:description" content="{e(t["desc"])}">
<meta property="og:url" content="{t["url"]}">
<meta property="og:locale" content="{t["og_locale"]}">
<meta property="og:image" content="{SITE}assets/brand/og-image.png">
<meta property="og:image:width" content="1200"><meta property="og:image:height" content="630">
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="{e(t["title"])}">
<meta name="twitter:description" content="{e(t["desc"])}">
<meta name="twitter:image" content="{SITE}assets/brand/og-image.png">
<link rel="icon" href="{r}assets/brand/favicon.svg" type="image/svg+xml">
<link rel="icon" href="{r}assets/brand/favicon.ico" sizes="48x48">
<link rel="apple-touch-icon" href="{r}assets/brand/apple-touch-icon.png">
<link rel="preload" href="{r}assets/fonts/inter-latin-wght-normal.woff2" as="font" type="font/woff2" crossorigin>
<link rel="stylesheet" href="{r}css/site.css">
<script src="{r}js/theme.js"></script>
<script type="application/ld+json">{ld}</script>
</head>
<body>
<a class="skip" href="#main">{t["skip"]}</a>
<header class="hdr"><div class="wrap">
<a class="brand" href="{r or "./"}" aria-label="ausgeloggt">{LOGO}</a>
<nav class="nav" aria-label="Navigation">{nav}</nav>
<div class="hdr-end"><a class="btn btn-secondary btn-sm gh" href="{REPO}" rel="noopener">{I["gh"]}{t["github"]}</a><a class="btn btn-primary btn-sm" href="#download">{t["dl"]}</a></div>
</div></header>
<main id="main">

<section class="hero"><div class="wrap">
<div class="hero-grid">
<div>
<span class="pill">{t["pill"]}</span>
<h1>{t["h1"]}</h1>
<p class="lead">{t["lead"]}</p>
<div class="cta"><a class="btn btn-primary btn-lg" href="#download">{I["download"]}{t["cta1"]}</a><a class="btn btn-secondary btn-lg" href="{REPO}" rel="noopener">{I["gh"]}{t["cta2"]}</a></div>
<p class="hero-meta">{"".join(f"<span>{m}</span>" for m in t["meta"])}</p>
</div>
<div class="ledger" id="ledger" data-labels='{json.dumps(t["led_tags"], ensure_ascii=False)}' aria-hidden="true">
<div class="ledger-top"><div><small>{t["led_label"]}</small><div class="count" id="count">73</div></div><div class="state"><i></i><span id="state" data-done="{t["led_done"]}">{t["led_state"]}</span></div></div>
<ul class="rows"></ul>
<div class="ledger-foot"><span>{t["led_foot_l"]}</span><span>{t["led_foot_r"]}</span></div>
</div>
<script type="application/json" id="ledger-data">{ledger_data}</script>
</div>
<div class="hero-shot rv"><div class="win">{hero_pic}</div></div>
</div></section>

<section class="strip"><div class="wrap"><ul>{strip}</ul></div></section>

<section class="sec" id="app"><div class="wrap">
<div class="sec-head rv"><span class="eyebrow">{t["show_k"]}</span><h2 class="h2">{t["show_h"]}</h2><p class="lead">{t["show_l"]}</p></div>
<div class="show"><div class="show-steps">{steps}</div><div class="show-stage" aria-hidden="true"><div class="win stage-win">{stage}</div></div></div>
</div></section>

<section class="sec priv" id="{ids["priv"]}"><div class="wrap">
<div class="sec-head rv"><span class="eyebrow">{t["priv_k"]}</span><h2 class="h2">{t["priv_h"]}</h2><p class="lead">{t["priv_l"]}</p></div>
<div class="diag rv">
<div class="dcol"><h3>{t["d_src"]}</h3>{src}</div>
<div class="arrow">{I["arrow"]}</div>
<div class="box"><h3>{MARK.replace("<svg ", '<svg style="height:16px;width:16px" ')}{t["d_box"]}</h3>
<div class="vault"><b>{I["lock"]}{t["d_vault"]}</b><p>{t["d_vault_t"]}</p><div class="kv">{kv}</div></div>
<div class="vault"><b>{I["cpu"]}{t["d_ai"]}</b><p>{t["d_ai_t"]}</p></div></div>
<div class="arrow blocked">{I["blocked"]}</div>
<div class="dcol"><h3>{t["d_out"]}</h3><div class="zero"><div class="big">{t["d_zero"]}<small>{t["d_zero_u"]}</small></div><ul>{outs}</ul></div></div>
</div>
<div class="conns rv"><h3>{t["conns_h"]}</h3><ul>{conns}</ul></div>
</div></section>

<section class="sec" id="{ids["how"]}"><div class="wrap">
<div class="sec-head rv"><span class="eyebrow">{t["how_k"]}</span><h2 class="h2">{t["how_h"]}</h2></div>
<ol class="steps3">{how}</ol>
</div></section>

<section class="sec hairline" id="{ids["feat"]}" style="border-top:1px solid var(--border)"><div class="wrap">
<div class="sec-head rv"><span class="eyebrow">{t["feat_k"]}</span><h2 class="h2">{t["feat_h"]}</h2><p class="lead">{t["feat_l"]}</p></div>
<div class="grid rv">{feats}</div>
</div></section>

<section class="sec letter-sec priv" id="{ids["let"]}"><div class="wrap"><div class="lgrid">
<div class="rv"><span class="eyebrow">{t["let_k"]}</span><h2 class="h2">{t["let_h"]}</h2><p class="lead">{t["let_l"]}</p><ul class="kinds">{kinds}</ul><p class="note">{t["let_note"]}</p></div>
<div class="paper-stage rv">{paper}</div>
</div></div></section>

<section class="sec"><div class="wrap">
<div class="sec-head rv"><span class="eyebrow">{t["cmp_k"]}</span><h2 class="h2">{t["cmp_h"]}</h2><p class="lead">{t["cmp_l"]}</p></div>
<div class="cmp rv">{cmp_head}{cmp}</div>
</div></section>

<section class="sec" id="download" style="border-top:1px solid var(--border)"><div class="wrap">
<div class="sec-head center rv"><span class="eyebrow">{t["dl_k"]}</span><h2 class="h2">{t["dl_h"]}</h2><p class="lead">{t["dl_l"]}</p></div>
<div class="dl rv">{os_}</div>
<p style="text-align:center;margin-top:28px"><a class="btn btn-secondary" href="{REL}" rel="noopener">{I["gh"]}{t["dl_btn"]}</a></p>
</div></section>

<section class="sec" id="faq" style="border-top:1px solid var(--border)"><div class="wrap">
<div class="sec-head center rv"><span class="eyebrow">{t["faq_k"]}</span><h2 class="h2">{t["faq_h"]}</h2></div>
<div class="faq rv">{faq}</div>
</div></section>

<section class="sec closing" style="border-top:1px solid var(--border)"><div class="wrap rv">
<div class="mk">{MARK}</div>
<h2>{t["close_h"]}</h2><p class="lead" style="margin-inline:auto">{t["close_l"]}</p>
<div class="cta"><a class="btn btn-primary btn-lg" href="#download">{I["download"]}{t["dl"]}</a><a class="btn btn-secondary btn-lg" href="{REPO}" rel="noopener">{I["gh"]}{t["github"]}</a></div>
</div></section>

</main>
<footer class="ftr"><div class="wrap">
<div class="ftr-grid">
<div><a class="brand" href="{r or "./"}" aria-label="ausgeloggt">{LOGO}</a><p style="margin-top:14px;max-width:34ch">{t["f_tag"]}</p></div>
<div><h4>{t["f_product"]}</h4><ul>{flp}</ul></div>
<div><h4>{t["f_project"]}</h4><ul>{flr}</ul></div>
</div>
<div class="fine"><p class="priv-note">{t["f_priv"]}</p><p>{t["f_copy"]}</p></div>
</div></footer>
<script type="module" src="{r}js/main.js"></script>
</body>
</html>
'''

for t, path in ((DE, "index.html"),):
    open(os.path.join(ROOT, path), "w", encoding="utf-8").write(page(t))

open(os.path.join(ROOT, "robots.txt"), "w").write(f"User-agent: *\nAllow: /\n\nSitemap: {SITE}sitemap.xml\n")
open(os.path.join(ROOT, "sitemap.xml"), "w").write(f'''<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>{SITE}</loc><changefreq>monthly</changefreq><priority>1.0</priority></url>
</urlset>
''')
open(os.path.join(ROOT, ".nojekyll"), "w").write("")
print("built")
