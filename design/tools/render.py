"""Render a .dc.html board to a PNG with headless Chrome (local preview only).

usage: python3 -I render.py <board.dc.html> <out.png> [width] [height] [key=value | @values.json ...]

The template is expanded here, not in a browser, and the board's own script is not run: the values
come from the command line, or from a JSON file named after an @ (made by `vals.js`, which does run
the script). Supported: {{dotted.holes}}, <sc-if value="{{x}}"> and <sc-for list="{{x}}" as="item">,
nested to any depth. A hole that is not there is empty; a list or an object is not printed.
THEME=dark in the environment renders the dark theme. One Chrome is started and finished; run one
render at a time.
"""
import json
import os
import re
import subprocess
import sys
from pathlib import Path

HOLE = re.compile(r"\{\{\s*([\w.$]+)\s*\}\}")
TAG = re.compile(r"<(/?)sc-(for|if)\b([^>]*)>")
ATTR_HOLE = re.compile(r"(\w[\w-]*)=\"\{\{\s*([\w.$]+)\s*\}\}\"")


def look(path, scope):
    cur = scope
    for part in path.split("."):
        if isinstance(cur, dict) and part in cur:
            cur = cur[part]
        elif isinstance(cur, list) and part.isdigit() and int(part) < len(cur):
            cur = cur[int(part)]
        else:
            return None
    return cur


def truthy(value):
    return value not in (None, False, 0, "", "0", "false", [], {})


def text_of(value):
    if value is None or value is False:
        return ""
    if value is True:
        return "1"
    if isinstance(value, (list, dict)):
        return ""
    return str(value)


def expand(tpl, scope):
    out = []
    pos = 0
    while True:
        m = TAG.search(tpl, pos)
        if m is None or m.group(1):
            out.append(HOLE.sub(lambda h: text_of(look(h.group(1), scope)), tpl[pos:]))
            return "".join(out)
        out.append(HOLE.sub(lambda h: text_of(look(h.group(1), scope)), tpl[pos:m.start()]))
        kind, attrs = m.group(2), m.group(3)
        depth, scan, end = 1, m.end(), None
        while depth:
            n = re.compile(r"<(/?)sc-%s\b" % kind).search(tpl, scan)
            if n is None:
                raise SystemExit("unclosed <sc-%s> in the template" % kind)
            depth += -1 if n.group(1) else 1
            scan = n.end()
            end = n.start()
        close = tpl.index(">", scan) + 1
        inner = tpl[m.end():end]
        holes = dict(ATTR_HOLE.findall(attrs))
        if kind == "if":
            if truthy(look(holes.get("value", ""), scope)):
                out.append(expand(inner, scope))
        else:
            items = look(holes.get("list", ""), scope) or []
            name = (re.search(r'\bas="(\w+)"', attrs) or [None, "item"])[1]
            for i, item in enumerate(items):
                out.append(expand(inner, {**scope, name: item, "$index": i}))
        pos = close


def main():
    src = Path(sys.argv[1]).resolve()
    out = Path(sys.argv[2]).resolve()
    width = int(sys.argv[3]) if len(sys.argv) > 3 else 1280
    height = int(sys.argv[4]) if len(sys.argv) > 4 else 800
    vals = {}
    for arg in sys.argv[5:]:
        if arg.startswith("@"):
            vals.update(json.loads(Path(arg[1:]).read_text(encoding="utf-8")))
        else:
            key, value = arg.split("=", 1)
            vals[key] = value

    html = src.read_text(encoding="utf-8")
    helmet = "".join(re.findall(r"<helmet>(.*?)</helmet>", html, re.S))
    html = re.sub(r"<helmet>.*?</helmet>", "", html, flags=re.S)
    html = re.sub(r"<script type=\"text/x-dc\".*?</script>", "", html, flags=re.S)
    html = html.replace('<script src="./support.js"></script>', helmet)
    html = html.replace("<x-dc>", "").replace("</x-dc>", "")
    html = expand(html, vals)
    html = html.replace(' disabled=""', "")
    if os.environ.get("THEME") == "dark":
        html = html.replace('<html lang="ko">', '<html lang="ko" data-theme="dark">')
    tmp = src.parent / ("_render_" + src.name.replace(".dc.html", ".html"))
    tmp.write_text(html, encoding="utf-8")
    try:
        subprocess.run(
            ["google-chrome", "--headless=new", "--disable-gpu", "--no-sandbox",
             "--hide-scrollbars", "--virtual-time-budget=6000",
             f"--window-size={width},{height}", f"--screenshot={out}", tmp.as_uri()],
            check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=90)
    finally:
        tmp.unlink(missing_ok=True)
    print("wrote", out)


main()
