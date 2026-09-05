# Acceptance testing

Tools for driving the built program the way a person would, in a terminal
of a fixed size, and recording what it showed. `MATRIX.md` is the list of
things to check; this file is how to check them.

## The driver

`uat/tui` runs the app in a detached tmux session of a chosen size, with
a scratch database under `uat/out/data`, never the real one. Every
subcommand prints what it did, and `screen` prints the whole screen as
text, so a transcript of a test session reads on its own.

    uat/tui start --fresh --size 120x36    build, wipe the scratch data, launch
    uat/tui keys a                         one tmux key name per argument
    uat/tui type "Call the accountant"     a literal string
    uat/tui keys Enter Escape              several keys, in order
    uat/tui screen added-a-task            capture: prints text, writes uat/out/added-a-task.{txt,ansi,png}
    uat/tui mouse click 6 8                SGR mouse events at a 1-based column and row
    uat/tui mouse wheel-down 30 10
    uat/tui mouse press 6 8; uat/tui mouse drag 6 10; uat/tui mouse release 6 10
    uat/tui resize 80x44                   the app follows the terminal
    uat/tui restart                        quit with q and launch again on the same data
    uat/tui sql "select id, title, day, closed_at from tasks"
    uat/tui log                            the app's own log, where a failed commit is written
    uat/tui stop

Key names are tmux's: `Enter`, `Escape`, `Space`, `Tab`, `BSpace`, `Up`,
`Down`, `Left`, `Right`, `C-c`, `M-t` for alt-t, `M-1` for alt-1, and any
single character as itself. A capital letter is a different key from the
lowercase one, so `J` reorders and `j` moves.

The screen print is the truth about layout: it is what the terminal
holds, column for column. The PNG is the same screen rendered in the
current Omarchy theme (`~/.local/state/omarchy/current/theme/foot.ini`),
for judging colour, bold and dim. `uat/render.py SCREEN.ansi OUT.png
120x36 --theme catppuccin-latte` renders a capture again in another
installed theme, for the light-mode check.

`uat/themes.py SCREEN.ansi...` puts one capture in every installed theme
on a single sheet, light themes first, under `uat/out/themes/`; with
`--themes a,b` it draws only those, twice the size, for a close look.
`--stock` draws the default palettes of common terminals instead, for how
the app looks where nothing writes a theme into the terminal.

Row 1 of the screen is the blank margin, row 2 the status line, row 3 its
rule, row 4 the pane titles, row 5 their rule, so the first list row is
row 6; the hint bar is the second-to-last row. Column 1 is the margin.

## Standing on another day

The app reads the real clock and the working day rolls at 05:00. There
is no override, so a flow that needs "yesterday" is set up by writing the
scratch database directly between a `stop` and a `start`:

    uat/tui stop
    uat/tui sql "update tasks set day = date('now', '-1 day') where id = 3"
    uat/tui sql "insert into placements (task_id, day, placed_at, from_place)
                 values (3, date('now', '-1 day'), datetime('now', '-1 day'), 'backlog')"
    uat/tui start

Dates are `YYYY-MM-DD`; instants are RFC 3339 with a zone, the way the
app writes them (`select created_at from tasks limit 1` shows the shape).
The columns are in `migrations/0001_initial.sql` and their meaning in
`docs/DOMAIN.md` section 17. The two `meta` keys that gate the review
are `review_on` and `review_before`; deleting them makes the next launch
count as the first of the day. A schedule's `generated_through` is the
last date copies were made for; moving it back makes the next launch
create the copies since.

Say in the report which tests stood on a written-in date, since a rule
that only fails at a real 05:00 rollover is not one this can catch.

## What to look at

- `docs/PRODUCT.md` is what the program must do. `docs/DESIGN.md` is how
  it must look and feel, section 3 for colour and section 4 for keys.
- `wireframes/*.txt` are the screens at 120×36, 160×48 and 80×44, drawn
  to the column. `wireframes/index.html` names the user flows A to G.
- A difference from a wireframe is a finding unless a doc explains it.
  A doc that contradicts another doc is a finding too.

## The clean machine

`uat/vm` is a clean Omarchy in QEMU, for the questions this machine
cannot answer: does the install work from nothing, and does the keybind
open the app floating and centred on a desktop with no personal config.
Run it with no arguments for the command list.

A person installs Omarchy once, from the ISO at omarchy.org, and seals
the result:

    uat/vm install --user tester     the installer opens in a window; pick the US keyboard layout
    curl -fsS http://10.0.2.2:8123/prepare.sh | bash     in a guest terminal after the first boot
    uat/vm seal                      powers off and freezes the base image

The base image is then read-only. Every `fresh` boots a throwaway
overlay on top of it and returns when the desktop is up, about twenty
seconds; `save NAME` keeps an overlay worth returning to, such as one
with rustup installed, and `restore NAME` boots it again. Images live
under `~/.local/share/jobsdone-uat/vm`, with the guest user and disk
passphrase in `config` there, outside the repository.

The guest differs from a truly clean machine in what a test machine
needs: passwordless sudo, autologin kept, idle lock off, and ssh let
through ufw. It has no Rust toolchain.

An agent drives it three ways: `ssh` for anything inside, with
`hyprctl` and `grim` working because the session's environment is set;
`keys` and `type` for the virtual keyboard, so a keybind is pressed
exactly as a person would press it; `shot` for the whole screen. With
`--show`, QEMU opens a window instead and the guest's resolution
follows that window's size.
