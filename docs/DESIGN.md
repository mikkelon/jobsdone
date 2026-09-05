# Design

Design principles for the task manager described in PRODUCT.md, and the
decisions that follow from them. The app is a terminal program (TUI) that
runs inside an Omarchy terminal. Wireframes live in `wireframes/` (open
`wireframes/index.html`); they are character grids at real terminal sizes,
grey by default, and can be switched to any installed Omarchy theme.

## 1. Opened for a moment, mostly

The usual way in is a keybind that opens the app in a floating terminal,
centred, around 120 columns by 36 rows. It is glanced at, changed, and
closed, many times a day. It can also be left running in a tile, full or
half width. Both must feel right, and the floating case comes first.

- Start is instant. There is no splash, no loading state, no sync.
- The day changes at 05:00, not midnight, so a late night belongs to the
  day it started in. Until then the app still says "Today" about the
  date that was on the calendar when the evening began.
- Every change is written the moment it is made. Quitting with `q` or
  closing the window never asks anything; reopening lands where you were.
- The morning review runs once per day, on the first open of a work day
  that has something to review. Later opens go straight to Today. Closing
  the window mid-review leaves the pile intact; the home screen shows the
  count until it is dealt with.
- The layout targets 120×36. A full-width tile shows the same layout with
  more rows. Under 100 columns the two panes collapse to tabs.
- Hyprland does the windowing: a window rule on the app's class floats and
  sizes it. The app never positions or resizes itself.
- Several windows may be open at once, a tile left running and a floating
  one opened elsewhere. All of them show the same data within a moment.

## 2. It is a peer of btop and lazygit

The app lives next to a terminal, an editor and a browser. It should look
like it belongs to the first two.

- One cell per row. Titles, chips and hints share the line. Density over
  decoration; 24 tasks fit in the floating window.
- A one-cell margin all round: a blank row above the status line and below
  the hint bar, a blank column at the left and right edges. Text never
  touches the window border.
- Structure is drawn with box-drawing lines in the dim colour: a divider
  between panes, a horizontal rule under the status line and above the
  hint bar, a box around a popup. Hyprland draws the window border; the
  app does not draw one.
- Hierarchy comes from bold, dim, and position, never from size. Group
  labels are dim capitals with a rule. Focus items are bold. Done and
  waiting rows are dim.
- Popups are lazygit-style: a centred box drawn over the panes, accent
  border, no dimming of what is behind.
- No window chrome of its own: the top line is a status line, the bottom
  line a hint bar.

## 3. Colour comes from the terminal, which Omarchy themes

The app uses only the terminal's default foreground and background and the
ANSI colours. Omarchy already writes the current theme into every terminal
emulator, so the app follows theme changes live, in dark and light mode,
with no code and no setting of its own.

| Terminal colour        | Meaning in the app                                                  |
|------------------------|---------------------------------------------------------------------|
| default fg / bg        | text, structure                                                     |
| bold                   | focus items, pane titles                                            |
| dim (or bright black)  | secondary text: metadata, hints, group labels, done and waiting rows|
| selection background   | the cursor row, so chips on it keep their colour                    |
| blue (bold)            | accent: focused pane title, popup border, selected item, primary action, and every key name in the hint bar, help overlay and popup footers, so a key never looks like its description |
| red                    | the review count when non-zero; overdue due-by chips; "on the pile" |
| yellow                 | due-by chips not yet overdue                                        |
| cyan                   | remind-on chips                                                     |
| magenta                | the waiting flag                                                    |
| green                  | closed task checkbox                                                |

Rules:

- Colour is always paired with text. A due date is `[due 12 Sep]`; the
  yellow is reinforcement. The grey wireframes are the test: everything
  must read without colour.
- Blue stands in for Omarchy's `accent` token, which nearly every theme
  defines as its blue. Reminders therefore use cyan, not blue.
- Accent marks "where the keyboard is" and "the one thing to press". It
  never marks task state.
- The cursor row is reverse video rather than the terminal's selection
  colour, because a terminal does not tell a program running inside it what
  its selection colour is. Reverse video is the ANSI equivalent: the row
  takes the foreground as its background, and a coloured chip on it stays
  that colour, as a block instead of as text.
- Nothing else. No 256-colour or truecolour values, no bright variants
  except bright black for dim where the terminal has no dim.

## 4. Keyboard first, mouse allowed

Every action is one key on the cursor row, listed in the hint bar for the
focused pane. The mouse does the obvious things (click a row, wheel to
scroll, drag to reorder) but nothing is only reachable by mouse.

Conventions, so the map is guessable:

- `j` `k` move; `h` `l` and `tab` switch pane (or tab when narrow); `J` `K`
  reorder.
- Lowercase acts on the cursor row: `space` done, `f` focus, `a` add,
  `e` edit, `x` delete, `t` to today, `b` to backlog, `m` move to a day,
  `d` due by, `r` remind on, `w` waiting. `R` opens the repeat schedule.
- Punctuation navigates: `[` `]` previous and next day, `.` today, `g` go
  to a date, `/` search, `:` command palette, `?` help, `n` notes page,
  `q` quit.
- `Enter` confirms, `Escape` backs out one level, `u` undoes.
- After a task is closed, moved or deleted the cursor steps to the next
  row of the group it left, so a list is worked down without moving the
  cursor by hand. Everything else leaves the cursor on the task it acted
  on, which is how a reordered row is followed up or down the list.
- A pane with more rows than the window has lines scrolls to keep the
  cursor row on screen and keeps no scroll position of its own. The wheel
  moves the cursor, so the view always follows it and there is never a
  cursor somewhere off screen.
- Where a text field has focus (search, the palette, the date card, an
  open note, a title being edited), every letter and digit types. Only
  `Enter`, `Escape`, `↑`/`↓` and `Tab` keep their meaning there. The few
  extra actions a text field needs are on Alt plus a key (`alt-t` re-add
  from search, `alt-1` to `alt-4` quick dates). `Tab` moves focus to the
  next control, where single keys work again.
- One collision, and its resolution: in the review `k` is "keep", so the
  cursor moves there with `j` and the arrow keys and `k` does not move it.
  Everywhere else `j` and `k` move.
- The command palette lists every action with its direct key beside it.
  Inside the palette you filter and press Enter; the key shown is for next
  time, outside the palette, so it teaches the map and then stops being
  needed. Help is the whole map on one overlay. There are no hidden keys.

## 5. The day is home; the review is a mode, not a place

The main screen is today's plan beside the backlog, because the core loop is
"look at the backlog, pull into today". Both lists are visible at once.

The review takes the whole window in two steps: the pile (unfinished tasks
from past days) and surfaced tasks (due, reminded, and new recurring
copies). An empty step is skipped; when both are empty the app opens
straight to Today. The review is never an empty ceremony, and it is never
shown twice in a day.

## 6. Two pages, and popups over them

The window shows one of two pages. The home page is today (or another day)
beside the backlog. The notes page, reached with `n`, is the list of notes
beside the open note. Everything else is a popup over a page: the date
card, the repeat card, the day picker, search, the palette, help.

History is not a separate screen. It is the day pane stepped backwards with
`[`, while the backlog pane becomes a list of days with their done counts.
Future days work the same way forwards.

A past day shows every task that was planned for it, in three states: closed
(dim, with the time, in the Done group), still open (marked "on the pile",
in red, in Focus or Plan), and moved away. Moved tasks sit in their own
group at the bottom, under Done, at normal weight: an arrow in the box
(`[→]`) says the task is not here any more, and the right side says where it
is now ("to today", "to Wed 3 Sep", "to backlog"). Dim is reserved for
finished. A moved row is a pointer, not a copy; Enter on it jumps there.
Moving a task never rewrites what was planned.

A past day is drawn exactly as it was while it was today: the same four
groups in the same order, Focus, Plan, Done, Moved, and a group with
nothing in it is not drawn at all. That is why today rarely shows a Moved
group and a finished past day rarely shows a Focus one.

## 7. Nothing moves on its own

The app never reorders, carries over, expires, or tidies.

- Task order on a day is manual and remembered. Closed tasks drop to a Done
  group at the bottom, in the order they were closed; one closed out of
  Focus keeps a "was focus" marker there, because closing changes where the
  row sits, not what the task was. A task moved off a day leaves its
  pointer in that day's Moved group the moment it is moved.
- A day is laid out the same way whether or not it is today. Nothing is
  re-grouped behind the person's back, least of all overnight.
- Unfinished tasks stay on their day, however old, until the person acts.
  The review shows their age relatively so the cost of ignoring them is
  visible.
- Due and remind dates surface a task; they never move it. Pulling onto
  today is always an explicit `t`.
- Recurring schedules are the one thing that creates tasks by themselves.
  On launch the app creates the copies for every scheduled date since the
  last launch, and the surfaced step lists today's, once, for information.
  A window left open past 05:00 has reached a new day without a launch, so
  it does the same then; a day is never short of its copies because a
  window happened to be open.
- Notes stay until deleted.

## 8. Undo instead of confirm

No action asks "are you sure". Delete, close, move, and review decisions
apply immediately and offer `u` in the hint bar until the next key. Editing
is always in place; the only forms are the three small cards (date, repeat,
move).

A field on a row is a mode like any other, so its keys are in the hint
bar rather than beside it: while a title is being typed the bar says what
Enter does there, "add & keep typing" when adding and "save" when
renaming. What just happened takes the same line over until the next key,
which is where the offer of `u` is.

The one deliberate question: editing the title of a recurring copy asks
whether the change is for this copy or this and future copies, because
PRODUCT.md gives both answers meaning.

## 9. Scratchpad is text, and only text

Notes get a page of their own: the list on the left, the open note taking
the rest of the width. Opening one gives a plain multi-line text area with
nothing else on it. There is no drawing, no formatting, no pop-out window:
the app itself is usually a floating window opened for a moment, so a note
is already on top whenever it is needed.

## 10. Empty states name the key that fills them

An empty list says what it is for and the one or two keys that put
something in it. No illustrations, no encouragement. The empty search
offers to add the typed text as a task. A review with nothing in it is not
shown at all.

## 11. Nothing to configure

Colours, font, and size come from the terminal, which Omarchy themes.
Window placement comes from Hyprland. The app has no settings screen and no
preferences file.

## Screen inventory

| Screen                      | Wireframe                         | Size    |
|-----------------------------|-----------------------------------|---------|
| Morning review: the pile    | `wireframes/01-review.html`       | 120×36  |
| Morning review: surfaced    | `wireframes/02-surfaced.html`     | 120×36  |
| Today + backlog             | `wireframes/03-today.html`        | 120×36 and 160×48 |
| Half-width tile (tabs)      | `wireframes/04-narrow.html`       | 80×44   |
| Task states                 | `wireframes/05-task-states.html`  | panels  |
| Due, remind, waiting        | `wireframes/06-dates.html`        | 120×36  |
| Repeat schedule             | `wireframes/07-repeat.html`       | 120×36  |
| History (past days)         | `wireframes/08-history.html`      | 120×36  |
| Search                      | `wireframes/09-search.html`       | 120×36  |
| Scratchpad (notes page)     | `wireframes/10-scratchpad.html`   | 120×36  |
| Command palette and help    | `wireframes/11-palette-help.html` | panels  |
| Empty states                | `wireframes/12-empty.html`        | panels  |

User flows A to G (the morning, plan today, work through the day, park with
a date, wait on someone, recurring task, look back, floating or tiled) are
drawn on `wireframes/index.html`. Every screen is also saved as plain text
next to its HTML, and screenshots, including the home screen in three
Omarchy themes, are in `wireframes/screenshots/`.
