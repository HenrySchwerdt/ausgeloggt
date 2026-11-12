"""Generates site/index.html (German only), robots.txt and sitemap.xml.
Run: python site/scripts/build-pages.py   (output is committed; the deployed site needs no build step)
Brand glyphs: simple-icons (CC0), paths copied to scripts/glyphs.json."""
import os, re, json, html

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SITE = "https://henryschwerdt.github.io/ausgeloggt/"
REPO = "https://github.com/HenrySchwerdt/ausgeloggt"

GL = json.load(open(os.path.join(ROOT, "scripts/glyphs.json"), encoding="utf-8"))
SPRITE = '<svg width="0" height="0" style="position:absolute" aria-hidden="true">' + "".join(
    f'<symbol id="g-{k}" viewBox="0 0 24 24"><path d="{v}"/></symbol>' for k, v in GL.items()) + "</svg>"
def g(k): return f'<svg class="glyph" aria-hidden="true"><use href="#g-{k}"/></svg>'

wm = open(os.path.join(ROOT, "assets/brand/wordmark.svg"), encoding="utf-8").read()
FRAME = re.search(r'<path d="(M49 24[^"]+)"', wm).group(1)
TEXT = re.findall(r'<path d="([^"]+)" fill', wm)[-1]
WMW = re.search(r'viewBox="0 0 (\d+) 64"', wm).group(1)
LOGO = (f'<svg viewBox="0 0 {WMW} 64" role="img" aria-label="ausgeloggt"><path d="{FRAME}" fill="none" stroke="currentColor" stroke-width="6.5" stroke-linecap="round" stroke-linejoin="round"/>'
        f'<circle class="dot" cx="57" cy="32" r="5.5"/><path d="{TEXT}" fill="currentColor"/></svg>')
ARROW = '<svg class="arrow" viewBox="0 0 28 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2 8h22M18 3l6 5-6 5"/></svg>'
GH = '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 .3a12 12 0 0 0-3.8 23.4c.6.1.8-.3.8-.6v-2c-3.3.7-4-1.6-4-1.6-.6-1.4-1.4-1.8-1.4-1.8-1-.7.1-.7.1-.7 1.2.1 1.8 1.2 1.8 1.2 1.1 1.8 2.8 1.3 3.5 1 .1-.8.4-1.3.8-1.6-2.7-.3-5.5-1.3-5.5-5.9 0-1.3.5-2.4 1.2-3.2-.1-.3-.5-1.5.1-3.2 0 0 1-.3 3.3 1.2a11.5 11.5 0 0 1 6 0c2.3-1.5 3.3-1.2 3.3-1.2.7 1.7.2 2.9.1 3.2.8.8 1.2 1.9 1.2 3.2 0 4.6-2.8 5.6-5.5 5.9.4.4.8 1.1.8 2.2v3.3c0 .3.2.7.8.6A12 12 0 0 0 12 .3"/></svg>'

TITLE = "ausgeloggt: Alte Accounts finden und löschen"
DESC = "Eine App für Windows, macOS und Linux. Findet Accounts und Abos in deinem Postfach und schreibt die Löschanträge. Alles bleibt auf deinem Rechner. Open Source."

def pic(name, alt, cls="", lazy=True):
    load = 'loading="lazy" decoding="async"' if lazy else 'fetchpriority="high" decoding="async"'
    return (f'<picture{cls}><source srcset="assets/shots/{name}.webp" type="image/webp">'
            f'<img src="assets/shots/{name}.png" width="1440" height="900" alt="{html.escape(alt)}" {load}></picture>')

TOASTS = [{"g": "spotify", "t": "Löschantrag für Spotify erstellt"}, {"g": "adobe", "t": "Adobe: Frist seit 3 Tagen überfällig"},
          {"g": "zalando", "t": "Löschlink für Zalando gefunden"}]

PAGE = r'''<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>__TITLE__</title>
<meta name="description" content="__DESC__">
<meta name="color-scheme" content="dark light">
<meta name="theme-color" content="#09090b" media="(prefers-color-scheme: dark)">
<meta name="theme-color" content="#ffffff" media="(prefers-color-scheme: light)">
<link rel="canonical" href="__SITE__">
<meta property="og:type" content="website">
<meta property="og:site_name" content="ausgeloggt">
<meta property="og:title" content="__TITLE__">
<meta property="og:description" content="__DESC__">
<meta property="og:url" content="__SITE__">
<meta property="og:locale" content="de_DE">
<meta property="og:image" content="__SITE__assets/brand/og-image.png">
<meta property="og:image:width" content="1200"><meta property="og:image:height" content="630">
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="__TITLE__">
<meta name="twitter:description" content="__DESC__">
<meta name="twitter:image" content="__SITE__assets/brand/og-image.png">
<link rel="icon" href="assets/brand/favicon.svg" type="image/svg+xml">
<link rel="icon" href="assets/brand/favicon.ico" sizes="48x48">
<link rel="apple-touch-icon" href="assets/brand/apple-touch-icon.png">
<link rel="preload" href="assets/fonts/inter-latin-wght-normal.woff2" as="font" type="font/woff2" crossorigin>
<link rel="stylesheet" href="css/site.css">
<script src="js/theme.js"></script>
<script type="application/ld+json">__LD__</script>
</head>
<body>
__SPRITE__
<a class="skip" href="#main">Zum Inhalt</a>
<header class="hdr"><div class="wrap">
  <a class="logo" href="./" aria-label="ausgeloggt">__LOGO__</a>
  <nav class="nav" aria-label="Hauptnavigation"><a href="#abos">Abos</a><a href="#mail">Mail</a><a href="#passwoerter">Passwörter</a><a href="#briefe">Briefe</a><a href="#technik">Technik</a><a href="#faq">FAQ</a></nav>
  <div class="right"><a class="btn sm" href="__REPO__">__GH__GitHub</a></div>
</div></header>

<main id="main">
<section class="hero"><div class="wrap">
  <h1>Finde deine alten Accounts und lösche sie.</h1>
  <p class="lead">Eine App für Windows, macOS und Linux. Sie findet Accounts und Abos in deinem Postfach und schreibt die Löschanträge. Alles bleibt auf deinem Rechner.</p>
  <div class="cta"><a class="btn primary" href="__REPO__">Auf GitHub ansehen</a><a class="btn" href="#download">Download</a></div>
</div>
<div class="wrap"><div class="shot">
  __HERO__
  <div class="toast" id="toast" data-items='__TOASTS__' role="status" aria-live="off">__T0G__<span>__T0T__</span></div>
</div></div></section>

<section class="sec" id="abos"><div class="wrap"><div class="split">
  <div class="copy">
    <h2>Ein Beleg von Apple, vier Abos.</h2>
    <p>Apple, Google Play und PayPal fassen Abos in einer Rechnung zusammen. ausgeloggt zerlegt sie in einzelne Abos mit Betrag, Rhythmus und nächster Abbuchung.</p>
    <p>Preiserhöhungen fallen auf. Behalten wird nur der Betrag, der Tarif und die letzten 4 Ziffern der Karte.</p>
  </div>
  <div class="pair">
    <div class="rcpt" role="img" aria-label="Beleg von Apple mit vier Positionen, zusammen 26,96 Euro">
      <header>__APPLE__Beleg von Apple</header>
      <ul>
        <li><div>iCloud+ 200 GB<span>Verlängert sich monatlich</span></div><em>2,99 €</em></li>
        <li><div>Apple Music<span>Verlängert sich monatlich</span></div><em>10,99 €</em></li>
        <li><div>Duolingo Super<span>Verlängert sich monatlich</span></div><em>6,99 €</em></li>
        <li><div>Strava<span>Verlängert sich monatlich</span></div><em>5,99 €</em></li>
      </ul>
      <footer><span>Gesamt</span><span>26,96 €</span></footer>
    </div>
    __ARROW__
    <div class="found">
      <ul>
        <li><span class="ic">__G_ICLOUD__</span><div class="nm"><b>iCloud+</b><span>Abbuchung am 03.12.</span></div><em>2,99 €</em></li>
        <li><span class="ic">__G_MUSIC__</span><div class="nm"><b>Apple Music</b><span>Abbuchung am 03.12.</span></div><em>10,99 €</em></li>
        <li><span class="ic">__G_DUO__</span><div class="nm"><b>Duolingo</b><span>Abbuchung am 07.12.</span></div><em>6,99 €</em></li>
        <li><span class="ic">__G_STRAVA__</span><div class="nm"><b>Strava</b><span>Abbuchung am 11.12.</span></div><em>5,99 €</em></li>
      </ul>
      <footer><span>Im Jahr</span><b>323,52 €</b></footer>
    </div>
  </div>
</div></div></section>

<section class="sec"><div class="wrap">
  <div class="head"><h2>Jeder Account mit Quelle und Löschweg.</h2><p>Kennt Löschlinks für rund 2.700 Dienste. Du siehst, woher ein Fund stammt, wie sicher der Account ist und was als Nächstes zu tun ist.</p></div>
  <div class="shot">__DETAIL__</div>
</div></section>

<section class="sec" id="mail"><div class="wrap"><div class="split flip">
  <div class="mono" role="img" aria-label="Eine Mail: Absender, Betreff, Datum und Abmeldeheader werden gelesen, der Text nicht.">
    <div class="c"># gelesen</div>
    <div class="row"><span class="k">From</span><span>Spotify &lt;noreply@spotify.com&gt;</span></div>
    <div class="row"><span class="k">Subject</span><span>Willkommen bei Spotify</span></div>
    <div class="row"><span class="k">Date</span><span>Thu, 16 Jul 2020 09:12:44 +0200</span></div>
    <div class="row"><span class="k">List-Unsubscribe</span><span>&lt;https://spotify.com/u/…&gt;</span></div>
    <div class="gap"></div>
    <div class="c"># nicht gelesen</div>
    <div class="bars"><i style="width:92%"></i><i style="width:78%"></i><i style="width:85%"></i><i style="width:40%"></i></div>
  </div>
  <div class="copy">
    <h2>Liest nur Kopfzeilen deiner Mails.</h2>
    <p>Das Postfach wird nur lesend geöffnet, per IMAP, direkt von deinem Rechner. Der Text einer Mail bleibt zu. Nur bei Rechnungen liest die App mehr, und nur wenn du zustimmst.</p>
    <p>Aus Absender und Betreff erkennt sie Willkommensmails, Bestätigungen, Passwortresets und Anmeldehinweise. Daran sieht sie, wo du einen Account hast.</p>
  </div>
</div></div></section>

<section class="sec" id="passwoerter"><div class="wrap"><div class="split">
  <div class="copy">
    <h2>Passwörter werden geprüft, aber nie gespeichert.</h2>
    <p>Aus dem Export deines Passwortmanagers liest die App, wo du Accounts hast und welche Passwörter mehrfach vorkommen. Gespeichert wird kein Passwort.</p>
    <p>Bei der Prüfung auf geleakte Passwörter gehen nur 5 Zeichen des Hashes an Have I Been Pwned. Den Rest vergleicht die App bei dir.</p>
  </div>
  <div class="mono" role="img" aria-label="SHA 1 Hash eines Passworts: die ersten 5 Zeichen gehen an Have I Been Pwned, die restlichen 35 bleiben auf dem Rechner.">
    <div class="row"><span class="k">Passwort</span><span>password</span></div>
    <div class="row"><span class="k">SHA-1</span><span><span class="out">5BAA6</span><span class="in">1E4C9B93F3F0682250B6CF8331B7EE68FD8</span></span></div>
    <div class="gap"></div>
    <div class="row"><span class="k"><span class="out">5 Zeichen</span></span><span>geht an Have I Been Pwned</span></div>
    <div class="row"><span class="k"><span class="in">35 Zeichen</span></span><span class="in">bleiben auf deinem Rechner</span></div>
  </div>
</div></div></section>

<section class="sec" id="briefe"><div class="wrap"><div class="split flip">
  <div class="paper-stage">__PAPER__</div>
  <div class="copy">
    <h2>Briefe als PDF nach DIN 5008.</h2>
    <p>Sie passen ins Fensterkuvert. Name, Adresse und Kennungen setzt die App ein, bei Unterlassungen auch die Mails als Beleg. Fristen zählt sie mit.</p>
    <ul class="kinds">
      <li>Löschung, Art. 17 DSGVO</li><li>Auskunft, Art. 15 DSGVO</li><li>Widerspruch</li><li>Kündigung</li><li>Unterlassung mit Beleg</li><li>Selbstauskunft bei SCHUFA und Co.</li><li>Beschwerde bei der Datenschutzbehörde</li>
    </ul>
  </div>
</div></div></section>

<section class="sec" id="technik"><div class="wrap">
  <h2 style="margin-bottom:40px">Technik</h2>
  <dl class="facts">
    <dt>Tresor</dt><dd>Argon2id, XChaCha20-Poly1305. Eine verschlüsselte Datei, kein Konto.</dd>
    <dt>Mail</dt><dd>IMAP, nur lesend, nur Kopfzeilen. TLS über rustls.</dd>
    <dt>Netzwerk</dt><dd>Dein Mailserver. Optional: Have I Been Pwned, Katalogupdate.</dd>
    <dt>Telemetrie</dt><dd>Keine.</dd>
    <dt>Katalog</dt><dd>Löschlinks für rund 2.700 Dienste.</dd>
    <dt>Stack</dt><dd>Rust, Tauri 2, React.</dd>
    <dt>Lizenz</dt><dd>MIT.</dd>
    <dt>Diese Seite</dt><dd>Keine Cookies, keine Analyse, keine Drittanbieter.</dd>
  </dl>
</div></section>

<section class="sec" id="download"><div class="wrap"><div class="split">
  <div class="copy">
    <h2>Noch kein Download.</h2>
    <p>ausgeloggt ist in früher Entwicklung. Das Backend ist fertig und getestet, die Oberfläche läuft noch mit Beispieldaten. Installer gibt es noch nicht.</p>
    <p><a class="btn primary" href="__REPO__">__GH__Entwicklung auf GitHub verfolgen</a></p>
  </div>
  <div class="code" role="group" aria-label="Selbst bauen"><span class="c"># Selbst bauen (Rust, Node.js 20, Voraussetzungen von Tauri)</span>
git clone __REPO__.git
cd ausgeloggt/desktop/ui
npm ci
npm run tauri dev</div>
</div></div></section>

<section class="sec" id="faq"><div class="wrap">
  <h2 style="margin-bottom:40px">FAQ</h2>
  <div class="faq">__FAQ__</div>
</div></section>
</main>

<footer class="ftr"><div class="wrap">
  <a class="logo" href="./" aria-label="ausgeloggt">__LOGO__</a>
  <ul><li><a href="__REPO__">GitHub</a></li><li><a href="__REPO__/issues">Fehler melden</a></li><li><a href="__REPO__/blob/main/LICENSE">MIT Lizenz</a></li></ul>
  <small>© 2026 ausgeloggt</small>
</div></footer>
<script type="module" src="js/main.js"></script>
</body>
</html>
'''

FAQ = [
 ("Welche Mailanbieter funktionieren?", "<p>Gmail, GMX, WEB.DE, iCloud, Yahoo, T-Online, Posteo, mailbox.org, Fastmail, AOL, freenet.de und Proton Mail über die Bridge. Eigene Server mit IMAP gehen auch. Mehrere Anbieter verlangen ein App Passwort, die App erklärt dir, wo du es erstellst.</p>"),
 ("Geht Outlook?", "<p>Noch nicht. Microsoft lässt für IMAP keine Anmeldung per Passwort mehr zu, nötig ist OAuth. Das ist geplant. Bis dahin kannst du Outlook Accounts über Passwortexporte und Datenexporte finden lassen.</p>"),
 ("Welche Passwortmanager werden gelesen?", "<p>Exporte aus Chrome, Firefox, Safari, Edge, Bitwarden, 1Password, KeePass, LastPass, Proton Pass und Dashlane. Dazu Datenexporte von Facebook und Google Takeout.</p>"),
 ("Ist das erlaubt?", "<p>Ja. Die DSGVO gibt dir ein Recht auf Auskunft (Art. 15) und Löschung (Art. 17). ausgeloggt schreibt die Anträge, abschicken musst du sie selbst. Die Briefe sind Entwürfe und keine Rechtsberatung.</p>"),
 ("Brauche ich eine KI?", "<p>Nein. Optional liest ein lokales Modell über Ollama Screenshots von Anmeldelisten. Die Claude API ist nur auf Wunsch aktiv und bekommt ausschließlich geschwärzte Absender und Betreffzeilen, nie Mailtexte.</p>"),
 ("Was kostet es?", "<p>Nichts. Die App steht unter der MIT Lizenz, braucht kein Konto, und der Quellcode liegt auf <a href=\"" + REPO + "\">GitHub</a>.</p>"),
]

paper = dict(ret="Lena Hartmann · Beispielweg 12 · 10115 Berlin".replace(" · ", ", "), addr="Beispielversand GmbH<br>Datenschutz<br>Musterstraße 1<br>12345 Musterstadt",
   info=[("Datum", "12.11.2026"), ("Zeichen", "AG0173"), ("Frist", "26.11.2026")],
   subj="Aufforderung zur Unterlassung: Werbemails nach Abmeldung",
   p1="Sehr geehrte Damen und Herren,<br>ich habe mich am 02.10.2026 von Ihrem Newsletter abgemeldet. Die Abmeldung wurde bestätigt. Trotzdem erhalte ich weiter Werbemails:",
   th=("Datum", "Absender", "Betreff"), tr=[("05.10.2026", "news@beispielversand.de", "Nur heute: 20 % auf alles"), ("19.10.2026", "news@beispielversand.de", "Dein Warenkorb wartet"), ("03.11.2026", "angebote@beispielversand.de", "Black Week startet")],
   p2="Ich fordere Sie auf, den Versand bis zum 26.11.2026 einzustellen und dies schriftlich zu bestätigen. Zudem bitte ich um Löschung meiner Daten nach Art. 17 DSGVO.",
   p3="Mit freundlichen Grüßen<br><br>Lena Hartmann", foot=("Erstellt mit ausgeloggt", "Seite 1 von 1"))

def build():
    P = paper
    info = "".join(f"<div><span>{a}</span>{b}</div>" for a, b in P["info"])
    rows = "".join(f"<tr><td>{a}</td><td>{b}</td><td>{c}</td></tr>" for a, b, c in P["tr"])
    paper_html = (f'<div class="paper-wrap"><div class="paper" role="img" aria-label="{html.escape(P["subj"])}">'
        '<i class="fm" style="top:calc(105*var(--mm))"></i><i class="fm" style="top:calc(148.5*var(--mm));width:calc(7*var(--mm))"></i><i class="fm" style="top:calc(210*var(--mm))"></i>'
        f'<div class="stamp"><i></i>ausgeloggt</div><p class="ret">{P["ret"]}</p><p class="addr">{P["addr"]}</p><div class="info">{info}</div>'
        f'<p class="subj">{P["subj"]}</p><div class="body"><p>{P["p1"]}</p><table><thead><tr><th>{P["th"][0]}</th><th>{P["th"][1]}</th><th>{P["th"][2]}</th></tr></thead><tbody>{rows}</tbody></table><p>{P["p2"]}</p><p>{P["p3"]}</p></div>'
        f'<div class="foot"><span>{P["foot"][0]}</span><span>{P["foot"][1]}</span></div></div></div>')
    hero = (pic("dashboard", "Die Übersicht von ausgeloggt mit 73 gefundenen Accounts, Fristen und zuletzt gefundenen Diensten", ' class="theme-dark"', lazy=False)
            + pic("dashboard-light", "Die Übersicht von ausgeloggt im hellen Design", ' class="theme-light"'))
    ld = json.dumps({"@context": "https://schema.org", "@type": "SoftwareApplication", "name": "ausgeloggt", "operatingSystem": "Windows, macOS, Linux",
        "applicationCategory": "SecurityApplication", "description": DESC, "url": SITE, "inLanguage": "de", "image": SITE + "assets/brand/og-image.png",
        "offers": {"@type": "Offer", "price": "0", "priceCurrency": "EUR"}, "license": "https://opensource.org/licenses/MIT", "isAccessibleForFree": True,
        "codeRepository": REPO, "sameAs": [REPO]}, ensure_ascii=False)
    faq = "".join(f"<details><summary>{q}</summary><div class=\"a\">{a}</div></details>" for q, a in FAQ)
    rep = {"__TITLE__": TITLE, "__DESC__": DESC, "__SITE__": SITE, "__REPO__": REPO, "__LD__": ld, "__SPRITE__": SPRITE, "__LOGO__": LOGO, "__GH__": GH,
           "__HERO__": hero, "__TOASTS__": html.escape(json.dumps(TOASTS, ensure_ascii=False), quote=True), "__T0G__": g(TOASTS[0]["g"]), "__T0T__": TOASTS[0]["t"],
           "__APPLE__": g("apple"), "__ARROW__": ARROW, "__G_ICLOUD__": g("apple"), "__G_MUSIC__": g("applemusic"), "__G_DUO__": g("duolingo"), "__G_STRAVA__": g("strava"),
           "__DETAIL__": pic("detail", "Accountliste mit geöffnetem Detailbereich zu Spotify: Kennungen, Hinweise und Löschen"),
           "__PAPER__": paper_html, "__FAQ__": faq}
    out = PAGE
    for k, v in rep.items(): out = out.replace(k, v)
    open(os.path.join(ROOT, "index.html"), "w", encoding="utf-8").write(out)
    open(os.path.join(ROOT, "robots.txt"), "w").write(f"User-agent: *\nAllow: /\n\nSitemap: {SITE}sitemap.xml\n")
    open(os.path.join(ROOT, "sitemap.xml"), "w").write(f'''<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>{SITE}</loc><changefreq>monthly</changefreq><priority>1.0</priority></url>
</urlset>
''')
build()
print("ok")
