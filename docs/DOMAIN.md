# Domain

The model behind PRODUCT.md and DESIGN.md: the entities, their states,
the commands that change them, the invariants that must hold, and the
storage schema. The words used here are the words used in the code.

Everything a screen shows is a view of this model. Section 13 walks
through every wireframe and names the view it is drawn from.

## 1. Vocabulary

| Word            | Meaning                                                                  |
|-----------------|--------------------------------------------------------------------------|
| task            | A title, whether it is done, and where it lives. Nothing else.           |
| place           | Where a task lives: the backlog, or one day.                             |
| day             | A civil date. Days are not stored; a day exists when a task refers to it.|
| working day     | The date the program calls "today". It changes at 05:00, not midnight.   |
| position        | A task's index in the order of its place.                                |
| focus           | A task marked as one of the day's must-dos.                              |
| closed          | Done. A closed task remembers when it was closed.                        |
| open            | Not closed.                                                              |
| placement       | The record that a task was put on a day. Kept after the task leaves.     |
| moved           | A placement whose task is no longer on that day.                         |
| waiting         | A backlog task blocked on someone or something else.                     |
| due, remind     | Dates a task carries. They surface it; they never move it.               |
| surface         | Appear in the second step of the review, as a prompt.                    |
| pile            | Every open task on a day before today.                                   |
| schedule        | A repeat rule with a title. It creates copies.                           |
| copy            | A task created by a schedule for one date. Ordinary from then on.        |
| review          | The two-step morning pass over the pile and the surfaced tasks.          |
| note            | A plain-text scratchpad entry.                                           |
| command         | One change to the model. Every command has an inverse.                   |

## 2. Time

### Working day

The domain never reads a clock. The application passes an instant (a
zoned timestamp) into every command that needs one, and the domain
derives the date from it:

    working_day(instant) = civil date of (instant - 5 hours), local time

So 01:30 on Saturday belongs to Friday. `DAY_STARTS_AT = 05:00` is a
constant in the domain, not a setting. Every rule below that says "today"
means the working day of the instant the command was given.

Consequences:

- A task closed at 01:30 sits in Friday's Done group, showing 01:30.
- Due and remind dates are compared with the working day.
- A copy for date D is created when the working day reaches D.
- The review gate compares working days.

### Representation

Dates are civil dates (`jiff::civil::Date`), stored as `YYYY-MM-DD`.
Instants are zoned timestamps, stored as RFC 3339 text with offset, for
example `2026-09-05T08:12:00+02:00`.

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
- `on the pile`: open, and D is before today.
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
| Day list row              | done / kept, and "· n open" when open > 0            |
| Days with nothing planned | placed = 0; skipped in the list, shown empty when stepped to |

## 7. Views of the backlog

| Group    | Contents                              | Order    |
|----------|---------------------------------------|----------|
| ordinary | live, `day` none, open, not waiting   | position |
| Waiting  | live, `day` none, open, waiting       | position |

Header count: live open backlog tasks, and how many of them are waiting.

Below the two groups the backlog pane lists the live, unstopped
schedules by title and rule. That list is a view of section 10, not of
tasks.

## 8. Due and remind

Both are dates on the task and both stay on it through every move and
after closing. Only backlog tasks surface. When a dated task is pulled
onto a day it is already planned, so there is nothing to prompt; when it
is sent back to the backlog the date prompts again.

| Kind   | Surfaces when                                                                     |
|--------|-----------------------------------------------------------------------------------|
| due    | `day` none, open, not waiting, `due_on` ≤ today. Every review until acted on.      |
| remind | `day` none, open, `remind_on` in (previous review date, today]. Once.            |

"Previous review date" is the date of the last review started on a day
before today (section 11 keeps it). A reminder set for a Saturday is
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
| work days      | `{"kind":"workdays"}`                            | Monday to Friday. No holidays, ever.                     |
| every day      | `{"kind":"daily"}`                               |                                                          |
| weekly         | `{"kind":"weekly","weekdays":["mon","thu"]}`     | The listed weekdays, at least one.                       |
| monthly        | `{"kind":"monthly","day":15}` or `"day":"last"`  | Day N clamped to the month's last day; or the last day.  |
| every N weeks  | `{"kind":"every_n_weeks","n":2,"from":"2026-09-05"}` | `from` and every `7n` days after it. N ≥ 1.          |

"Every N weeks" is one weekday (that of `from`); "weekly" is a set of
weekdays. They do not overlap. The "Next work day" shortcut in the move
card uses the same Monday-to-Friday definition as the work-days rule.

`next_dates(rule, after, count)` is a pure function used both by the
repeat card's preview and by generation.

### Creating a schedule from a task

`R` on a task creates a schedule with the task's title and the chosen
rule, and links the task as the first copy:

- `scheduled_on` = the task's day if it is on one, else today.
- `generated_through` = that same date.

The task stays where it is. Its own date need not match the rule; it is
simply the first copy. The repeat card previews `next_dates` after
`generated_through`.

### Generation

`generate_copies(today)` runs on every launch, before the review. For
each live schedule with `stopped_on` none, for every scheduled date D in
(`generated_through`, today]:

- create a task with `title` = schedule title, `day` = D, position at
  the end of D, not focus, `schedule_id` and `scheduled_on` set, and a
  placement for D with `from_place = new`;
- then set `generated_through` = today.

There is no cap. Three weeks away means fifteen standup copies, each on
its own past day, all on the pile. PRODUCT.md promises this and the pile
is where the cost is meant to be seen.

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

Notes are deleted the same way.

### Undo

The undo stack is part of the data, one stack per database, shared by
every running instance and surviving restarts.

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
- Note body edits, copy generation, starting a review, and undo itself
  push nothing.
- After a reload (another instance wrote), the stack is reloaded with
  everything else.

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
| Reorder(task, position)  | live, position in range          | Moves the task within its place.                                                                       | Reorder(old position)                       |
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
| StopSchedule(schedule)         | not stopped                 | `stopped_on` = today.                                             | `stopped_on` none                           |
| GenerateCopies                 | system                      | Section 10. Not undoable.                                         | none                                        |

Deleting a schedule outright is not a command. The inverse of
CreateSchedule may delete the row because at that moment its only copy
is the task being unlinked.

### Review

| Command       | Precondition | Effect                                                                     | Inverse |
|---------------|--------------|----------------------------------------------------------------------------|---------|
| StartReview   | system       | If `review_on` ≠ today: `review_before` = `review_on`, `review_on` = today. | none    |

Every decision in the review (`d`, `t`, `b`, `m`, `x`, `k`, `w`, `space`)
is one of the task commands above, or nothing at all in the case of
`k keep`. The review adds no state to tasks.

### Notes

| Command               | Precondition | Effect                       | Inverse           |
|-----------------------|--------------|------------------------------|-------------------|
| CreateNote            |              | New empty note, `created_at` = now. | DeleteNote  |
| EditNote(note, body)  | live         | Sets body, `updated_at` = now. Not undoable. | none |
| DeleteNote(note)      | live         | `deleted_at` = now.          | RestoreNote       |

## 13. The review

### The pile

    pile = live, open, day < today

Grouped by day, newest day first; within a day, in position order. The
age shown is `today − day` in days, rendered relatively by the screen.
`was focus` on a pile row is the focus flag.

### Surfaced

Three groups, each computed at the moment the step is shown:

| Group               | Contents                                                        |
|---------------------|-----------------------------------------------------------------|
| Due                 | section 8, due; overdue first, then by `due_on`                 |
| Reminders           | section 8, remind; waiting ones dimmed                          |
| Also starting today | live copies with `scheduled_on` = today, shown for information  |

### Gate and session

The meta table holds two dates:

| Key            | Meaning                                                  |
|----------------|----------------------------------------------------------|
| review_on      | The working day a review was last started.               |
| review_before  | The value `review_on` had before that.                   |

On launch, after generation: if `review_on` ≠ today and the pile or the
surfaced set is non-empty, the review opens and StartReview runs. An
empty step is skipped; if both are empty nothing opens and the gate is
not written. Escape leaves the review with the pile intact. The review
can be started again any time from the command palette; that also runs
StartReview, which changes nothing on the same day.

The home screen's "n in review" is the live size of the pile, whatever
was or was not decided.

Which rows have been handled, the "2 of 7" progress, and the "→ today"
and "✓ closed" annotations are a **review session**: a value the
application holds in memory for the length of the review. It holds the
task ids the review opened with and the decision made for each. It is
not stored; closing the window mid-review forgets it, and the pile
itself is the record.

## 14. Search

    matches(text) = live tasks whose title contains text, case-insensitive

Results in two groups: open tasks (their place and flags beside them),
then closed tasks by place day, newest first. Each recurring copy is its
own row, marked `↻`. Enter goes to the task's place day. Notes are not
searched.

The empty result offers to add the typed text as a task on today.

## 15. Notes

| Field      | Type            |
|------------|-----------------|
| id         | integer         |
| body       | text            |
| created_at | instant         |
| updated_at | instant         |
| deleted_at | instant or none |

The list is ordered by `created_at`, newest first, and editing does not
move a note. The list row shows the first line of the body.

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

Notes on the schema:

- `tasks.day` NULL is the backlog. Position density is the domain's
  invariant, not the schema's (section 4).
- `tasks_copy` is what makes generation idempotent across instances.
- `placements` rows are inserted and, only by undo, deleted. Never
  updated.
- `undo_log.inverse` is the JSON of a command from section 12. Rows are
  popped by deleting them; the cap is enforced by deleting the lowest
  ids.
- `meta` keys: `review_on`, `review_before`.

## 18. Every screen as a view

The check PLAN.md asks for: each wireframe, and the parts of the model it
is drawn from.

| Wireframe               | Drawn from                                                                                      |
|-------------------------|-------------------------------------------------------------------------------------------------|
| 01 Review: the pile     | pile (13) grouped by day; focus flag; repeat chip; review session for handled rows and progress |
| 02 Review: surfaced     | due, remind, copies-today (13); waiting flag for dimming; review session for progress            |
| 03 Today + backlog      | day view for today (6); backlog view (7); pile size for the count; note count (15)              |
| 04 Half-width tile      | the same two views, one at a time                                                               |
| 05 Task states          | AddTask, EditTitle and the copy question, Reorder, SetFocus, Close, Move, DeleteTask and its undo label (12) |
| 06 Due, remind, waiting | SetDue, SetRemind, SetWaiting (12); chips and the Waiting group (6, 7)                          |
| 07 Repeat               | rule shapes and `next_dates` (10); CreateSchedule, SetRule, StopSchedule; the schedule list (7) |
| 08 History              | day view for a past day (6) including Moved; day list with counts (6)                            |
| 09 Search               | matches (14); place day; `↻` from `schedule_id`                                                 |
| 10 Scratchpad           | notes (15); CreateNote, EditNote, DeleteNote                                                    |
| 11 Palette and help     | the command list (12) and the key map in DESIGN.md; no model state                              |
| 12 Empty states         | every view above when its contents are empty; the empty search's AddTask                        |

Two things the wireframes show that are not model state: the "moving"
marker during a reorder and the text of a title being edited. Both are
application state that becomes a command on Enter.
