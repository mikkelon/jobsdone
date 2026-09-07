# Acceptance matrix

One row per thing a person would notice. Each has an id, what to do, and
what must be true afterwards. Do them in order; later rows assume the
state earlier rows left. Check the screen with `uat/tui screen <id>`
after every step that changes it, and the database with `uat/tui sql`
whenever a row says "stored". Take the wireframes as the expected drawing
wherever one exists, and `docs/` as the expected behaviour: a difference
from a doc is a finding.

Verdicts: PASS, or FAIL with the screen and the steps.

## Phase 9: settings

The settings page, the thirteen settings, and the install script's flags.
`uat/tui` drives the app; the settings are read back with
`uat/tui sql "select key, value from settings"`.

| Id   | Steps | Expected |
|------|-------|----------|
| 9.01 | `start --fresh --size 120x36`, `keys ,`, `screen` | The settings page: the status line `Settings kept in the database, beside the tasks` with `, or esc back` on the right, the pane header `Settings` with `colour and font come from the terminal`, the list in five groups DAY, WORK DAYS, REVIEW, WINDOW, LOOKS with every row `label ..... value`, the description pane on the right for the cursor row ending in a `Default:` line, the hint bar `SETTINGS  h/l adjust  space ⏎ change  esc , back`. Compare to `wireframes/13-settings.txt` column for column. |
| 9.02 | `keys ,`, `screen` | Back on the home page, exactly as it was. `keys n , ,` returns to the notes page. `esc` on the settings page goes back too. |
| 9.03 | `keys :`, `type "sett"`, `screen`, `keys Enter` | The palette lists "settings" with `,` beside it in the app section; Enter opens the page. `?` on the page shows its keys. |
| 9.04 | On "Day starts at": `keys l l l`, `screen`, `sql` | The value steps 5:00 → 8:00; stored `day_starts_at` = 8. `h` steps back. At 0 and 23 the value stops. |
| 9.05 | Day start test: set `day_starts_at` to 23 with `sql` at a time of day before 23:00, `restart`, `screen` | `Today` in the header is yesterday's date: the working day has not begun. Set it back to 5 and restart: today is today again. |
| 9.06 | On "Week starts on": `keys space`, `screen` | Cycles monday → sunday. `keys [` on the home page with a history: the day list's THIS WEEK rule starts on Sunday; the date card calendar (`d`, Tab) starts its columns with Su. |
| 9.07 | WORK DAYS rows: toggle Sat on, Mon off, `screen`, `sql` | Each row flips `on`/`off`; stored `work_days` = `tue,wed,thu,fri,sat`. A task with `R` "every work day" previews Saturday and skips Monday; the move card's "next work day" agrees. |
| 9.08 | Toggle every work day off | The last one is refused: the hint bar says at least one day must be a work day, and the row stays on. |
| 9.09 | "Open the review on launch" off, put an unfinished task on yesterday with `sql`, `restart`, `screen` | The home page opens, not the review; the status line counts `● 1 on the pile` in red with `M` in accent beside it; `M` opens the review. On again, clear the gate (`delete from meta where key like 'review_%'`, since the review opens itself once a working day, DOMAIN.md 13) and `restart`: the review opens. |
| 9.10 | On "Surface due tasks early": `keys Enter`, type `3`, `keys Enter`, `screen`; a backlog task due in 2 days, `restart` | The value reads `3 days before`; at 0 it reads `on its day`. The review's surfaced step lists the task under due, chip yellow, not overdue. Back at 0 it does not surface until its date. |
| 9.11 | On "Catch up recurring tasks": `keys Enter`, type `7`, `keys Enter`, `screen`; a schedule with `generated_through` 30 days ago (via `sql`), `restart`, `sql` | The row is labelled `Catch up recurring tasks` and reads `the last 7 days`; at 0 it reads `every missed day`. Copies exist only for the last 7 days; `generated_through` is today; the pile holds only those. At 0 every missed day's copy is made. |
| 9.12 | "Hide pile tasks older than" to 30, an open task on a day 40 days ago and one 10 days ago, `restart`, `screen` | The review count is 1 and the pile shows only the newer; `[` to the old day shows the task marked "still open", not "on the pile". At 0 (never) both are in the pile. |
| 9.13 | "Floating window" `space`, `screen` | The value flips. With no Hyprland and no `hypr` directory under `XDG_CONFIG_HOME` the hint bar says Hyprland is not here and the setting is kept; with the directory there but no Hyprland running, the rule is written into it and the hint bar says it applies the next time the app opens. On Hyprland (`uat/vm`): `~/.config/hypr/bindings.lua` has the window block with `o.window(...)` when on and bare markers when off, and `hyprctl clients` shows the next window floating or tiled. |
| 9.14 | On "Window size": `screen`, `keys l`, `screen`, hold `l`, `screen`, `sql`; then `keys Enter`, type `900x700`, `keys Enter`, `screen` | The value reads `870x650 · 120 by 36 cells`; `l` steps to `1010x755 · 140 by 42 cells` and `h` back, five sizes in all, stopping at `730x550 · 100 by 30 cells` and `1290x960 · 180 by 54 cells`. Nothing is written to `~/.config/hypr/bindings.lua` while the key repeats: the rule is rewritten once, a quarter of a second after the last press, at the size the row stopped on. A typed size reads `900x700 · custom` and is stored as `900x700`; `l` from there steps to `1010x755` and `h` to `870x650`. A value like `abc` is refused in the hint bar; below 200 is clamped. On Hyprland (`uat/vm`) with the window floating: `keys l`, wait a second, `hyprctl clients -j` shows the app's own window resized to the preset the row stopped on and centred. With "Floating window" off, `l` moves the value and resizes nothing. |
| 9.15 | "Mouse" off: `mouse click` on a home row | Nothing happens: the terminal has the mouse, and an event the driver writes anyway is dropped. On: click selects. Takes effect without a restart. |
| 9.16 | "Hint bar messages stand for" to 0; close a task; wait 6 s; `screen` | The message still stands; the next key clears it. At 4, it is gone after 5 s. |
| 9.17 | "Date order": cycle locale → day first → month first, `screen` on home | Headers and chips read `Fri Sep 5` under month first, `Fri 5 Sep` under day first. Under locale, `LANG=en_US.UTF-8 uat/tui start` gives month first and `LANG=da_DK.UTF-8` day first. |
| 9.18 | "Confirm before delete" on; `x` on a task, `screen`, `keys Escape`; `x`, `keys Enter` | A card asks about the quoted title with `⏎ delete  esc keep`; Escape keeps it; Enter deletes and offers `u`. The same on a note and on a pile row in the review. Off, `x` deletes at once. |
| 9.19 | Two instances (`UAT_SESSION=second uat/tui start --data uat/out/data`), change a setting in one, `screen` in the other after a second | The other window shows the new value on its settings page and behaves by it. |
| 9.20 | `q` from the settings page, then `restart`; a value typed into an open field and `q` | `q` quits without asking and the app reopens on the home page; `,` shows every stored value. In an open field `q` types, as every letter does (DESIGN.md 4); Escape leaves the stored value as it was. |
| 9.21 | Narrow: `resize 80x44`, `keys ,`, `screen` | The list alone, no description pane; the hint bar in its narrow layout. |
| 9.22 | Colour, on the PNGs, and `uat/render.py ... --theme catppuccin-latte` | Values on the cursor row in accent, group labels dim, nothing said by colour alone. |
| 9.23 | `jobsdone --help`, `jobsdone --version`, `jobsdone bogus` | Usage on stdout and exit 0; the version; usage on stderr and exit 2. |
| 9.24 | `JOBSDONE_DATA_DIR=uat/out/data target/debug/jobsdone desktop --tiled --size 800x600`, `sql` | Stored `floating_window` = false, `window_size` = `800x600`; one line on stdout; off Hyprland the line says the settings are kept for when it is there, and the exit is 0 (STACK.md 9). |
| 9.25 | `scripts/install --keybind "SUPER + T" --tiled` with a fake `HOME`/`XDG_CONFIG_HOME` and a scratch `bindings.lua` holding the old `-- jobsdone: begin` block | The old block is gone; a keybind block binds `SUPER + T`; the window block is bare markers; the modmask check reports a taken `SUPER + T` correctly when `hyprctl` is faked to say so. `scripts/uninstall` removes both blocks. On the clean VM, which has no Rust: `make install` refuses with the sentence that names the pacman command; after that command, `make install` (answer the keybind prompt, or run `scripts/install --keybind`), `uat/vm keys super-shift-j`, `hyprctl clients -j` shows class `org.omarchy.jobsdone` floating at 870x650. |

## Notes: copy entire note

| Id | Steps | Expected |
|----|-------|----------|
| N.01 | Open notes, create a multiline note with blank lines, indentation and Unicode; press `M-y`; read the clipboard with `wl-paste --no-newline` (Wayland) | Exact full note text, including trailing newlines; "Note copied" feedback; editor remains active. |
| N.02 | Move the caret into the note, press `M-y`, then type `y` | Copy keeps caret and scroll position; plain `y` inserts at the caret. |
| N.03 | `Escape`, `y`; repeat with `M-y`; open `:` and select "copy note" | Each copies the selected note and leaves the list active. |
| N.04 | Repeat at 80×24 with a note longer than the viewport | Copy hints fit; the clipboard includes offscreen text. |
| N.05 | Delete all scratch notes, then `y` | "There is no note here yet."; clipboard unchanged. |
| N.06 | Run with the clipboard helper unavailable and copy a note | Failure feedback; app stays usable and the note stays intact. |
