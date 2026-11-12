#!/usr/bin/env bash
# Captures app screenshots (mock demo data) into site/assets/shots/ as WebP (2x) + PNG fallback (1x).
# Needs: the app UI dev server (cd desktop/ui && npm run dev -- --port 1420), Chrome, Python + Pillow.
set -euo pipefail
cd "$(dirname "$0")/.."
CHROME="${CHROME:-/c/Program Files/Google/Chrome/Application/chrome.exe}"
BASE="${BASE:-http://localhost:1420}"
OUT=assets/shots; RAW="${TMPDIR:-/tmp}/ausgeloggt-raw"; mkdir -p "$OUT" "$RAW"
curl -sf "$BASE/" >/dev/null || { echo "UI dev server not running at $BASE (cd desktop/ui && npm run dev -- --port 1420)"; exit 1; }
RAWW="$(cd "$RAW" && (pwd -W 2>/dev/null || pwd))"
shoot(){ # name  query+hash
  "$CHROME" --headless=new --hide-scrollbars --disable-gpu --window-size=1440,900 --force-device-scale-factor=2 \
    --virtual-time-budget=5000 --screenshot="$RAWW/$1.png" "$BASE/?mock$2" >/dev/null 2>&1
  echo "shot $1"
}
shoot dashboard        "#/"
shoot dashboard-light  "&theme=light#/"
shoot accounts         "#/accounts"
shoot detail           "&select=spotify#/accounts"
shoot letter           "&select=spotify&gdpr#/accounts"
shoot newsletter       "#/newsletter"
shoot deletions        "#/deletions"
shoot palette          "&palette#/accounts"
python - "$RAWW" "$OUT" <<'PY'
import sys,glob,os
from PIL import Image
raw,out=sys.argv[1:3]
for f in sorted(glob.glob(raw+"/*.png")):
    n=os.path.splitext(os.path.basename(f))[0]
    im=Image.open(f).convert("RGB")
    im.save(f"{out}/{n}.webp","WEBP",quality=82,method=6)
    im.resize((1440,900),Image.LANCZOS).quantize(256,method=Image.Quantize.MEDIANCUT,dither=Image.Dither.NONE).save(f"{out}/{n}.png",optimize=True)
    print(n, os.path.getsize(f"{out}/{n}.webp")//1024,"KB webp")
PY
