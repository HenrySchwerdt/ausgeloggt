# ausgeloggt – Design-Spezifikation

Vorbild: T3 Code, Linear, Vercel, shadcn/ui ("zinc"). Umsetzung mit Tailwind v4 + shadcn/ui: die Tokens unten sind CSS-Variablen. **Ruhig, flach, monochrom, präzise.**
Farbe ist Information, keine Dekoration. Im Zweifel: weniger Rahmen, weniger Farbe, mehr Weißraum.

## Tokens

| Token | Dark | Light | Verwendung |
|---|---|---|---|
| `bg` | `#09090b` | `#ffffff` | Fenster, Inhalt |
| `sidebar` | `#0c0c0e` | `#fafafa` | Seitenleiste |
| `surface` | `#111113` | `#ffffff` | Karten (selten!), Popover, Dialoge |
| `muted` | `#18181b` | `#f4f4f5` | Hover, aktiver Nav-Eintrag, Inputs, Segment-Hintergrund |
| `muted_hover` | `#1f1f23` | `#ececee` | Hover auf muted |
| `border` | `#1f1f23` | `#e4e4e7` | Haarlinien (1 px) |
| `border_strong` | `#2e2e33` | `#d4d4d8` | Inputs, Fokus-frei |
| `text` | `#fafafa` | `#09090b` | Primärtext |
| `text_muted` | `#a1a1aa` | `#71717a` | Sekundärtext, Labels, Icons |
| `text_faint` | `#63636b` | `#a1a1aa` | Hilfstext, Platzhalter |
| `primary_bg` / `primary_fg` | `#fafafa` / `#09090b` | `#18181b` / `#fafafa` | Primärer Button (weiß auf schwarz) |
| Status-Punkte | grün `#22c55e`, gelb `#eab308`, rot `#ef4444`, blau `#3b82f6`, violett `#a855f7`, grau `#71717a` | gleiche | **nur** als 6-px-Punkt oder 2-px-Balken |

- Radius: 6 px (Buttons, Inputs, Rows), 8 px (Karten, Dialoge). Nie mehr als 10.
- Schatten: keine. Ausnahme: Popover/Dialog `0 8 24 rgba(0,0,0,.35)`.
- Abstände: 4/8/12/16/24/32. Seiten-Padding 28 px. Inhalt max. 1120 px breit (Übersicht zentriert).

## Typografie (Inter)

- Body 13 px Regular, `text`. Sekundär 13 px `text_muted`. Klein 12 px.
- Zeilentitel 13–14 px Medium. Seitentitel 20 px SemiBold. Große Zahlen 26 px SemiBold, tabular.
- Section-Label: 12 px Medium `text_faint` (keine Großbuchstaben-Kaskaden).
- Keine fetten bunten Überschriften, keine Emoji.

## Komponenten

- **Sidebar** 220 px, Hintergrund `sidebar`, rechts 1-px-`border`. Oben Logo (20 px) + „ausgeloggt“ 14 px SemiBold.
  Nav-Einträge 30 px hoch, Icon 16 px `text_muted` + Label 13 px; Hover `muted`; aktiv `muted` + Text `text` + Icon `text`.
  **Kein** farbiger Balken, **keine** gefüllten Boxen pro Eintrag. Zähler rechts als 12 px `text_faint` Zahl (kein Badge).
  Unten: kleiner Tresor-Status (Punkt + „Entsperrt“), Theme-Umschalter und „Sperren“ als Ghost-Buttons.
- **Header** der Seite: Titel links (20 px), darunter 13 px `text_muted` Beschreibung; Aktionen rechts.
  Suche als Command-Button (`muted`-Hintergrund, Platzhalter `text_faint`, rechts `Strg K` Kbd-Tag).
- **Buttons:** Primary (`primary_bg`/`primary_fg`), Secondary (`muted` bg, kein Rand), Ghost (transparent, Hover `muted`),
  Destructive (Text rot, Hover rot 10 %). Höhe 30 px, Padding 12 px, Icon 14 px.
- **Inputs/Dropdowns:** 30 px, transparenter Hintergrund, 1-px-`border_strong`, Radius 6.
- **Tabs/Filter:** Segment-Control (`muted`-Container, aktives Segment `bg`/`surface`) oder schlichte Text-Tabs
  mit 1-px-Unterstrich. Zähler daneben in `text_faint`.
- **Listen statt Karten:** Zeilen 44 px, keine Einzelrahmen, nur 1-px-Trenner `border`, Hover `muted`, Auswahl `muted`.
  Spalten wie eine Tabelle: Avatar · Name/Domain · Kennung · Quellen · Status · Sicherheit (rechtsbündig).
- **Status:** 6-px-Punkt + 12–13 px `text_muted` Label. Keine farbig gefüllten Pills.
- **Sicherheit/Konfidenz:** rechtsbündige Zahl `text_muted` oder 40-px-Mini-Balken (2 px, Statusfarbe).
- **Avatar:** 24 px, Radius 6, `muted` Hintergrund, 1 Buchstabe 12 px Medium `text_muted`. Keine Pastellfarben.
- **SSO-Badges:** kleiner `muted`-Tag 11 px mit Providername, kein Rand.
- **Kennzahlen (Übersicht):** eine Zeile mit 4 Kennzahlen (responsive 4/2/1 Spalten), je: 12 px Label `text_muted`,
  darunter 26 px Zahl. Karten mit 1-px-`border`, `bg`-Hintergrund, kein Icon-Kasten.
- **Fortschritt:** dünner horizontaler Balken (4 px, Radius 2) statt Ring, mit kurzer Legende als Text darunter.
- **Detail-Panel:** rechts, 420 px, gleicher `bg`, linke 1-px-Linie. **Keine** verschachtelten Karten:
  Abschnitte mit 12-px-Label und Haarlinie dazwischen. Aktionen als Button-Gruppe.
- **Leere Zustände:** zentriert, 32-px-Icon `text_faint`, ein Satz `text_muted`, optional ein Button. Nie leere Karten.
- **Dialoge:** `surface`, 1-px-`border`, Radius 8, Overlay `rgba(0,0,0,.6)`.
- **Toasts:** unten rechts, `surface`, 1-px-`border`, Statuspunkt + Text.

## Icons

Ein einziger Icon-Satz in Linienstil (Phosphor Regular oder Lucide), 16 px in Nav, 14 px in Buttons, Farbe `text_muted`.
Jedes Icon muss korrekt rendern. Kaputte Glyphen sind ein Blocker.
