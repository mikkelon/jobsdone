# Wireframes

Terminal wireframes for the app, drawn on character grids at real sizes:
120×36 (an Omarchy floating terminal, the usual way the app is opened),
160×48 (a full-width tile) and 80×44 (a half-width tile).

Open `index.html` for the screen map and user flows, then step through the
screens. Each screen has a notes column with numbered markers. The theme
selector swaps in any installed Omarchy theme, mapped the way a terminal
would show it. Each screen is also written as a plain `.txt` next to its
`.html`.

The screens are generated, not hand-drawn. Edit `gen.py` and run:

    ./gen.py

`themes.js` holds the colour tokens of the installed themes. Regenerate it
after installing new themes:

    ./gen-themes.py

Screenshots in `screenshots/` were taken with Playwright at 1600×1000.
