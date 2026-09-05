#!/usr/bin/env python3
"""Render a tmux `capture-pane -e` file to a PNG the way a foot terminal
in the current Omarchy theme would show it.

    render.py SCREEN.ansi OUT.png COLSxROWS [--theme NAME]

The palette comes from the current theme's foot.ini, or from
/usr/share/omarchy/themes/NAME/ when --theme is given. Only the SGR
sequences the app uses are understood: reset, bold, dim, reverse, the
sixteen colours as 30-37/90-97/40-47/100-107 and 38;5;n/48;5;n. The HTML
is written next to the PNG and turned into an image with Playwright's
Chromium, which is installed under mise.
"""
import html, os, re, subprocess, sys
from pathlib import Path

PLAYWRIGHT = Path.home() / ".local/share/mise/installs/npm-playwright/latest/node_modules"


def palette(theme):
    """Sixteen colours plus foreground and background from a foot.ini."""
    if theme:
        candidates = [Path(f"/usr/share/omarchy/themes/{theme}/foot.ini")]
    else:
        candidates = [Path.home() / ".local/state/omarchy/current/theme/foot.ini"]
    colours = {}
    for path in candidates:
        if path.exists():
            for line in path.read_text().splitlines():
                m = re.match(r"\s*(\w+)\s*=\s*([0-9a-fA-F]{6})", line)
                if m:
                    colours[m.group(1)] = "#" + m.group(2)
            break
    slots = []
    for i in range(8):
        slots.append(colours.get(f"regular{i}", ["#282828", "#ea6962", "#a9b665", "#d8a657", "#7daea3", "#d3869b", "#89b482", "#d4be98"][i]))
    for i in range(8):
        slots.append(colours.get(f"bright{i}", slots[i] if i else "#665c54"))
    return slots, colours.get("foreground", slots[7]), colours.get("background", slots[0])


def render(ansi, cols, rows, slots, fg, bg):
    """Turn one line of ANSI into HTML spans; state resets per line, which
    is what capture-pane -e produces."""
    out = []
    for line in ansi.split("\n")[:rows]:
        state = dict(bold=False, dim=False, rev=False, fg=None, bg=None)
        spans = []
        text = ""
        width = 0

        def flush():
            nonlocal text
            if not text:
                return
            f = state["fg"] or fg
            b = state["bg"] or bg
            if state["rev"]:
                f, b = b, f
            style = f"color:{f};background:{b}"
            if state["bold"]:
                style += ";font-weight:bold"
            if state["dim"]:
                style += ";opacity:.55"
            spans.append(f'<span style="{style}">{html.escape(text)}</span>')
            text = ""

        i = 0
        while i < len(line):
            m = re.match(r"\x1b\[([0-9;]*)m", line[i:])
            if m:
                flush()
                params = [int(p) if p else 0 for p in m.group(1).split(";")] or [0]
                j = 0
                while j < len(params):
                    p = params[j]
                    if p == 0:
                        state.update(bold=False, dim=False, rev=False, fg=None, bg=None)
                    elif p == 1:
                        state["bold"] = True
                    elif p == 2:
                        state["dim"] = True
                    elif p == 7:
                        state["rev"] = True
                    elif p == 22:
                        state["bold"] = state["dim"] = False
                    elif p == 27:
                        state["rev"] = False
                    elif 30 <= p <= 37:
                        state["fg"] = slots[p - 30]
                    elif 90 <= p <= 97:
                        state["fg"] = slots[p - 90 + 8]
                    elif 40 <= p <= 47:
                        state["bg"] = slots[p - 40]
                    elif 100 <= p <= 107:
                        state["bg"] = slots[p - 100 + 8]
                    elif p == 39:
                        state["fg"] = None
                    elif p == 49:
                        state["bg"] = None
                    elif p in (38, 48) and j + 2 < len(params) and params[j + 1] == 5:
                        n = params[j + 2]
                        colour = slots[n] if n < 16 else "#ff00ff"
                        state["fg" if p == 38 else "bg"] = colour
                        j += 2
                    elif p in (38, 48) and j + 4 < len(params) and params[j + 1] == 2:
                        colour = "#%02x%02x%02x" % tuple(params[j + 2 : j + 5])
                        state["fg" if p == 38 else "bg"] = colour
                        j += 4
                    j += 1
                i += m.end()
                continue
            if line[i] == "\x1b":
                m = re.match(r"\x1b\[[0-9;?]*[A-Za-z]", line[i:])
                i += m.end() if m else 1
                continue
            text += line[i]
            width += 1
            i += 1
        flush()
        if width < cols:
            spans.append(html.escape(" " * (cols - width)))
        out.append("".join(spans))
    while len(out) < rows:
        out.append(" " * cols)
    body = "\n".join(out)
    return f"""<!doctype html><meta charset=utf-8>
<style>
body{{margin:0;background:{bg}}}
pre{{margin:0;padding:14px;background:{bg};color:{fg};
font:9pt/1.25 "JetBrainsMono Nerd Font","JetBrains Mono","Liberation Mono",monospace;
display:inline-block;white-space:pre}}
</style><pre>{body}</pre>"""


def main():
    args = sys.argv[1:]
    theme = None
    if "--theme" in args:
        k = args.index("--theme")
        theme = args[k + 1]
        del args[k : k + 2]
    src, dst, size = args[0], args[1], args[2]
    cols, rows = (int(x) for x in size.split("x"))
    slots, fg, bg = palette(theme)
    page = render(Path(src).read_text(errors="replace"), cols, rows, slots, fg, bg)
    html_path = Path(dst).with_suffix(".html")
    html_path.write_text(page)
    script = f"""
const {{ chromium }} = require('playwright');
(async () => {{
  const b = await chromium.launch();
  const p = await b.newPage({{ deviceScaleFactor: 2 }});
  await p.goto('file://{html_path}');
  const pre = await p.$('pre');
  await pre.screenshot({{ path: '{dst}' }});
  await b.close();
}})();
"""
    env = dict(os.environ, NODE_PATH=str(PLAYWRIGHT))
    subprocess.run(["node", "-e", script], env=env, check=True)


if __name__ == "__main__":
    main()
