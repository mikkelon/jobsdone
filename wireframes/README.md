# Wireframes

Open `index.html` in a browser. Each screen has a notes column with numbered
callouts. The theme selector at the top swaps in any installed Omarchy theme;
the tokens come from `/usr/share/omarchy/themes/*/colors.toml` via
`themes.js`. Regenerate it after installing new themes:

    ./gen-themes.py

Screenshots in `screenshots/` were taken with Playwright at 1600×1000.
