# Domain

The model behind PRODUCT.md and DESIGN.md: the entities, their states,
the commands that change them, the invariants that must hold, and the
storage schema. The words used here are the words used in the code.

Everything a screen shows is a view of this model. Section 18 walks
through every wireframe and names the view it is drawn from.

## 1. Vocabulary

| Word            | Meaning                                                                  |
|-----------------|--------------------------------------------------------------------------|
| task            | A title, whether it is done, and where it lives. Nothing else.           |
| place           | Where a task lives: the backlog, or one day.                             |
| day             | A civil date. Days are not stored; a day exists when a task refers to it.|
| working day     | The date the program calls "today". It changes at `day_starts_at`, 05:00 by default, not midnight. |
| position        | A task's index in the order of its place.                                |
| focus           | A task marked as one of the day's must-dos.                              |
| closed          | Done. A closed task remembers when it was closed.                        |
| open            | Not closed.                                                              |
| placement       | The record that a task was put on a day. Kept after the task leaves.     |
| moved           | A placement whose task is no longer on that day.                         |
| waiting         | A backlog task blocked on someone or something else.                     |
| due, remind     | Dates a task carries. They surface it; they never move it.               |
| surface         | Appear in the second step of the review, as a prompt.                    |
| pile            | Every open task on a day before today, back as far as the pile horizon reaches. |
| schedule        | A repeat rule with a title. It creates copies.                           |
| copy            | A task created by a schedule for one date. Ordinary from then on.        |
| review          | The two-step morning pass over the pile and the surfaced tasks.          |
| note            | A plain-text scratchpad entry.                                           |
| archived        | A live note put out of the notes list without being deleted.             |
| command         | One change to the model. Every command has an inverse.                   |
| settings        | The fourteen values that change what the rules do. Section 19.           |

## 2. Time

### Working day

The domain never reads a clock. The application passes an instant (a
zoned timestamp) into every command that needs one, and the domain
derives the date from it:

    working_day(instant) = civil date of (instant - day_starts_at hours), local time

So 01:30 on Saturday belongs to Friday. The hour is
`settings.day_starts_at`, which is 5 unless it has been changed (section
19), so the working day is `model.settings.working_day(instant)` and no
caller may work it out for itself. Every rule below that says "today"
means the working day of the instant the command was given.

Consequences:

- A task closed at 01:30 sits in Friday's Done group, showing 01:30.
- Due and remind dates are compared with the working day.
- A copy for date D is created when the working day reaches D.
- The review gate compares working days.
- Moving the hour moves all of these at once: the day the panes are on is
  worked out again the moment the setting is saved.

### Representation

Dates are civil dates (`jiff::civil::Date`), stored as `YYYY-MM-DD`.
Instants are zoned timestamps, stored as RFC 3339 text with the offset
and the time zone it came from, for example
`2026-09-05T08:12:00+02:00[Europe/Copenhagen]`. The bracketed name is RFC
9557's addition to RFC 3339; without it the offset alone would not read
back as the zoned timestamp that was written.

### Reading a typed date

The date card is the one place a date is written rather than picked, and
what the shapes mean is a rule about dates. `parse_date(text, today)`
reads, case and spacing aside:

| Typed                          | Means                                        |
|--------------------------------|----------------------------------------------|
| `2026-09-30`                   | That date.                                   |
| `30 sep`, `sep 30`, `30/9`     | Day and month, in either order, any of ` / - . ,` between them. |
| `30 sep 2027`, `1/10/2027`     | The same with a year.                        |
| `30`                           | The next month that has a 30th.              |
| `mon`, `monday`                | The next such weekday, never today.          |
| `today`, `tomorrow`            | Those days.                                  |
| `+3`, `-3`                     | Three days from today, or three days ago.    |

A month or a weekday is written out or cut to three letters. A date with
no year is the next one that has not passed, so `1 sep` typed in
December is next September. Nothing else is guessed: text that is not
one of these shapes is not a date, and the card says so rather than
choosing a day.

## 3. Task

| Field          | Type            | Meaning                                                  |
|----------------|-----------------|----------------------------------------------------------|
| id             | integer         |                                                          |
| title          | text            | Non-empty after trimming, one line, no maximum length.   |
| day            | date or none    | The place. None means the backlog.                       |
| position       | integer         | Index within the place. See section 4.                   |
| focus          | bool            |                                                          |
| closed_at      | instant or none | None means open.                                         |
| waiting        | bool            | Only ever true in the backlog.                           |
| due_on         | date or none    |                                                          |
| remind_on      | date or none    |                                                          |
| schedule_id    | id or none      | Set on a copy.                                           |
| scheduled_on   | date or none    | The date the copy was created for. Set with schedule_id. |
| created_at     | instant         |                                                          |
| deleted_at     | instant or none | Set means deleted. See section 11.                       |

A title is the whole task. Nothing in it is parsed.

### States

A task is in exactly one place and is either open or closed. On top of
that it may be focus, waiting, dated, or a copy. The combinations that
are not allowed:

- `waiting` implies `day` is none. Waiting is a backlog state.
- `schedule_id` and `scheduled_on` are set together or not at all.
- No two live copies of one schedule share a `scheduled_on`.
- A closed task cannot be moved. Its place is where it was done.

"Live" means `deleted_at` is none. Deleted tasks are invisible to every
view, every count, search, and generation, but their rows stay.

### Focus

Focus is a flag. There is no cap on how many tasks on a day carry it.
Closing keeps the flag; a closed focus task shows "was focus". Reopening
keeps it too, so the task returns to the Focus group.

## 4. Places and order

Every place, each day and the backlog, is one manually ordered list. The
positions of the live tasks in a place are the dense integers `0..n`, in
one sequence regardless of focus, closed or waiting. The groups a screen
draws (Focus, Plan, Done, Moved, Waiting) are filters over that one
sequence, not lists of their own.

- A task arriving in a place, by add, move, reopen or generation, gets
  position `n`, the end.
- Reorder swaps a task with its neighbour and renumbers nothing else.
- Delete and move renumber the place the task left, in the same
  transaction, so the sequence stays dense.
- A closed task keeps its position. The Done group orders by
  `closed_at`, so the position only matters if the task is reopened,
  and reopening sends it to the end anyway. It is kept because
  nothing about a task changes when it is closed except `closed_at`.

The domain owns the density invariant. The schema does not enforce it,
because renumbering inside a transaction would trip a unique index.

## 5. Placements

A placement records that a task was put on a day:

| Field      | Type    | Meaning                                                   |
|------------|---------|-----------------------------------------------------------|
| task_id    | id      |                                                           |
| day        | date    |                                                           |
| placed_at  | instant |                                                           |
| from_place | text    | `new`, `backlog`, or the date the task came from.         |

One row per task and day. The row is written when the task arrives on the
day and is never updated. If the task comes back to a day it has already
been on, the existing row stands; `from_place` keeps the first arrival.

The backlog has no placements. Putting a task in the backlog writes
nothing; the day it left keeps its row.

Placements are the whole history mechanism:

- A day's **Moved** group is its placements whose task is live and whose
  `task.day` is not that day. The pointer text is computed from where the
  task is now: "to today", "to Wed 3 Sep", "to backlog". A task moved
  Mon to Tue to Wed shows "to Wed" on both Mon and Tue.
- The **day list** is the distinct days that have a placement.
- The "←backlog" annotation on a plan row is `from_place = backlog` with
  `placed_at` on the working day being shown.

The one command that deletes a placement row is the undo of the move
that created it. Undo restores the state before the command, and the
row did not exist then.

## 6. Views of a day

A day is drawn the same way whether it is today, past or future: four
groups in a fixed order, and a group with nothing in it is not drawn.

| Group | Contents                                                  | Order        |
|-------|-----------------------------------------------------------|--------------|
| Focus | live, `day` = D, open, focus                               | position     |
| Plan  | live, `day` = D, open, not focus                          | position     |
| Done  | live, `day` = D, closed                                   | closed_at    |
| Moved | placements for D whose live task has `day` ≠ D            | placed_at    |

Row annotations, all derived:

- `was focus`: closed and focus.
- `on the pile`: open, D before today, and D within the pile horizon.
- `still open`: open, D before today, and D beyond the horizon, which is
  the same task saying so without asking to be dealt with.
- Done time: the time of `closed_at` when its working day is D;
  otherwise the date, "closed Tue 2 Sep", because a task closed from the
  review is closed on its old day at a later date.
- Chips: due, remind, repeat, with `[due …]` red when `due_on` < today
  and yellow otherwise.

Counts, with `placed` = live placements for D, `kept` = placed whose task
is still on D, `moved` = placed − kept:

| Where                     | Formula                                              |
|---------------------------|------------------------------------------------------|
| Today header              | open = kept open · done = kept closed · moved        |
| Past day header           | planned = placed · done · open · moved               |
| Day list row              | done / kept, and "· n open" when D is before today and open > 0 |
| Days with nothing planned | placed = 0; skipped in the list, shown empty when stepped to |

An open task on a day that has passed is on the pile while the horizon
reaches it, which is what the day list's open count warns about. The
count is of the day, not of the pile, so a day beyond the horizon still
says what it left open. Today's own open tasks are the working list and a
future day's are a plan, so neither is counted there; the row says
"today" instead.

The list is newest day first, broken into four stretches by the first
day of the week today is in, which `week_starts_on` names: **later**
(after this week), **this week**, **last week**, and **earlier**. A
stretch with no day in it is not drawn, which is why a list of one old
day is one group.

## 7. Views of the backlog

| Group    | Contents                              | Order    |
|----------|---------------------------------------|----------|
| ordinary | live, `day` none, open, not waiting   | position |
| Waiting  | live, `day` none, open, waiting       | position |

Header count: live open backlog tasks, and how many of them are waiting.

Below the two groups the backlog pane lists the live, unstopped
schedules by title and rule (wireframes 03 and 07). That list is a view
of section 10, not of tasks: the cursor reaches its rows, `R` on one
opens the repeat card for that schedule, and a key that acts on a task
answers that the row is not one. It is how a schedule is changed or
stopped when no copy of it is on a screen, which for a monthly rule is
most of the month.

## 8. Due and remind

Both are dates on the task and both stay on it through every move and
after closing. Only backlog tasks surface. When a dated task is pulled
onto a day it is already planned, so there is nothing to prompt; when it
is sent back to the backlog the date prompts again.

| Kind   | Surfaces when                                                                     |
|--------|-----------------------------------------------------------------------------------|
| due    | `day` none, open, not waiting, `due_on` ≤ today + `due_ahead_days`. Every review until acted on. |
| remind | `day` none, open, `remind_on` in (previous review date, today]. Once.            |

A due date seen before it arrives is not overdue, so surfacing early
changes when the step shows a task and not the order it shows it in:
overdue first, then by date.

"Previous review date" is the date of the last review started on a day
before today (section 13 keeps it). A reminder set for a Saturday is
seen at Monday's review. A reminder set for today, after today's review
has already started, is not seen at a review at all; its chip in the
backlog is what shows it. The first review ever has no lower bound.

Waiting suppresses due, not remind: a waiting task with a reminder
surfaces, and the screen dims it.

## 9. Waiting

Waiting is a state of a backlog task. The invariant is `waiting` implies
`day` is none, and three commands keep it:

- `w` on a day task moves the task to the end of the backlog (leaving a
  placement pointer on the day, as any move does) and sets waiting. One
  command, one undo entry.
- `t` or `m` on a waiting task clears waiting as part of the move.
- `w` on a backlog task toggles waiting.

Because waiting tasks are never on a day, the pile never holds one, and
the due step of the review skips them (section 8). That is the whole of
"not nagged about".

## 10. Schedules and copies

### Schedule

| Field             | Type          | Meaning                                                  |
|-------------------|---------------|----------------------------------------------------------|
| id                | integer       |                                                          |
| title             | text          | The title new copies get.                                |
| rule              | rule          | See below.                                               |
| generated_through | date          | Copies exist for every scheduled date up to and including this. |
| stopped_on        | date or none  | Set means no more copies.                                |
| created_at        | instant       |                                                          |

A schedule is never deleted while a copy points at it, which in practice
is never: stopping is the only way to end one, and search still shows the
`↻` mark on its old copies.

### Rules

Exactly five shapes, stored as JSON in `rule`:

| Shape          | JSON                                             | Dates                                                    |
|----------------|--------------------------------------------------|----------------------------------------------------------|
| work days      | `{"kind":"workdays"}`                            | The days `work_days` names, Monday to Friday by default. No holidays, ever. |
| every day      | `{"kind":"daily"}`                               |                                                          |
| weekly         | `{"kind":"weekly","weekdays":["mon","thu"]}`     | The listed weekdays, at least one.                       |
| monthly        | `{"kind":"monthly","day":15}` or `"day":"last"`  | Day N clamped to the month's last day; or the last day.  |
| every N weeks  | `{"kind":"every_n_weeks","n":2,"from":"2026-09-05"}` | `from` and every `7n` days after it. N ≥ 1.          |

"Every N weeks" is one weekday (that of `from`); "weekly" is a set of
weekdays. They do not overlap. The "Next work day" shortcut in the move
card reads `work_days` the same way the work-days rule does, so the two
never disagree about which day comes next.

`next_dates(rule, after, count, work_days)` is a pure function used both
by the repeat card's preview and by generation. It takes the work days
rather than reading them off a model, because the repeat card previews a
rule that has not been saved to one.

### Creating a schedule from a task

`R` on a task creates a schedule with the task's title and the chosen
rule, and links the task as the first copy:

- `scheduled_on` = the task's day if it is on one, else today.
- `generated_through` = that same date.

The task stays where it is. Its own date need not match the rule; it is
simply the first copy. The repeat card previews `next_dates` after
`generated_through`.

### Generation

`generate_copies(today)` runs on every terminal UI launch, before the review.
The CLI invokes it explicitly through `refresh`; ordinary reads do not generate. For
each live schedule with `stopped_on` none, for every scheduled date D in
(`generated_through`, today] that is not before today − `backfill_days`:

- create a task with `title` = schedule title, `day` = D, position at
  the end of D, not focus, `schedule_id` and `scheduled_on` set, and a
  placement for D with `from_place = new`;
- then set `generated_through` = today.

With `backfill_days` at 0 there is no cap: three weeks away means fifteen
standup copies, each on its own past day, all on the pile. PRODUCT.md
promises this and the pile is where the cost is meant to be seen. With a
cap, only the dates inside it are copied and `generated_through` still
moves to today, so a date the cap skipped is never copied later.

Generation is not a user action and pushes nothing on the undo stack. It
is idempotent across instances: the unique index on
`(schedule_id, scheduled_on)` makes the second writer's insert fail, and
that failure is ignored. A deleted copy keeps its row, so it is never
regenerated.

### Editing

- Changing the rule takes effect from the next generation; existing
  copies stay.
- Editing a copy's title asks the one question. "This copy" renames the
  task. "This and future copies" renames the task and the schedule.
- Stopping sets `stopped_on` = today. Copies stay where they are,
  including any on future days.

## 11. Delete and undo

### Delete

Delete sets `deleted_at`. A deleted task is invisible everywhere,
including search, history, and the day list, and its place is renumbered
as if it had been removed. Deleting a task that was closed on a past day
removes it from that day's record. That is the one deliberate act of
forgetting the product has, and it is undoable.

Notes are deleted the same way. Archiving a note is not deleting it: an
archived note is live, and deleting one works as for any other note.

### Undo

The undo stack is part of the data, one stack per database, shared by
every running instance and surviving restarts. Each entry has an integer identity
allocated from the persistent `meta.undo_high_water` counter. Allocation and the
entry commit atomically; undo, rejected inverses, and stack truncation never
reduce the counter. Compound operations allocate one identity. Exhaustion of the
signed 64-bit counter rejects the operation without writes.

- Every user command in section 12 pushes one entry: an instant, a
  label for the hint bar, and the inverse command as JSON.
- `u` pops the top entry and applies its inverse as an ordinary command,
  except that applying an inverse pushes nothing. There is no redo.
- If the inverse's precondition fails, because another instance has
  changed things since, the entry is dropped and the hint bar says so.
  A position that no longer exists is clamped to the end, not failed.
- The stack is a LIFO capped at a length the application passes in;
  the oldest entry is discarded when the cap is exceeded. The domain
  does not choose the number.
- UI note typing (`EditNote`), copy generation, starting a review, and undo
  itself push nothing. A deliberate CLI note replacement (`ReplaceNote`) is one
  undoable operation.
- Undoing `UnarchiveNote` puts the note back at the `archived_at` it had
  rather than now, so it returns to its place in the archive's order. The
  inverse carries the old instant, as `RestoreNoteBody` carries the old
  body.
- After a reload (another instance wrote), the stack is reloaded with
  everything else.

Note-editor undo/redo is separate application state: Ctrl+Z/Ctrl+Y restores
local text snapshots and autosaves the restored body through EditNote. It
does not add entries to this shared stack or survive a process restart.

## 12. Commands

Every change to the model is one of these. The precondition is checked
first; a command whose precondition fails does nothing and reports why.
Each command runs in one storage transaction. "End" means the last
position of the target place. `now` and `today` come from the instant the
application passes in.

### Tasks

| Command                  | Precondition                     | Effect                                                                                                 | Inverse                                     |
|--------------------------|----------------------------------|--------------------------------------------------------------------------------------------------------|---------------------------------------------|
| AddTask(title, place)    | title valid                      | New live open task at end of place. Placement `new` if place is a day.                                 | DeleteTask                                  |
| EditTitle(task, title)   | live, title valid                | Sets title.                                                                                            | EditTitle(old)                              |
| EditTitleAndFuture(task, title) | live copy, title valid    | Sets title of task and of its schedule.                                                                | EditTitleAndFuture(old task title, old schedule title) |
| Close(task)              | live, open                       | `closed_at` = now. If in the backlog: first Move to today, then close. Position and focus unchanged.  | Reopen restoring position, place and `closed_at` none |
| Reopen(task)             | live, closed                     | `closed_at` none, position = end of its place. Focus unchanged.                                        | Close restoring old `closed_at` and position |
| SetFocus(task, bool)     | live, on a day                   | Sets focus.                                                                                            | SetFocus(old)                               |
| Move(task, place)        | live, open, place ≠ current      | Renumber old place; position = end of new; `waiting` = false; placement written if new place is a day and no row exists for that day. | Move back, restoring position and `waiting`, deleting the placement row if this command created it |
| Reorder(task, position)  | live                             | Moves the task within its place. A position past the end is clamped to it, because undo needs that (section 11) and one code path serves both. | Reorder(old position)                       |
| SetWaiting(task, bool)   | live, open                       | If on a day and bool is true: Move to backlog, then set. Otherwise sets the flag.                      | The reverse Move if one happened, and the old flag |
| SetDue(task, date/none)  | live                             |                                                                                                        | SetDue(old)                                 |
| SetRemind(task, date/none) | live                           |                                                                                                        | SetRemind(old)                              |
| DeleteTask(task)         | live                             | `deleted_at` = now; renumber its place.                                                                | RestoreTask at old position                 |

Labels: "Added …", "Renamed …", "Closed …", "Reopened …", "Moved … to
today", "Deleted …", and so on, each naming the task.

### Schedules

| Command                        | Precondition                | Effect                                                            | Inverse                                     |
|--------------------------------|-----------------------------|-------------------------------------------------------------------|---------------------------------------------|
| CreateSchedule(task, rule)     | live, not a copy            | Section 10. The schedule row and the task's link.                 | Unlink the task and delete the schedule row |
| SetRule(schedule, rule)        | live, not stopped           | Sets rule.                                                        | SetRule(old)                                |
| EditScheduleTitle(schedule, title) | exists | Renames future copies without changing existing task titles. | EditScheduleTitle(old) |
| StopSchedule(schedule)         | not stopped                 | `stopped_on` = today.                                             | `stopped_on` none                           |
| GenerateCopies                 | system                      | Section 10. Not undoable.                                         | none                                        |

Deleting a schedule outright is not a command. The inverse of
CreateSchedule may delete the row because at that moment its only copy
is the task being unlinked.

### Review

| Command       | Precondition | Effect                                                                     | Inverse |
|---------------|--------------|----------------------------------------------------------------------------|---------|
| StartReview   | system       | If `review_on` ≠ today: `review_before` = `review_on`, `review_on` = today. | none    |

Every decision in the review (`d`, `t`, `b`, `m`, `x`, `s`, `w`, `space`)
is one of the task commands above, or nothing at all in the case of
`s leave in backlog`. The review adds no state to tasks.

### Notes

| Command               | Precondition | Effect                       | Inverse           |
|-----------------------|--------------|------------------------------|-------------------|
| CreateNote            |              | New empty note, `created_at` = now. | DeleteNote  |
| EditNote(note, body)  | live         | Sets body, `updated_at` = now. Not undoable. | none |
| ReplaceNote(note, body) | live       | Sets body, `updated_at` = now. | RestoreNoteBody(old body, old `updated_at`) |
| DeleteNote(note)      | live         | `deleted_at` = now.          | RestoreNote       |
| ArchiveNote(note)     | live, not archived | `archived_at` = now.   | UnarchiveNote     |
| UnarchiveNote(note)   | live, archived | `archived_at` none.        | RearchiveNote(old `archived_at`) |

Every command but EditNote works on an archived note as on any other.
Labels: "Added a note", "Replaced a note", "Deleted a note", and "Archived
…" and "Unarchived …" naming the note's first line, or "a note" when it
is blank.

### Settings

Changing a setting is not a command. `change_settings` stands beside
GenerateCopies and StartReview: one write, not undoable, nothing on the
stack (section 19).

## 13. The review

### The pile

    pile = live, open, day < today, and day ≥ today − `pile_horizon_days`

Grouped by day, newest day first; within a day, in position order. The
age shown is `today − day` in days, rendered relatively by the screen.
`was focus` on a pile row is the focus flag. A horizon of 0 reaches every
day; anything older than one that is set stays on its day, out of the
pile and out of the count, marked `still open` (section 6).

### Surfaced

Three groups, each computed at the moment the step is shown:

| Group               | Contents                                                        |
|---------------------|-----------------------------------------------------------------|
| Due                 | section 8, due; overdue first, then by `due_on`                 |
| Reminders           | section 8, remind; waiting ones dimmed                          |
| Also starting today | live copies on today with `scheduled_on` = today, shown for information |

### Gate and session

The meta table holds two dates:

| Key            | Meaning                                                  |
|----------------|----------------------------------------------------------|
| review_on      | The working day a review was last started.               |
| review_before  | The value `review_on` had before that.                   |

On launch, after generation: if `review_opens_itself`, `review_on` ≠
today, and the pile or the surfaced set is non-empty, the review opens
and StartReview runs. With the setting off a launch writes no gate at
all, and `M` opens the review, which runs StartReview then. An
empty step is skipped; if both are empty nothing opens and the gate is
not written. Escape leaves the review with the pile intact. The review
can be started again any time from the command palette; that also runs
StartReview, which changes nothing on the same day.

The home screen's "n on the pile" is the live size of the pile, whatever
was or was not decided.

Which rows have been handled, the "2 of 7" progress, and the "→ today"
and "✓ closed" annotations are a **review session**: a value the
application holds in memory for the length of the review. It holds the
task ids the review opened with and the decision made for each. It is
not stored; closing the window mid-review forgets it, and the pile
itself is the record.

A row the review has decided about has left the pile or the surfaced
set, and it stays on screen in the day the review found it, drawn as the
task is now, so the list never moves under the person's hand. That is
the one place a deleted task is still drawn: the review has to be able
to say what it did to it, and `u` is on the same line.

## 14. Search

    matches(text) = live tasks whose title contains text, case-insensitive

Results in two groups: open tasks (their place and flags beside them),
then closed tasks by place day, newest first. Each recurring copy is its
own row, marked `↻`. Enter goes to the task's place day. Notes are not
searched: they are filtered on their own page (section 15).

The empty result offers to add the typed text as a task on today. A
result offers the same thing under `alt-t`, with its own title: a task
done long ago is started again as a new task rather than reopened,
because reopening would take the old one off the day it was done on and
rewrite that day's record.

## 15. Notes

| Field      | Type            |
|------------|-----------------|
| id         | integer         |
| body       | text            |
| created_at | instant         |
| updated_at | instant         |
| deleted_at | instant or none |
| archived_at | instant or none |

A live note is in one of two lists: the notes, where `archived_at` is
none, or the archive. Archived notes never expire and are not in the note
count the home page shows.

The notes list is ordered by `created_at`, newest first, and editing does
not move a note. The list row shows the first line of the body and
`created_at`, which the screen renders as an age. It is the age of the
note rather than of its last edit, because that is the order the list is
already in. The archive is ordered by `archived_at`, newest first, ties
by the higher id, and its rows show the age since archiving for the same
reason.

A filter narrows whichever list is shown. Its text is split on
whitespace, and a note matches when every word is in its whole body as a
subsequence, ignoring case and Unicode composition. Matches are ranked by
how well the letters fit: at word starts and in runs score more, gaps
cost. Ties keep the list's own order. The filter is application state,
not model state.

## 16. Several instances

The domain is a value: the application loads the whole model from
storage and rebuilds it whenever storage reports a change (STACK.md
section 3). Nothing in this document depends on which instance did what:
the pile, the surfaced set, the day views, the gate and the undo stack
are all recomputed from the same rows. The in-memory review session is
the only per-instance state, and it holds ids, so a task another
instance has since moved simply shows in its new state.

## 17. Storage schema

The DDL below is the first migration. Storage conventions:

- SQLite, WAL, `PRAGMA foreign_keys = ON` on every connection.
- Integer primary keys. Dates as `YYYY-MM-DD` text, instants as RFC 3339
  text with offset. Booleans as 0 and 1.
- Migrations are numbered SQL files applied in order on start;
  `PRAGMA user_version` holds the number of the last one applied. A
  migration only adds: new tables, new nullable columns with defaults,
  new indexes. A change that needs more gets a new table and a copy step
  in the same migration. Downgrades are not supported.
- One command, one transaction.

```sql
CREATE TABLE tasks (
    id            INTEGER PRIMARY KEY,
    title         TEXT    NOT NULL,
    day           TEXT,
    position      INTEGER NOT NULL,
    focus         INTEGER NOT NULL DEFAULT 0 CHECK (focus IN (0, 1)),
    waiting       INTEGER NOT NULL DEFAULT 0 CHECK (waiting IN (0, 1)),
    closed_at     TEXT,
    due_on        TEXT,
    remind_on     TEXT,
    schedule_id   INTEGER REFERENCES schedules (id),
    scheduled_on  TEXT,
    created_at    TEXT    NOT NULL,
    deleted_at    TEXT,
    CHECK (title <> '' AND title = trim(title) AND instr(title, char(10)) = 0),
    CHECK (waiting = 0 OR day IS NULL),
    CHECK ((schedule_id IS NULL) = (scheduled_on IS NULL))
);

CREATE INDEX tasks_place ON tasks (day, position) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX tasks_copy ON tasks (schedule_id, scheduled_on)
    WHERE schedule_id IS NOT NULL;

CREATE TABLE placements (
    task_id     INTEGER NOT NULL REFERENCES tasks (id),
    day         TEXT    NOT NULL,
    placed_at   TEXT    NOT NULL,
    from_place  TEXT    NOT NULL,
    PRIMARY KEY (task_id, day)
);

CREATE INDEX placements_day ON placements (day);

CREATE TABLE schedules (
    id                 INTEGER PRIMARY KEY,
    title              TEXT NOT NULL,
    rule               TEXT NOT NULL,
    generated_through  TEXT NOT NULL,
    stopped_on         TEXT,
    created_at         TEXT NOT NULL
);

CREATE TABLE notes (
    id          INTEGER PRIMARY KEY,
    body        TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    deleted_at  TEXT
);
-- archived_at TEXT, added by the fifth migration.

CREATE TABLE meta (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);

CREATE TABLE undo_log (
    id       INTEGER PRIMARY KEY,
    at       TEXT NOT NULL,
    label    TEXT NOT NULL,
    inverse  TEXT NOT NULL
);

PRAGMA user_version = 1;
```

The second migration adds the settings, which are data like everything
else and so are in the database rather than in a file of their own
(STACK.md section 8):

```sql
CREATE TABLE settings (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);

PRAGMA user_version = 2;
```

The third migration adds `personal_dictionary`, with a canonical `key` primary
key and a `word` display value. Entries are written individually, so changing
one word does not replace the other entries.

The fourth migration initializes `meta.undo_high_water` from the largest surviving
undo ID, or zero for an empty stack. New IDs are never reused after this upgrade,
including across restarts. Identities already recycled by older versions cannot
be reconstructed; clients must reread undo state after upgrading. The schema
version bump refuses older binaries when they open the database. Already-running
older connections are also protected: an insert trigger rejects an ID at or below
the persisted counter (and rejects missing or invalid counters), then an after-insert
trigger advances the counter in the same transaction. An old client may append a
fresh ID, but an attempted reuse fails atomically as a conflict.

The fifth migration adds `notes.archived_at`, a nullable instant. The note
upsert names its columns, so a client started before the migration that saves
a body into an archived note leaves `archived_at` as it is; it shows archived
notes as live until it is restarted.

Opening a database acquires a SQLite immediate transaction before reading its
schema version. All pending migrations and version updates commit together;
failure rolls back the entire sequence. Concurrent openers reread the version
after acquiring the lock. Journal-mode setup retries transient contention within
the same bounded three-second wait used for database locks.

Notes on the schema:

- `tasks.day` NULL is the backlog. Position density is the domain's
  invariant, not the schema's (section 4).
- `tasks_copy` is what makes generation idempotent across instances.
- `placements` rows are inserted and, only by undo, deleted. Never
  updated.
- `undo_log.inverse` is the JSON of a command from section 12. Rows are
  popped by deleting them; the cap is enforced by deleting the lowest
  ids.
- `meta` keys: `review_on`, `review_before`, `undo_high_water`.
- `settings` keys are the ones in section 19, every value text. A
  `PutSettings` writes the whole table: the rows are deleted and written
  again in one transaction, because the settings are one value in the
  model rather than fourteen. Loading is the same in reverse, so a
  missing key is that setting's default and a key this build does not
  know is ignored. The delete takes those unknown keys with it, which is
  the price of the value being whole: an older binary can read a newer
  database's settings, and writing one of them drops what it could not
  read.

## 18. Every screen as a view

The check PLAN.md asks for: each wireframe, and the parts of the model it
is drawn from.

| Wireframe               | Drawn from                                                                                      |
|-------------------------|-------------------------------------------------------------------------------------------------|
| 01 Review: the pile     | pile (13) grouped by day; focus flag; repeat chip; review session for handled rows and progress |
| 02 Review: surfaced     | due, remind, copies-today (13); waiting flag for dimming; review session for progress            |
| 03 Today + backlog      | day view for today (6); backlog view (7); pile size for the count; note count (15)              |
| 04 Half-width tile      | the same two views, one at a time; the archive (15) as the fourth tab                           |
| 05 Task states          | AddTask, EditTitle and the copy question, Reorder, SetFocus, Close, Move, DeleteTask and its undo label (12) |
| 06 Due, remind, waiting | SetDue, SetRemind, SetWaiting (12); chips and the Waiting group (6, 7)                          |
| 07 Repeat               | rule shapes and `next_dates` (10); CreateSchedule, SetRule, StopSchedule; the schedule list (7) |
| 08 History              | day view for a past day (6) including Moved; day list with counts (6)                            |
| 09 Search               | matches (14); place day; `↻` from `schedule_id`                                                 |
| 10 Scratchpad           | notes and the archive (15); CreateNote, EditNote, DeleteNote, ArchiveNote, UnarchiveNote; the filter |
| 11 Palette and help     | the command list (12) and the key map in DESIGN.md; no model state                              |
| 12 Empty states         | every view above when its contents are empty, the archive included; the empty search's AddTask   |
| 13 Settings             | the settings (19); `change_settings` and the sentence it refuses an empty week with             |

Four things the wireframes show that are not model state: the "moving"
marker during a reorder, the text of a title being edited, the number or
size being typed on a settings row, and the notes filter. The first three
are application state that becomes a command on Enter; the filter only
narrows a view.

## 19. Settings

Fourteen settings, held in the `settings` table and loaded into the model
as one typed value. The domain owns the type, its defaults, its
validation and its codec; storage only moves the rows and the settings
page only draws them.

| Key                  | Type and range                         | Default    | Page label            | What it does |
|----------------------|----------------------------------------|------------|-----------------------|--------------|
| `day_starts_at`      | hour, 0..=23                           | `5`        | Day starts at         | The hour the working day rolls over. 01:30 on Saturday belongs to Friday while it is 5. |
| `week_starts_on`     | `monday` or `sunday`                   | `monday`   | Week starts on        | Where the history's "this week"/"last week" rules fall, and the first column of the calendar and the weekday row of the repeat card. |
| `work_days`          | comma list of `mon`..`sun`, at least 1 | `mon,tue,wed,thu,fri` | Work days (seven toggle rows) | What "every work day" repeats on and what "next work day" on the move card means. |
| `review_opens_itself`| `true`/`false`                         | `true`     | Open the review on launch | Whether the morning review opens itself on the first launch of a day. Off, it is only opened with `M`. |
| `due_ahead_days`     | days, 0..=365                          | `0`        | Surface due tasks early | How many days before its due date a backlog task is put in front of you in the morning review. At 0 it surfaces on the due date and on every day after until it is dealt with; at 3 it also surfaces on the three days before. The row reads `on its day` at 0 and `N days before` otherwise. |
| `backfill_days`      | days, 0..=365; 0 means no cap          | `0`        | Catch up recurring tasks | A recurring task gets a fresh copy on every day its schedule names. After days away from the app, the copies for the days you missed are made on the next launch, each landing on the review pile. This caps how far back that goes: at 7, only the last week's missed copies are made and older ones are skipped for good. Every missed day makes them all. The row reads `every missed day` at 0 and `the last N days` otherwise. |
| `pile_horizon_days`  | days, 0..=3650; 0 means never          | `0`        | Hide pile tasks older than | An unfinished task from a day more than N days ago stays on its day but is left out of the pile and its count. 0 hides nothing. |
| `floating_window`    | `true`/`false`                         | `true`     | Floating window       | On Hyprland, whether the app opens in a centred floating window or tiles. Written to the Hyprland rule once the keys have gone quiet. |
| `window_size`        | `WxH` in logical pixels, 200..=10000 each | `870x650` | Window size         | The floating window's size in logical pixels. `h` and `l` step through five presets, named by the cells they give in foot with Omarchy's default font; Enter types any other. 870 by 650 is 120 by 36 cells, the size the screens are designed at. |
| `mouse`              | `true`/`false`                         | `true`     | Mouse                 | Whether the app takes the mouse. Off, the terminal's own text selection works again and the keyboard does everything. Takes effect at once. |
| `message_seconds`    | seconds, 0..=60; 0 means until the next key | `4`   | Hint bar messages stand for | How long "closed X · u undo" stays when no key follows. 0 keeps it until the next key. |
| `date_style`         | `locale`, `day_first`, `month_first`   | `locale`   | Date order            | `Fri 5 Sep` or `Fri Sep 5`, everywhere a date is written. `locale` follows the environment's locale (STACK.md section 8). |
| `confirm_delete`     | `true`/`false`                         | `false`    | Confirm before delete | `x` asks first instead of deleting and offering `u`. Applies to tasks, notes and the review pile. |
| `spell_check_notes`  | `true`/`false`                         | `false`    | Spell-check notes in US English | Underline possible US English spelling mistakes in notes. The application checks locally; note text is unchanged. |

### The window sizes

`window_size` may hold any pair in its range, but the page offers five,
which are the ones `h` and `l` walk. Each is a whole number of cells in
foot with Omarchy's default font, where a cell is about 7.02 by 17.28
pixels and 14 pixels of padding sit on each side (STACK.md section 7):

| Pixels    | Cells      |
|-----------|------------|
| 730x550   | 100 by 30  |
| 870x650   | 120 by 36, the default |
| 1010x755  | 140 by 42  |
| 1150x860  | 160 by 48  |
| 1290x960  | 180 by 54  |

A size that is one of the five is written with its grid; any other is
written `900x700 · custom`, because what a cell measures depends on the
font and the padding the terminal was started with, and only these five
were measured.

### The codec

Every value is text. A missing key or a value that cannot be read is that
setting's default, and an unknown key is ignored, so an older binary can
read a newer database's settings rather than refusing them. A number
outside its range is held to the range rather than refused: a typed 48
for the hour the day starts is 23.

### Validation

A `Settings` cannot hold a value outside the ranges above, so the only
thing left to refuse is a week with no work day in it, which
`change_settings` answers with "At least one day of the week must be a
work day." for the hint bar.

### Changing a setting

`change_settings(&Model, Settings)` is one write, `PutSettings`, and
nothing on the undo stack: like the system operations of section 12 it is
not undoable, and `u` after it takes back whatever it was that came
before. `date_style` is settled against the locale by the application,
which is the only part of the program that knows what a locale is; the
domain sees the resolved order in the `Context` of every command.

## 20. Personal dictionary

Personal words apply to every note. Matching normalizes to NFC, converts Unicode
letters to lowercase, and normalizes again. Display words retain their entered
capitalization. Thus a product name only needs one entry for ordinary case
variants; canonically equivalent accents also share an entry.

An entry is one word: outer whitespace is trimmed, internal whitespace and
control characters are rejected, and at least one alphabetic character is
required. Words are limited to 128 characters. Duplicate canonical keys are
refused, while editing an entry's display capitalization is allowed. Editing
and removing identify the entry by its original key rather than a list index.

Dictionary changes use domain validation and transactional storage, without
adding task undo entries. They never rewrite notes. The application reloads
entries along with the model and invalidates its spelling caches when they
change, including changes from another window. Removing an entry permits
Harper to flag that word again; built-in dictionary words remain accepted.

## 21. Compound command-line operations

The CLI uses the same commands and views as the UI, with complete-intention
operations layered over them. `apply_many` validates a sequence against an
in-memory working model and returns one change only when every command succeeds.
It combines the inverses in reverse order into one undo entry. Undoing a compound
entry either applies every inverse or drops the invalid entry without applying
any of it, following the existing stale-undo rule.

A full note replacement is a deliberate CLI action (`ReplaceNote`) with an
inverse restoring the previous body and update timestamp. UI typing continues
using `EditNote`, which does not push an entry for each keystroke. Settings,
dictionary and system operations keep their existing non-undoable semantics.

Public reorder positions are one-based among open tasks in a place. They are
translated into the internal sequence over all live tasks; completed task slots
and historical placements remain intact. Complete ordering requires exactly the
current open task IDs. CLI requests expose domain intentions, never serialized
internal inverse commands.
