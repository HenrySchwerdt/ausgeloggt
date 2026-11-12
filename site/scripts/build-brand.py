"""Generates brand SVGs (mark, wordmark with outlined Inter glyphs, favicon). Run: python site/scripts/build-brand.py"""
import os
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "assets", "brand")
FONT = os.path.join(ROOT, "assets", "fonts", "inter-latin-wght-normal.woff2")

# --- mark: a door frame that is open on the right, and the "you" dot that has just stepped out -------------
FRAME = "M49 24v-3a10 10 0 0 0-10-10H19a10 10 0 0 0-10 10v22a10 10 0 0 0 10 10h20a10 10 0 0 0 10-10v-3"
def mark_inner(fg, dot=None):
    dot = dot or fg
    return (f'<path d="{FRAME}" fill="none" stroke="{fg}" stroke-width="6.5" stroke-linecap="round" stroke-linejoin="round"/>'
            f'<circle cx="57" cy="32" r="5.5" fill="{dot}"/>')
def svg(w, h, inner, vb=None, extra=""):
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb or f"0 0 {w} {h}"}" width="{w}" height="{h}"{extra}>{inner}</svg>\n'
def write(name, s):
    open(os.path.join(OUT, name), "w", encoding="utf-8").write(s)

# tight viewBox: frame spans x 7.75..57.5 -> pad to a 64 square, keeps optical centring
write("mark.svg", svg(64, 64, '<title>ausgeloggt</title>' + mark_inner("#09090b")))
write("mark-dark.svg", svg(64, 64, '<title>ausgeloggt</title>' + mark_inner("#fafafa")))
fav = ('<style>path{stroke:#09090b}circle{fill:#09090b}@media(prefers-color-scheme:dark){path{stroke:#fafafa}circle{fill:#fafafa}}</style>'
       f'<path d="{FRAME}" fill="none" stroke-width="8" stroke-linecap="round" stroke-linejoin="round"/><circle cx="57" cy="32" r="6"/>')
write("favicon.svg", svg(64, 64, fav))

# --- wordmark: outlined Inter SemiBold so it renders identically everywhere -------------------------------
f = instantiateVariableFont(TTFont(FONT), {"wght": 620})
gs, cmap, upm = f.getGlyphSet(), f.getBestCmap(), f["head"].unitsPerEm
def text_path(txt, size, x0, base, track=-0.02):
    x, parts = x0, []
    k = size / upm
    for ch in txt:
        g = cmap[ord(ch)]
        pen = SVGPathPen(gs, ntos=lambda v: f"{v:.2f}".rstrip("0").rstrip("."))
        gs[g].draw(TransformPen(pen, (k, 0, 0, -k, x, base)))
        parts.append(pen.getCommands())
        x += gs[g].width * k + track * size
    return "".join(parts), x
d, end = text_path("ausgeloggt", 36, 76, 43.5)
W = round(end + 4)
for name, fg in (("wordmark.svg", "#09090b"), ("wordmark-dark.svg", "#fafafa")):
    write(name, svg(W, 64, f'<title>ausgeloggt</title>{mark_inner(fg)}<path d="{d}" fill="{fg}"/>'))
print("ok", W)
