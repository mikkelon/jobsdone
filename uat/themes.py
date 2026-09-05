#!/usr/bin/env python3
"""Render a captured screen in every installed Omarchy theme on one sheet,
light and dark side by side, for the colour check.

    themes.py SCREEN.ansi... [--themes a,b,c] [--columns N] [--scale N] [--out DIR]

One PNG per screen lands in DIR (default uat/out/themes), named after the
capture. Each cell is the screen the way render.py draws it, captioned
with the theme's name and mode. --themes limits the sheet to a few, at
--scale 2 for a close look; without it every theme under
/usr/share/omarchy/themes is on the sheet at scale 1.
"""
import html, os, re, subprocess, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import render  # noqa: E402

THEMES = Path("/usr/share/omarchy/themes")


def mode_of(theme):
    text = (THEMES / theme / "colors.toml").read_text(errors="replace")
    return "light" if re.search(r'^mode\s*=\s*"light"', text, re.M) else "dark"


def cell(theme, ansi, cols, rows):
    slots, fg, bg = render.palette(theme)
    page = render.render(ansi, cols, rows, slots, fg, bg)
    pre = re.search(r"<pre>.*</pre>", page, re.S).group(0)
    caption = f"{theme} · {mode_of(theme)}"
    # Dim text is drawn with opacity, so the cell behind it must be the
    # theme's own background or the sheet's grey shows through.
    return f'<div class="cell"><div class="cap">{html.escape(caption)}</div><div style="background:{bg}">{pre}</div></div>', fg, bg


def sheet(ansi_path, themes, columns, out_dir, scale):
    text = ansi_path.read_text(errors="replace")
    rows = len(text.rstrip("\n").split("\n"))
    cols = max(len(re.sub(r"\x1b\[[0-9;?]*[A-Za-z]", "", line)) for line in text.split("\n"))
    cells = [cell(t, text, cols, rows)[0] for t in themes]
    page = f"""<!doctype html><meta charset=utf-8>
<style>
body{{margin:0;background:#808080;padding:12px;font-family:sans-serif}}
.grid{{display:grid;grid-template-columns:repeat({columns},max-content);gap:12px}}
.cell{{display:inline-block}}
.cap{{color:#111;font:12px sans-serif;margin:0 0 3px 2px}}
pre{{margin:0;padding:14px;font:9pt/1.25 "JetBrainsMono Nerd Font","JetBrains Mono","Liberation Mono",monospace;display:inline-block;white-space:pre}}
</style><div class="grid">{"".join(cells)}</div>"""
    out_dir.mkdir(parents=True, exist_ok=True)
    html_path = out_dir / (ansi_path.stem + ".html")
    png_path = out_dir / (ansi_path.stem + ".png")
    html_path.write_text(page)
    script = f"""
const {{ chromium }} = require('playwright');
(async () => {{
  const b = await chromium.launch();
  const p = await b.newPage({{ deviceScaleFactor: {scale} }});
  await p.goto('file://{html_path}');
  await p.screenshot({{ path: '{png_path}', fullPage: true }});
  await b.close();
}})();
"""
    env = dict(os.environ, NODE_PATH=str(render.PLAYWRIGHT))
    for attempt in (1, 2):
        result = subprocess.run(["node", "-e", script], env=env, capture_output=True, text=True)
        if result.returncode == 0:
            break
        if attempt == 2:
            sys.exit(f"themes.py: chromium failed on {html_path.name}:\n{result.stderr.strip()}")
    print(f"{len(themes)} themes -> {png_path.relative_to(Path.cwd()) if png_path.is_relative_to(Path.cwd()) else png_path}")


def main():
    args = sys.argv[1:]
    themes = sorted(p.name for p in THEMES.iterdir() if (p / "colors.toml").exists())
    columns, scale = 3, 1
    out_dir = Path(__file__).resolve().parent / "out" / "themes"
    files = []
    while args:
        arg = args.pop(0)
        if arg == "--themes":
            themes = args.pop(0).split(",")
            scale = 2
        elif arg == "--columns":
            columns = int(args.pop(0))
        elif arg == "--scale":
            scale = int(args.pop(0))
        elif arg == "--out":
            out_dir = Path(args.pop(0)).resolve()
        else:
            files.append(Path(arg).resolve())
    if not files:
        sys.exit(__doc__.strip())
    # light themes first, so the risky ones are at the top of the sheet
    themes.sort(key=lambda t: (mode_of(t) != "light", t))
    for path in files:
        sheet(path, themes, columns, out_dir, scale)


if __name__ == "__main__":
    main()
