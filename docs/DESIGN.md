# ausgeloggt – Design specification

References: T3 Code, Linear, Vercel, shadcn/ui ("zinc"). Implemented with Tailwind v4 + shadcn/ui; the tokens
below are CSS variables. **Calm, flat, monochrome, precise.**
Colour is information, not decoration. When in doubt: fewer borders, less colour, more whitespace.

## Tokens

| Token | Dark | Light | Use |
|---|---|---|---|
| `bg` | `#09090b` | `#ffffff` | Window, content |
| `sidebar` | `#0c0c0e` | `#fafafa` | Sidebar |
| `surface` | `#111113` | `#ffffff` | Cards (rarely!), popovers, dialogs |
| `muted` | `#18181b` | `#f4f4f5` | Hover, active nav item, inputs, segment background |
| `muted_hover` | `#1f1f23` | `#ececee` | Hover on muted |
| `border` | `#1f1f23` | `#e4e4e7` | Hairlines (1 px) |
| `border_strong` | `#2e2e33` | `#d4d4d8` | Inputs |
| `text` | `#fafafa` | `#09090b` | Primary text |
| `text_muted` | `#a1a1aa` | `#71717a` | Secondary text, labels, icons |
| `text_faint` | `#63636b` | `#a1a1aa` | Hints, placeholders |
| `primary_bg` / `primary_fg` | `#fafafa` / `#09090b` | `#18181b` / `#fafafa` | Primary button (white on black) |
| Status dots | green `#22c55e`, yellow `#eab308`, red `#ef4444`, blue `#3b82f6`, violet `#a855f7`, grey `#71717a` | same | **only** as a 6 px dot or 2 px bar |

- Radius: 6 px (buttons, inputs, rows), 8 px (cards, dialogs). Never more than 10.
- Shadows: none. Exception: popover / dialog `0 8 24 rgba(0,0,0,.35)`.
- Spacing: 4/8/12/16/24/32. Page padding 28 px. Content max 1120 px wide (overview centred).

## Typography (Inter)

- Body 13 px regular, `text`. Secondary 13 px `text_muted`. Small 12 px.
- Row titles 13–14 px medium. Page titles 20 px semibold. Large numbers 26 px semibold, tabular.
- Section labels: 12 px medium `text_faint` (no all-caps cascades).
- No bold colourful headings, no emoji.

## Components

- **Sidebar** 220 px, `sidebar` background, 1 px `border` on the right. Top: logo (20 px) + "ausgeloggt" 14 px semibold.
  Nav items 30 px high, 16 px icon `text_muted` + 13 px label; hover `muted`; active `muted` + `text` label and icon.
  **No** coloured bar, **no** filled boxes per item. Counts on the right as 12 px `text_faint` numbers (no badges).
  Bottom: small vault status (dot + "Entsperrt"), theme toggle and "Sperren" as ghost buttons.
- **Page header:** title left (20 px), 13 px `text_muted` description below; actions right.
  Search as a command button (`muted` background, `text_faint` placeholder, `Strg K` kbd tag on the right).
- **Buttons:** primary (`primary_bg`/`primary_fg`), secondary (`muted` bg, no border), ghost (transparent, hover `muted`),
  destructive (red text, hover red 10 %). Height 30 px, padding 12 px, icon 14 px.
- **Inputs / dropdowns:** 30 px, transparent background, 1 px `border_strong`, radius 6.
- **Tabs / filters:** segmented control (`muted` container, active segment `bg`/`surface`) or plain text tabs with a
  1 px underline. Counts next to them in `text_faint`.
- **Lists instead of cards:** 44 px rows, no per-row borders, only 1 px `border` dividers, hover `muted`, selection `muted`.
  Table-like columns: avatar · name/domain · identifier · sources · status · confidence (right-aligned).
- **Status:** 6 px dot + 12–13 px `text_muted` label. No colour-filled pills.
- **Confidence:** right-aligned `text_muted` number or a 40 px mini bar (2 px, status colour).
- **Avatar:** 24 px, radius 6, `muted` background, one 12 px medium `text_muted` letter. No pastel colours.
- **SSO tags:** small 11 px `muted` tag with the provider name, no border.
- **Key figures (overview):** one row of 4 figures (responsive 4/2/1 columns), each a 12 px `text_muted` label
  above a 26 px number. Cards with 1 px `border`, `bg` background, no icon boxes.
- **Progress:** thin horizontal bar (4 px, radius 2) instead of a ring, short text legend below.
- **Detail panel:** right, 420 px, same `bg`, 1 px line on the left. **No** nested cards:
  sections with 12 px labels and hairlines between them. Actions as a button group.
- **Empty states:** centred, 32 px `text_faint` icon, one `text_muted` sentence, optional button. Never empty cards.
- **Dialogs:** `surface`, 1 px `border`, radius 8, overlay `rgba(0,0,0,.6)`.
- **Toasts:** bottom right, `surface`, 1 px `border`, status dot + text.

## Icons

One line-style icon set (Lucide), 16 px in navigation, 14 px in buttons, colour `text_muted`.
Every icon must render correctly; broken glyphs are a blocker.
