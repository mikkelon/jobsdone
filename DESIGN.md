# Design

Design principles for the task manager described in PRODUCT.md, and the
decisions that follow from them. Wireframes live in `wireframes/`
(open `wireframes/index.html`); they are grey and structural, and can be
switched to any installed Omarchy theme to check that the structure
survives theming.

## 1. It is a peer of btop and lazygit, not a web app in a window

The app runs as a Hyprland tile on Omarchy, next to a terminal, an editor
and a browser. It should look like it belongs to the first two.

- Monospace everywhere, at the shell's base size (12 to 13 px). No second
  typeface, no display sizes. Hierarchy comes from weight, case, and
  position, not from size.
- Square corners. One-pixel lines at 25 to 40 percent foreground alpha for
  structure, the two-pixel accent border for the window itself (Hyprland
  draws it; the app must not draw a second one). No shadows, no gradients,
  no rounded cards.
- Fills are foreground at low alpha, the way `shell.toml` does it: 4 percent
  for surfaces, 8 percent for the cursor row, 18 percent for selected.
- Density over decoration. One line per task, 26 px rows, no icons other
  than a small set of text glyphs (☐ ✓ ↻ ◷ ▌).
- No window chrome of its own: no title bar, no toolbar, no menu bar. The
  top strip is a status line and the bottom strip is a hint bar.

## 2. Colour is borrowed from the theme, never owned

The app has no palette. It consumes the Omarchy theme tokens from
`colors.toml`, so it follows theme changes live, in dark and light mode,
without a setting of its own.

Only the tokens below are used, and each has one meaning. In the grey
wireframes all of them collapse to the foreground, which is the test: the
structure must read without colour.

| Token               | Meaning in the app                                              |
|---------------------|-----------------------------------------------------------------|
| `background`        | Window and card background                                      |
| `foreground`        | Text, glyphs, and every derived line and fill (via alpha)       |
| `muted`             | Secondary text: metadata, hints, group labels, done rows        |
| `accent`            | The cursor row, the focused pane's header, active tab, primary action, selected item in cards. One accent thing at a time. |
| `red`               | The review pile count when non-zero; overdue due-by chips       |
| `yellow`            | Due-by chips that are not yet overdue                           |
| `blue`              | Remind-on chips                                                 |
| `magenta`           | The waiting flag                                                |
| `green`             | Closed task checkmark                                           |
| `lighter_background`| Input fields and the note surface                               |

Rules:

- Semantic colour is always paired with a glyph or a word. A due date is a
  chip that says "due 12 Sep"; the yellow is reinforcement, not the signal.
- Accent is reserved for "where the keyboard is" and "the one thing to
  press". It never marks task state.
- Fills and lines derive from `foreground` with alpha, so they are correct
  on both dark and light themes without separate values.
- Bright variants and `orange`, `cyan`, `brown` are not used. Fewer tokens,
  fewer ways a theme can look wrong.

## 3. Keyboard first, mouse allowed

Every action is a single key on the cursor row, shown in the hint bar at the
bottom for the focused pane. The mouse does the obvious things (click a row,
drag to reorder, click a checkbox) but nothing is only reachable by mouse,
and nothing is only reachable by keyboard.

Conventions, so the map is guessable:

- `j` `k` move the cursor; `h` `l` and `tab` move between panes (or tabs
  when narrow). `J` `K` reorder.
- Lowercase letters act on the cursor row: `space` done, `f` focus, `a` add,
  `e` edit, `x` delete, `t` to today, `b` to backlog, `m` move to a day,
  `d` due by, `r` remind on, `w` waiting. `R` opens the repeat schedule.
- Punctuation navigates: `[` `]` previous and next day, `.` today, `g` go
  to a date, `/` search, `:` command palette, `?` help, `n` notes drawer.
- `Enter` confirms, `Escape` backs out one level, `u` undoes.
- The command palette lists every action with its key next to it, so it
  teaches the map and then stops being needed. Help is the whole map on one
  card. There are no hidden keys.

## 4. The day is home; the review is a mode, not a place

The main screen is today's plan beside the backlog, because the core loop is
"look at the backlog, pull into today". Both lists are visible at once.

The morning review takes over the whole window on the first open of a work
day, as two steps: the pile (unfinished tasks from past days) and surfaced
tasks (due, reminded, and new recurring copies). It is a ritual, so it gets
the whole window. It is also optional: `Escape` leaves it, the pile stays,
and an alert count in the top strip keeps it honest. An empty step is
skipped; when both are empty the app opens straight to today. The review
is never an empty ceremony.

## 5. Places, not pages

There are three places a task can be and the window shows them as panes:
today (or another day), the backlog, and, when opened, the notes drawer.
Everything else is an overlay on top of a place: the date card, the repeat
card, the day picker, search, the palette, help. Overlays are the Omarchy
menu shape: a centred card with the accent border, a scrim behind it, the
selected row in accent.

History is not a separate screen. It is the day pane stepped backwards with
`[`, while the backlog pane becomes a list of days with their done counts.
Future days work the same way forwards.

Below a width threshold (roughly a half-screen tile) the panes collapse to
one and a tab strip takes over. Same keys, same content, less metadata per
row. Nothing else changes.

## 6. Nothing moves on its own

The app never reorders, carries over, expires, or tidies.

- Task order on a day is manual and remembered. Closed tasks drop to a Done
  group at the bottom, in the order they were closed.
- Unfinished tasks stay on their day, however old, until the person acts.
  The review shows their age relatively ("3 weeks ago") so the cost of
  ignoring them is visible.
- Due and remind dates surface a task; they never move it. Pulling onto
  today is always an explicit `t`.
- Recurring schedules are the one thing that creates tasks by themselves,
  so the surfaced step lists the copies it made, once, for information.
- Notes stay until deleted.

## 7. Undo instead of confirm

No action asks "are you sure". Delete, close, move, and review decisions all
apply immediately and offer `u` in a toast. Editing is always in place; the
only forms are the three small cards (date, repeat, move).

The one exception is deliberately a question, not a confirmation: editing
the title of a recurring copy asks whether the change is for this copy or
this and future copies, because PRODUCT.md gives both answers meaning.

## 8. Empty states name the key that fills them

An empty list says what it is for and the one or two keys that put
something in it. No illustrations, no encouragement. The empty search
offers to add the typed text as a task. A review with nothing in it is not
shown at all.

## 9. Nothing to configure

Theme, font, base size, and window borders come from Omarchy. The app has
no settings screen and no preferences file. If something needs tuning, it
is tuned in the theme, not in the app.

## Screen inventory

| Screen                      | Wireframe                       |
|-----------------------------|---------------------------------|
| Morning review: the pile    | `wireframes/01-review.html`     |
| Morning review: surfaced    | `wireframes/02-surfaced.html`   |
| Today + backlog             | `wireframes/03-today.html`      |
| Narrow tile (tabs)          | `wireframes/04-narrow.html`     |
| Task states                 | `wireframes/05-task-states.html`|
| Due, remind, waiting        | `wireframes/06-dates.html`      |
| Repeat schedule             | `wireframes/07-repeat.html`     |
| History (past days)         | `wireframes/08-history.html`    |
| Search                      | `wireframes/09-search.html`     |
| Scratchpad                  | `wireframes/10-scratchpad.html` |
| Command palette and help    | `wireframes/11-palette-help.html`|
| Empty states                | `wireframes/12-empty.html`      |

User flows A to G (morning, plan today, work through the day, park with a
date, wait on someone, recurring task, look back, half-width tile) are drawn
on `wireframes/index.html`. Screenshots of every screen, plus the main
screen rendered in three Omarchy themes, are in `wireframes/screenshots/`.
