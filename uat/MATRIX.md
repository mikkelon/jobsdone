# Acceptance matrix

One row per thing a person would notice. Each has an id, the phase it
belongs to, what to do, and what must be true afterwards. Do them in
order within a phase; later rows assume the state earlier rows left.
Check the screen with `uat/tui screen <id>` after every step that changes
it, and the database with `uat/tui sql` whenever a row says "stored".
Take the wireframes as the expected drawing wherever one exists.

Verdicts: PASS, FAIL (with the screen and the steps), or BLOCKED (the
feature the row needs is not on `main` yet; the key should then answer
with a "not built yet" sentence in the hint bar, which is itself a PASS
for a phase that has not landed).

## Phase 7: home page

| Id   | Steps | Expected |
|------|-------|----------|
| 7.01 | `start --fresh --size 120x36`, `screen` | Blank margin all round, status line, rule, `Today <weekday d Mon>` beside `Backlog`, a divider, an empty-state message in each pane as in wireframe 12-empty A and B, the hint bar reading `TODAY …` with key names, and the review count `0 in review` not in red. Compare the frame to 03-today.txt column for column. |
| 7.02 | `keys a`, `type "Write the weekly report"`, `keys Enter`, `type "Call the accountant"`, `keys Enter`, `screen` | Each Enter adds the row and keeps the field open, as DESIGN.md 8 says ("add & keep typing" in the hint bar). Rows appear under PLAN in the order typed. Stored: two tasks with `day` = today and positions 0 and 1. |
| 7.03 | `keys Escape`, `screen` | The field closes on an empty Enter or Escape; the cursor is on a task row. |
| 7.04 | `keys j k Down Up` | The cursor moves one row at a time and stops at the ends without wrapping unless a doc says otherwise. Arrow keys equal j and k. |
| 7.05 | `keys f`, `screen` | The cursor row moves to a FOCUS group above PLAN and is drawn bold. `f` again clears it and the row returns to PLAN at its old position. |
| 7.06 | With three or more rows: `keys J`, `keys K`, `screen` | Reorder swaps with the neighbour in the same group. The cursor follows the row. Stored positions change. |
| 7.07 | `keys Space`, `screen` | The row drops to a DONE group at the bottom with the time, dim, `[x]` in green. Cursor steps to the next row of the group it left. Hint bar says what happened and offers `u`. |
| 7.08 | `keys u`, `screen` | The close is undone; the row is back where it was, open. |
| 7.09 | Focus a row, close it, `screen` | The done row carries a `was focus` marker. |
| 7.10 | Cursor on a done row, `keys Space` | Reopening puts it at the end of PLAN (DOMAIN.md 12) and the cursor follows it there. |
| 7.11 | `keys e`, `type " and send it"`, `keys Enter`, `screen` | The row becomes a text field with the old title; the hint bar says Enter saves. The title is stored trimmed. |
| 7.12 | `keys e`, `keys Escape` | Editing backs out with the title unchanged. |
| 7.13 | `keys x`, `screen`, `keys u` | Delete removes the row at once, no confirmation; cursor steps to the next row; `u` brings it back at its position. Stored: `deleted_at` set then cleared. |
| 7.14 | `keys b`, `screen` | The task leaves today for the backlog. It appears in the backlog pane. Today's pane shows a MOVED group with `[→] title` and `to backlog`, as in 06-dates left pane. Stored: task `day` null, a `placements` row for today remains. |
| 7.15 | `keys l` or `Tab`, `screen` | The backlog pane title takes the accent and the hint bar switches to `BACKLOG …`. `h` comes back. |
| 7.16 | In the backlog, `keys a`, type a title, `keys Enter Escape` | A task is added to the backlog. |
| 7.17 | In the backlog on a task, `keys t`, `screen` | The task goes to the end of today's plan, the flag it was waiting with (if any) cleared. |
| 7.18 | On a today task, `keys m`, `screen` | The move card opens over the panes: today, tomorrow, the next work day, next Monday, a date, the backlog, each with a key from the key table, accent border, no dimming behind. |
| 7.19 | Pick tomorrow, `screen`, `sql "select id,title,day from tasks"` | The task is stored on tomorrow's date and today shows it under MOVED as `to <weekday d Mon>`. |
| 7.20 | `keys u` | The move is undone: the task is on today again and the placement on tomorrow is gone. |
| 7.21 | `keys :`, `type "dele"`, `screen`, `keys Enter` | The palette lists actions with their keys; filtering narrows the list; Enter runs the selected one (here delete). Compare 11-palette-help. |
| 7.22 | `keys ?`, `screen`, `keys Escape` | The help overlay shows the whole key map for the current context and closes on Escape. |
| 7.23 | `keys /`, `type "report"`, `screen`, `keys Escape` | Search lists open matches first then closed, as in 09-search; Escape closes. |
| 7.24 | `keys /`, `type "tax return"`, `screen`, `keys Enter`, `screen` | Zero matches shows 12-empty E: Enter adds "tax return" to today. |
| 7.25 | `mouse click` on a row, `mouse wheel-down`, `mouse press`/`drag`/`release` to move a row two places | Click selects the row; the wheel moves the cursor; drag reorders and the carried row is marked `moving`. |
| 7.26 | Add 30 tasks to the backlog, `keys j` ×30, `screen` | The pane scrolls to keep the cursor visible; nothing is drawn off the pane; the top of the pane scrolls back when `k` returns. |
| 7.27 | `resize 80x44`, `screen`, `keys Tab`, `screen` | Under 100 columns the panes collapse to tabs `TODAY n  BACKLOG n  NOTES n` as in 04-narrow.txt; Tab switches the tab; the hint bar uses the narrow layout. `resize 160x48` shows the two-pane layout with more rows. |
| 7.28 | `restart`, `screen` | Everything is as it was: same rows, same groups, same order; the review count is 0. |
| 7.29 | Add a task, then `stop` with `kill -9` on the jobsdone process instead of `q` (find it with `pgrep -f target/debug/jobsdone`), `start` | The task is there. No change is ever lost to a kill. |
| 7.30 | Colour, on the PNGs | Only theme colours: cursor row in reverse video, key names in the hint bar in accent, group labels dim, focus bold, done dim with a green `[x]`. Nothing carries meaning by colour alone: read the `.txt` captures and confirm every state is legible without colour. |
| 7.31 | Render one capture with `render.py … --theme catppuccin-latte` | The light theme reads as well as the dark. |
| 7.32 | `keys ?`, then `keys :` and `type "morning"` | Every key the hint bar, help and palette name does something; nothing answers "not built yet" anywhere in the program. The palette lists the morning review under `M`. |
| 7.33 | Two instances: start a second copy on the same data in another tmux session (`UAT_SESSION=second uat/tui start --data uat/out/data`), add a task in one, `screen` in the other after a second | Both show the same rows within a tick (DESIGN.md 1). |

## Phase 8: dates, waiting and repeat

| Id   | Steps | Expected |
|------|-------|----------|
| 8.01 | Backlog task, `keys d`, `screen` | The due-by card as in 06-dates: a typed date field, alt-1 to alt-4 quick dates with their resolved dates, alt-0 clear, a calendar reachable with Tab, footer `⏎ set esc cancel tab calendar …`. `alt-r` switches the card to remind on. |
| 8.02 | `keys M-1`, `screen` | Due tomorrow. The row carries a yellow `[due <d Mon>]` chip. Stored `due_on`. |
| 8.03 | `keys d`, `type "30 sep"`, `keys Enter` | A typed date parses and is set; the card showed the resolved date beside the field as it was typed. |
| 8.04 | `keys d`, Tab to the calendar, move with h/l/j/k, `<`/`>` for the month, `keys Enter` | The calendar picks the date. |
| 8.05 | `keys d`, `keys M-0` | The date is cleared, the chip gone. |
| 8.06 | `keys r`, pick a date | A cyan `[◷ <d Mon>]` remind chip. Stored `remind_on`. |
| 8.07 | Write a due date in the past with `sql`, `restart`, `screen` | The chip is red and says how far over, as in 02-surfaced `[due 3 Sep · 2 days over]`. |
| 8.08 | `keys w` on a backlog task, `screen` | The row moves to a WAITING group in the backlog, drawn dim with a magenta `[waiting]` chip; the backlog title counts `n · m waiting`. `w` again clears it in place. |
| 8.09 | `keys w` on a today task | It goes to the backlog as waiting; today shows it under MOVED. |
| 8.10 | On a waiting task, `keys t` | Pulled onto today with the flag cleared. Stored `waiting` = 0, `day` = today. |
| 8.11 | On a today task, `keys R`, `screen` | The repeat card as in 07-repeat: the five rules with their controls, `0` stop, a `Next:` preview line, footer `⏎ save esc cancel h/l adjust`. |
| 8.12 | Choose "every work day", `keys Enter`, `screen` | The row carries `[↻ work days]`. Stored: a `schedules` row, the task's `schedule_id` and `scheduled_on` set. The backlog pane lists the schedule, or a doc says why not (the phase 7 report left this open). |
| 8.13 | Choose "every week on" and toggle days with h/l, `screen` | The preview updates as the rule changes. |
| 8.14 | `keys e` on a copy, change the title, `keys Enter`, `screen` | The one deliberate question: this copy, or this and future copies. Answering "future" changes the schedule's title, not other existing copies. |
| 8.15 | `sql` to move `generated_through` back three work days, `restart`, `screen` | Copies exist for every scheduled date since, one per day, each on its day's plan; the older ones would be on the pile. No duplicates on a second restart. |
| 8.16 | `keys R`, `keys 0`, `keys Enter` | Copies stop; existing copies stay where they are. Stored `stopped_on`. |
| 8.17 | Undo after each of 8.02, 8.08, 8.12 | `u` reverses the date, the flag, and the schedule creation. |
| 8.18 | Screen at 80x44 with chips | Chips still fit and read; long titles are cut before the chip, not after. |

## Phase 9: morning review

| Id   | Steps | Expected |
|------|-------|----------|
| 9.01 | Fresh data, `start` | No review is shown when there is nothing to review (12-empty C). |
| 9.02 | `stop`; with `sql` put two open tasks on yesterday and one on a date three weeks ago with placements; delete the `review_*` meta keys; `start`, `screen` | The pile as in 01-review: `MORNING REVIEW step 1 of 2 · the pile`, `n unfinished from past days`, `esc skip for now`, the cursor task's title in capitals on the right, groups per day with relative age (`YESTERDAY · THU 4 SEP`, `3 WEEKS AGO`), the key card `d t b m x`, a progress bar `0 of n handled`. |
| 9.03 | `keys d` on one, `keys t` on the next, `keys b`, `keys x` | Each decision applies at once; the row stays where it was, checked and dim, saying what happened (`✓ closed`, `→ today`, `→ backlog`, `✗ deleted`); the cursor steps to the next undecided row; the bar advances; `u` undoes the last decision. `k` does not move the cursor here (DESIGN.md 4). |
| 9.04 | When the pile is empty, `screen` | Step 2 shows only if something is surfaced; otherwise the home page. |
| 9.05 | Set up a due-today task, an overdue one, a reminder for today, a waiting task with a due date, a waiting task with a reminder, and a schedule with a copy for today; delete the `review_*` meta; `start` | Step 2 as in 02-surfaced: DUE, REMINDERS, ALSO STARTING TODAY groups; the waiting task's due date does not surface but its reminder does (PRODUCT.md waiting); keys `t k d w space`; `Start the day ⏎` box. |
| 9.06 | Decide each, `keys Enter` | Home page. Status line `0 in review`. |
| 9.07 | `restart` | The review does not show again today. Stored `review_on` = today. |
| 9.08 | Set up a pile again, delete the meta, `start`, handle one, `keys Escape`, `screen` | Skipping leaves the pile; the home page shows the count in red `n in review`. |
| 9.09 | `restart`, `screen`, `keys M`, `screen` | A restart the same day goes straight to today with the red count (DESIGN.md 1: closing mid-review leaves the pile intact). `M` opens the review again on what is still on the pile and whatever has surfaced since; it never shows empty. |
| 9.10 | With a pile and empty surfaced set | Step 2 is skipped; the review says `step 1 of 1` or the equivalent the docs define. |

## Phase 10: history and search

| Id    | Steps | Expected |
|-------|-------|----------|
| 10.01 | On the home page, `keys [`, `screen` | The day pane shows yesterday, titled `<Ddd d Mon> past day`, with counts `n planned · n done · n open · n moved` and `. back to today` in the status line; the backlog pane becomes the day list as in 08-history: THIS WEEK, LAST WEEK, EARLIER, `done / planned`, `· n open`, days with nothing planned skipped. |
| 10.02 | `keys [` several times, `keys ]`, `keys .` | Steps back and forward; `.` returns to today. A future day works forward. |
| 10.03 | A past day with a task still open | The row shows a red `[on the pile]` chip in FOCUS or PLAN. |
| 10.04 | A past day with a task moved away | MOVED group under DONE, `[→] title` and `to today` / `to <day>` / `to backlog`, normal weight. `keys Enter` on it jumps to where the task is now. |
| 10.05 | `sql` the moved task's current day; then check the past day | The past day still lists it as moved. The planned record is never rewritten. |
| 10.06 | A past day with nothing planned | 12-empty D: `Nothing was planned on this day.` with `[` and `g` hints. |
| 10.07 | On a past day, `keys a`, type, `keys Enter` | A task can be added to that day (`+ add a task to this day`). |
| 10.08 | `keys g`, pick a date | The day pane jumps there. |
| 10.09 | In the day list, `keys j` and `keys Enter` | Selecting a day opens it in the day pane. |
| 10.10 | `keys /`, type a title closed weeks ago | CLOSED group lists it with its day; a recurring copy shows `· ↻`; `keys Enter` goes to that day; `keys M-t` re-adds it to today as a new task. |
| 10.11 | 80x44 | The day list works as a tab. |

## Phase 11: scratchpad

| Id    | Steps | Expected |
|-------|-------|----------|
| 11.01 | `keys n`, `screen` | The notes page: `Notes n notes` status line, `n or esc back to today`, the list beside the open note, 12-empty F when there are none. |
| 11.02 | `keys a`, `type "Mention to Anna: CI runner budget"`, `keys Enter`, `type "- Friday demo slot"`, `screen` | A note is created and the text area takes typing, including newlines. Letters do not trigger commands while the note has focus. |
| 11.03 | `keys Escape`, `screen` | Focus returns to the list; the row shows the first line cut to fit and a relative age, as in 10-scratchpad. |
| 11.04 | `keys n` or `Escape`, then `keys n` again | Leaving and reopening the page shows the note with its full text. Stored: `notes.body`. |
| 11.05 | `restart` | The text is still there. Then test the save moment the docs define: type text, kill with `kill -9` at that moment, `start`; what the doc promises is saved is saved. |
| 11.06 | Create three notes, `keys j`, `keys Enter` | The list opens the selected note. Newest first. |
| 11.07 | `keys x` on a note, `keys u` | Deleted at once, `u` brings it back. |
| 11.08 | `resize 80x44`, `screen` | NOTES tab as in 04-narrow. |
| 11.09 | The status line's note count on the home page | Matches the number of notes. |

## Cross-cutting, run last

| Id   | Steps | Expected |
|------|-------|----------|
| X.01 | Every popup at 80x44 | Fits inside the window, still boxed, still readable. |
| X.02 | `keys q` inside a text field, inside a popup | `q` types or is ignored; it only quits from a list. |
| X.03 | `keys C-c` | The terminal is restored (tmux shows a prompt, no raw-mode garbage). |
| X.04 | `uat/tui log` after all of the above | No errors or warnings that a test did not deliberately cause. |
| X.05 | Timing: `start` on a database with 500 tasks over 200 days | The first screen appears within a moment; `screen` right after `start` already shows it. |
