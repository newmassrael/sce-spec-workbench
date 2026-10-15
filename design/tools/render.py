"""Render a .dc.html board to a PNG with headless Chrome (local preview only).

usage: python3 -I render.py <board.dc.html> <out.png> [width] [height] [key=value ...]
Holes {{key}} are replaced with the key=value pairs; unknown holes become empty.
THEME=dark in the environment renders the dark theme.

This is a static preview: it does not run the board's script, and its handling of
<sc-if> is a regular expression, so a nested <sc-if> shows both branches. A hole
that the script would compute has to be passed on the command line.
"""
import os
import re
import subprocess
import sys
from pathlib import Path

src = Path(sys.argv[1]).resolve()
out = Path(sys.argv[2]).resolve()
width = int(sys.argv[3]) if len(sys.argv) > 3 else 1280
height = int(sys.argv[4]) if len(sys.argv) > 4 else 800
vals = dict(a.split("=", 1) for a in sys.argv[5:])

html = src.read_text(encoding="utf-8")
helmet = "".join(re.findall(r"<helmet>(.*?)</helmet>", html, re.S))
html = re.sub(r"<helmet>.*?</helmet>", "", html, flags=re.S)
html = re.sub(r"<script type=\"text/x-dc\".*?</script>", "", html, flags=re.S)
html = html.replace('<script src="./support.js"></script>', helmet)
html = html.replace("<x-dc>", "").replace("</x-dc>", "")
def _sc_if(m):
    return m.group(2) if vals.get(m.group(1), "") not in ("", "0", "false") else ""
html = re.sub(r"<sc-if value=\"\{\{\s*([\w.$]+)\s*\}\}\"[^>]*>(.*?)</sc-if>", _sc_if, html, flags=re.S)
html = re.sub(r"\{\{\s*([\w.$]+)\s*\}\}", lambda m: vals.get(m.group(1), ""), html)
html = html.replace(' disabled=""', "")
if os.environ.get("THEME") == "dark":
    html = html.replace('<html lang="ko">', '<html lang="ko" data-theme="dark">')
tmp = src.parent / ("_render_" + src.name.replace(".dc.html", ".html"))
tmp.write_text(html, encoding="utf-8")
subprocess.run(
    ["google-chrome", "--headless=new", "--disable-gpu", "--no-sandbox",
     "--hide-scrollbars", "--virtual-time-budget=6000",
     f"--window-size={width},{height}", f"--screenshot={out}", tmp.as_uri()],
    check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=90)
tmp.unlink()
print("wrote", out)
