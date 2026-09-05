#!/usr/bin/env python3
"""Render a captured screen in every installed Omarchy theme on one sheet,
light and dark side by side, for the colour check.

    themes.py SCREEN.ansi... [--themes a,b,c | --stock] [--columns N] [--scale N] [--out DIR]

One PNG per screen lands in DIR (default uat/out/themes), named after the
capture. Each cell is the screen the way render.py draws it, captioned
with the theme's name and mode. --themes limits the sheet to a few, at
--scale 2 for a close look; without it every theme under
/usr/share/omarchy/themes is on the sheet at scale 1. --stock draws the
default palettes of common terminals instead, approximated from their
sources, for how the app looks where no Omarchy theme is written into
the terminal.
"""
import html, os, re, subprocess, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import render  # noqa: E402

THEMES = Path("/usr/share/omarchy/themes")

# Default palettes of terminals as shipped, sixteen colours then foreground
# and background. Approximations from memory of each project's defaults,
# close enough to judge contrast, not to match a screenshot.
STOCK = {
    "xterm (default, black on white)": (
        "000000 cd0000 00cd00 cdcd00 0000ee cd00cd 00cdcd e5e5e5 7f7f7f ff0000 00ff00 ffff00 5c5cff ff00ff 00ffff ffffff",
        "000000", "ffffff"),
    "Linux console (VGA)": (
        "000000 aa0000 00aa00 aa5500 0000aa aa00aa 00aaaa aaaaaa 555555 ff5555 55ff55 ffff55 5555ff ff55ff 55ffff ffffff",
        "aaaaaa", "000000"),
    "alacritty (default)": (
        "181818 ac4242 90a959 f4bf75 6a9fb5 aa759f 75b5aa d8d8d8 6b6b6b c55555 aac474 feca88 82b8c8 c28cb8 93d3c3 f8f8f8",
        "d8d8d8", "181818"),
    "kitty (default)": (
        "000000 cc0403 19cb00 cecb00 0d73cc cb1ed1 0dcdcd dddddd 767676 f2201f 23fd00 fffd00 1a8fff fd28ff 14ffff ffffff",
        "dddddd", "000000"),
    "foot (default)": (
        "242424 f62b5a 47b413 e3c401 24acd4 f2affd 13c299 e6e6e6 616161 ff4d51 35d450 e9e836 5dc5f8 feabf2 24dfc4 ffffff",
        "ffffff", "242424"),
    "GNOME Terminal (Tango, dark)": (
        "2e3436 cc0000 4e9a06 c4a000 3465a4 75507b 06989a d3d7cf 555753 ef2929 8ae234 fce94f 729fcf ad7fa8 34e2e2 eeeeec",
        "d3d7cf", "2e3436"),
    "Windows Terminal (Campbell)": (
        "0c0c0c c50f1f 13a10e c19c00 0037da 881798 3a96dd cccccc 767676 e74856 16c60c f9f1a5 3b78ff b4009e 61d6d6 f2f2f2",
        "cccccc", "0c0c0c"),
    "macOS Terminal (Basic, light)": (
        "000000 990000 00a600 999900 0000b2 b200b2 00a6b2 bfbfbf 666666 e50000 00d900 e5e500 0000ff e500e5 00e5e5 e5e5e5",
        "000000", "ffffff"),
}


def stock_palette(name):
    slots, fg, bg = STOCK[name]
    return ["#" + c for c in slots.split()], "#" + fg, "#" + bg


def mode_of(theme):
    if theme in STOCK:
        bg = STOCK[theme][2]
        return "light" if int(bg[:2], 16) + int(bg[2:4], 16) + int(bg[4:], 16) > 3 * 128 else "dark"
    text = (THEMES / theme / "colors.toml").read_text(errors="replace")
    return "light" if re.search(r'^mode\s*=\s*"light"', text, re.M) else "dark"


def cell(theme, ansi, cols, rows):
    slots, fg, bg = stock_palette(theme) if theme in STOCK else render.palette(theme)
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
        elif arg == "--stock":
            themes = list(STOCK)
            scale = 2
            columns = 2
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
