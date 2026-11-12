#!/usr/bin/env bash
# Renders brand rasters via headless Chrome + Pillow. Run after build-brand.py.
set -euo pipefail
cd "$(dirname "$0")/.."
CHROME="${CHROME:-/c/Program Files/Google/Chrome/Application/chrome.exe}"
B=assets/brand
shot(){ "$CHROME" --headless=new --hide-scrollbars --disable-gpu --default-background-color=00000000 --force-device-scale-factor=1 --window-size="$2" --virtual-time-budget=3000 --screenshot="$(pwd -W 2>/dev/null || pwd)/$B/$3" "file:///$(pwd -W 2>/dev/null || pwd)/scripts/templates/$1" >/dev/null 2>&1; }
shot app-icon.html 1024,1024 app-icon-1024.png
shot og.html 1200,630 og-image.png
python - <<'PY'
from PIL import Image
b="assets/brand/"
im=Image.open(b+"app-icon-1024.png").convert("RGBA")
im.save(b+"favicon.ico",sizes=[(16,16),(32,32),(48,48),(64,64)])
im.resize((180,180),Image.LANCZOS).save(b+"apple-touch-icon.png")
im.resize((512,512),Image.LANCZOS).save(b+"icon-512.png")
Image.open(b+"og-image.png").convert("RGB").save(b+"og-image.png",optimize=True)
PY
