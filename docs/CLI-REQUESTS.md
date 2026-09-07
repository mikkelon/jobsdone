# CLI JSON requests and responses

The command selects the operation: for example, `jobsdone task update --input -`
accepts the fields listed under `task.update`. Supply the operation fields as a
JSON object on stdin or in a file. The CLI inserts `op`; if a body includes it,
it must match the selected operation. Unknown fields and conflicting flag/body
assignments are errors. `--json` selects the response format independently.

These examples use the internal operation name to identify the matching CLI
command. They describe the public schema, not serialized domain undo commands.
See [the CLI guide](CLI.md) for workflow semantics and command-line examples.

## 1. Envelope

    {
      "schema_version": 1,
      "ok": true,
      "data": { ...per operation... },
      "context": {
        "today": "2026-09-07",
        "date_order": "day_first",
        "recurrence_pending": false
      }
    }

- `context.today` is the working day of `now` under `settings.day_starts_at`,
  as an ISO `YYYY-MM-DD` date. It is computed **after** the operation, so a
  `settings.set` that moves `day_starts_at` reports the new day.
- `context.date_order` is `"day_first"` or `"month_first"`: the resolved
  order, which is the `dates` argument `execute` was given while `date_style`
  is `locale` and the setting otherwise. It is on every response so that a
  caller honours the setting without asking `settings.get` for it.
- `context.recurrence_pending` is `true` when `domain::generate_copies` would
  write something for `now` against the post-operation model. It is a pure
  computation: reads never generate. Run `refresh` to act on it.

## 2. Request

A JSON object. `op` selects the operation; the remaining fields are that
operation's. **Every request struct is `deny_unknown_fields`**: an unknown or
misspelled field is an error, never ignored. Fields marked optional may be
omitted entirely.

Scalar formats:

| Kind    | JSON                                                              |
|---------|-------------------------------------------------------------------|
| id      | integer ≥ 1 (`task`, `note`, `schedule`, `undo` ids are all this) |
| date    | `"YYYY-MM-DD"`, or one of `today` `yesterday` `tomorrow` `next-work-day` |
| instant | RFC 9557 zoned timestamp, `"2026-09-05T08:12:00+02:00[Europe/Copenhagen]"` |
| place   | `{"kind":"backlog"}` or `{"kind":"day","day":"2026-09-07"}`       |
| rule    | section 6                                                          |

**The service resolves dates itself**, against the same model and instant it
executes with, so that nothing else has to load the settings or read a clock to
know what "today" means. Every date field takes, case-insensitively:

| Text                              | Means                                             |
|-----------------------------------|---------------------------------------------------|
| `2026-09-07`                      | That date.                                        |
| `today`                           | The working day of `now` under `day_starts_at`.   |
| `yesterday`, `tomorrow`           | The day either side of it.                        |
| `next-work-day`, `next_work_day`  | The next day after today that `work_days` names, which is `next_dates(Rule::Workdays, today, 1, work_days)` — the same call the move card makes. |

Anything else is `invalid_argument`, with the accepted forms in the message.

## 3. Errors

| code                    | exit | Meaning                                                                    |
|-------------------------|------|----------------------------------------------------------------------------|
| `invalid_request`       | 2    | Not an object, no `op`, unknown `op`, unknown field, wrong type, missing required field. |
| `invalid_argument`      | 2    | Well-typed but wrong: bad date, duplicate/extra/missing id, position out of range, two mutually exclusive fields, a title scope that does not apply. |
| `confirmation_required` | 2    | `task.delete` / `note.delete` while `settings.confirm_delete` is on and `confirm` was not `true`. |
| `not_found`             | 3    | The named task, note, schedule or dictionary entry does not exist (or is deleted). |
| `rejected`              | 4    | `domain::Rejected`. `message` is the domain's own sentence.                 |
| `conflict`              | 5    | `StoreError::Conflict`, or `undo.apply` with an `expected_id` that is not the top entry. |
| `storage_error`         | 1    | `StoreError::Other`.                                                        |

Validation happens before anything is committed. A rejected request writes
nothing at all, including the multi-id and compound operations.

## 4. Objects the service returns

### Task

The full row. Returned by `task.get`, `task.list` and every task mutation.

    {
      "id": 12,
      "title": "Write the report",
      "place": {"kind":"day","day":"2026-09-07"},
      "position": 2,
      "open": true,
      "focus": false,
      "waiting": false,
      "closed_at": null,
      "due_on": "2026-09-10",
      "remind_on": null,
      "created_at": "2026-09-05T08:12:00+02:00[Europe/Copenhagen]",
      "schedule": {"id": 3, "scheduled_on": "2026-09-07", "rule": {"kind":"daily"}}
    }

- `position` is **one-based among the open live tasks of the place**, which is
  what `task.reorder` and `day.reorder` speak in. It is `null` for a closed
  task: closed tasks keep an underlying slot, but the Done group is ordered by
  `closed_at`, so no public position is offered for one.
- `schedule` is `null` unless the task is a copy.

### Row

What a domain view hands a screen, with the annotations the UI draws. Returned
by `day.get`, `backlog.get`, `search`, `review.get`, `review.start`. It is a
different shape from Task on purpose: a view row carries derived state that a
bare task row does not.

    {
      "id": 12,
      "title": "Write the report",
      "place": {"kind":"day","day":"2026-09-05"},
      "closed_at": null,
      "focus": false,
      "waiting": false,
      "due": {"on":"2026-09-10","overdue":false},
      "remind": null,
      "repeat": {"kind":"daily"},
      "was_focus": false,
      "on_the_pile": true,
      "still_open": false,
      "from_backlog": false,
      "closed_on_this_day": false
    }

`due`, `remind` and `repeat` are `null` when unset.

### Schedule

    {
      "id": 3,
      "title": "Standup",
      "rule": {"kind":"workdays"},
      "generated_through": "2026-09-07",
      "stopped_on": null,
      "stopped": false,
      "created_at": "...",
      "copies": 12
    }

`copies` counts the live tasks linked to the schedule.

### Note

    {"id": 4, "body": "…", "created_at": "…", "updated_at": "…"}

List rows drop `body` for `first_line` unless `include_body` is asked for.

### Undo entry

    {"id": 42, "label": "Closed \"Write the report\"", "at": "…"}

Every **mutating** response carries `"undo"`, the entry the operation pushed, or
`null` where it pushed none (`settings.set`, the dictionary operations,
`refresh`, `review.start`, and a reorder that was already in that order).

## 5. Operations

Reads never write. `refresh` and `review.start` are the only operations that
write system state, and neither pushes an undo entry.

### Reads

| op             | Request fields                                                       | `data`                                                     |
|----------------|----------------------------------------------------------------------|------------------------------------------------------------|
| `task.list`    | `place?`, `state?` (`"open"`\|`"closed"`\|`"all"`, default `"all"`), `focus?: bool`, `waiting?: bool` | `{"total": n, "tasks":[Task]}` — days in date order, the backlog after them, each place in its own order |
| `task.get`     | `id`                                                                 | `{"task": Task}`                                            |
| `day.get`      | `day?` (default today)                                               | `{"day":"…","focus":[Row],"plan":[Row],"done":[Row],"moved":[Row],"counts":{"planned":n,"open":n,"done":n,"moved":n}}` |
| `backlog.get`  | —                                                                    | `{"ordinary":[Row],"waiting":[Row],"schedules":[{"id":n,"title":"…","rule":rule}],"open":n,"waiting_count":n}` |
| `history.list` | `limit?` (≥ 1, newest days first)                                    | `{"stretches":[{"stretch":"this_week","days":[{"day":"…","kept":n,"done":n,"open":n}]}],"total_days":n}` — `stretch` is `later`\|`this_week`\|`last_week`\|`earlier` |
| `search`       | `text`                                                               | `{"open":[Row],"closed":[Row],"total":n}`                   |
| `review.get`   | —                                                                    | section 8                                                 |
| `schedule.list`| `include_stopped?: bool` (default `false`)                           | `{"total":n,"schedules":[Schedule]}`                        |
| `schedule.get` | `id`                                                                 | `{"schedule": Schedule, "next": ["2026-09-08", …]}` — the next 3 dates after `generated_through` |
| `schedule.preview` | exactly one of `rule` or `schedule`; `after?` (date, default the schedule's `generated_through`, or today for a bare rule); `count?` (1..=50, default 3) | `{"rule": rule, "after":"…", "dates":["…"]}` |
| `note.list`    | `include_body?: bool` (default `false`)                              | `{"count":n,"notes":[{"id":n,"first_line":"…","created_at":"…","updated_at":"…","body"?:"…"}]}` — newest `created_at` first; `body` only when asked for |
| `note.get`     | `id`                                                                 | `{"note": Note}`                                            |
| `note.check`   | exactly one of `note` (id) or `text`; `suggestions?: bool` (default `false`) | section 9                                          |
| `settings.get` | —                                                                    | `{"settings": Settings, "date_order":"day_first"}`           |
| `dictionary.list` | —                                                                 | `{"count":n,"words":[{"key":"acme","word":"Acme"}]}` — by key |
| `undo.get`     | —                                                                    | `{"entry": UndoEntry \| null, "depth": n}`                   |

### Task mutations

**`task.add`** — `title` and `place`, plus any of:

| Field     | Type                | Meaning                                              |
|-----------|---------------------|-------------------------------------------------------|
| `focus`   | bool                | Needs a day place.                                    |
| `waiting` | bool                | Needs the backlog place.                              |
| `due`     | date                |                                                       |
| `remind`  | date                |                                                       |
| `repeat`  | rule                | Gives the new task a repeat schedule, as `R` does.    |
| `position`| integer ≥ 1         | Where in the place it lands, one-based among open tasks. |
| `before`  | id                  | Immediately before that open task instead.            |
| `after`   | id                  | Immediately after it.                                 |

At most one of `position`, `before`, `after`. The whole intention is one change
with one undo entry: nothing is written if any part of it is refused.
`data` = `{"task": Task, "schedule": Schedule | null, "undo": UndoEntry}`.

`waiting: true` with a day place is `invalid_argument`: the domain would move
the task to the backlog rather than refuse, and a silent move is not what the
request said. `focus: true` on a backlog place is left to the domain, which is
`rejected` (exit 4) with "Focus is for tasks on a day."

A flag already saying what it is asked to say earns no command: adding a
backlog task with `focus: false` or `waiting: false` states where it stands
rather than being refused for a rule it is not breaking. The same holds for
`task.update`, which makes it idempotent.

**`task.update`** — `id`, and any of:

| Field         | Type                      | Meaning                                                        |
|---------------|---------------------------|-----------------------------------------------------------------|
| `title`       | string                    | The new title.                                                  |
| `title_scope` | `"this"` \| `"future"`    | Required when `title` is given **and** the task is a copy; forbidden otherwise. `"future"` renames the schedule too. |
| `place`       | place                     | Moves the task. Clears `waiting`, as a move does.               |
| `focus`       | bool                      |                                                                 |
| `waiting`     | bool                      | `true` on a task that is on a day moves it to the backlog, which is what the UI's `w` does. Combined with `place: day` it is `invalid_argument`. |
| `due`         | date \| `null`            | Absent leaves it, `null` clears it, a date sets it.             |
| `remind`      | date \| `null`            | The same.                                                       |
| `position`    | integer ≥ 1               | One-based among the open tasks of the place the task ends in.   |

At least one of them is required. They are applied in the order of the table and
commit as one change with one undo entry.
`data` = `{"task": Task, "undo": UndoEntry}`.

**`task.close`**, **`task.reopen`**, **`task.move`**, **`task.delete`** — `ids`,
a non-empty array of distinct ids. `task.move` also takes `place`; `task.delete`
also takes `confirm?: bool`. All four are atomic: one change, one undo entry, and
a rejection anywhere writes nothing.
`data` = `{"count": n, "tasks":[Task], "undo": UndoEntry}`.

Closing a backlog task moves it onto today first, as the domain does.
`task.delete` is refused with `confirmation_required` when
`settings.confirm_delete` is on and `confirm` is not `true`.

**`task.reorder`** — `id` plus **exactly one** of:

- `before`: id — the task lands immediately before that open task;
- `after`: id — immediately after it;
- `position`: integer ≥ 1 — one-based among the open tasks of the place.

The reference task must be live, open and in the same place. `position` must be
in `1..=(open tasks in the place)`.
`data` = `{"place": place, "order":[Task], "undo": UndoEntry}` — the open tasks
of the place in their new order.

**`day.reorder`** — `place` plus `ids`, the **exact** set of open live task ids
of that place, in the order wanted. A duplicate, a missing id, an extra id, an
id from another place, or a closed/deleted id is `invalid_argument` and nothing
is written. Despite the name it takes the backlog as happily as a day.
`data` = `{"place": place, "order":[Task], "undo": UndoEntry}`.

Closed tasks keep their exact underlying slots: `Model::place` is one dense
sequence over **all** live tasks of a place, so the public order over open tasks
is mapped onto the slots the open tasks already occupy. A day's history is never
rewritten by a reorder.

### Schedules

| op                | Fields                | `data`                                        |
|-------------------|-----------------------|------------------------------------------------|
| `schedule.create` | `task`, `rule`        | `{"schedule": Schedule, "task": Task, "undo": UndoEntry}` |
| `schedule.update` | `id`, and `title` and/or `rule` (at least one) | `{"schedule": Schedule, "undo": UndoEntry}` |
| `schedule.stop`   | `id`                  | `{"schedule": Schedule, "undo": UndoEntry}`    |

### Notes

| op            | Fields                       | `data`                                  |
|---------------|------------------------------|------------------------------------------|
| `note.create` | `body?` (string, default "") | `{"note": Note, "undo": UndoEntry}`      |
| `note.update` | `id`, `body`                 | `{"note": Note, "undo": UndoEntry}` — one deliberate replacement, so unlike the TUI's per-tick save it is undoable |
| `note.delete` | `id`, `confirm?: bool`       | `{"note": Note, "undo": UndoEntry}`      |

`body` is full text: newlines and any Unicode, unchanged. `note.create` with a
body is one change and one undo entry ("Added a note").

### Settings and the dictionary

**`settings.set`** — `settings`, an object of one or more of the fourteen keys in
section 7. Unknown keys and values outside the documented range are
`invalid_argument`: the request is refused rather than clamped, unlike the
tolerant database codec. Not undoable.

    "data": {
      "settings": Settings,
      "date_order": "day_first",
      "desktop": {
        "window_rule_changed": true,
        "floating_window": true,
        "window_size": {"width": 870, "height": 650}
      },
      "undo": null
    }

`desktop.window_rule_changed` is `true` when `floating_window` or `window_size`
actually moved. **Neither the service nor the adapter writes a window rule here.**
The settings are saved like any other; the flag exists so the adapter can say
that `jobsdone desktop` will hand them to Hyprland. Writing the rule stays that
one command's job.

**`dictionary.add`** — `word`. **`dictionary.update`** — `key`, `word`.
**`dictionary.delete`** — `key`. `key` is matched through
`domain::dictionary_key`, so any capitalisation or Unicode composition of the
stored word finds the entry. `data` = `{"word":{"key":"…","word":"…"},"undo":null}`
(for `delete`, the entry that was removed). Not undoable.

### System and undo

**`refresh`** — no fields. Runs `generate_copies`. Pushes nothing.
`data` = `{"count": n, "created":[Task], "undo": null}`.

**`review.start`** — no fields. Runs `start_review`, which is a no-op on a day it
has already run. Pushes nothing.
`data` = `{"started": bool, ...the whole of `review.get`'s data...,  "undo": null}`.

**`undo.apply`** — optional `expected_id`. When given it must equal the id of the
top entry, or the request is `conflict` (exit 5) and nothing is written. Scripts
that read `undo.get` and then act should pass it; a bare `undo.apply` takes
whatever is on top.

    "data": {
      "applied": true,
      "entry_id": 42,
      "label": "Closed \"Write the report\"",
      "task": 12,
      "dropped": null,
      "depth": 6
    }

`dropped` is the domain's sentence when the inverse no longer applied: the entry
is popped, nothing else changes, and `applied` is `false`. `undo.apply` on an
empty stack is `not_found` (exit 3).

## 6. Rule

The five shapes of DOMAIN.md section 10, in the JSON the schedule row stores.
The service validates them itself rather than leaning on the storage codec:
unknown fields, unknown `kind`, an empty or repeating `weekdays`, a monthly day
outside `1..=31`, and `n < 1` are all `invalid_argument`.

    {"kind":"workdays"}
    {"kind":"daily"}
    {"kind":"weekly","weekdays":["mon","thu"]}
    {"kind":"monthly","day":15}
    {"kind":"monthly","day":"last"}
    {"kind":"every_n_weeks","n":2,"from":"2026-09-05"}

Weekdays are `mon tue wed thu fri sat sun`.

## 7. Settings

Values are typed, not text. Out-of-range is refused.

| Key                   | JSON                                            | Range                    |
|-----------------------|-------------------------------------------------|--------------------------|
| `day_starts_at`       | integer                                         | 0..=23                   |
| `week_starts_on`      | `"monday"` \| `"sunday"`                        |                          |
| `work_days`           | array of weekday strings                        | 1..=7 distinct           |
| `review_opens_itself` | bool                                            |                          |
| `due_ahead_days`      | integer                                         | 0..=365                  |
| `backfill_days`       | integer                                         | 0..=365, 0 = no cap      |
| `pile_horizon_days`   | integer                                         | 0..=3650, 0 = never      |
| `floating_window`     | bool                                            |                          |
| `window_size`         | `{"width": 870, "height": 650}`                 | 200..=10000 each         |
| `mouse`               | bool                                            |                          |
| `message_seconds`     | integer                                         | 0..=60                   |
| `date_style`          | `"locale"` \| `"day_first"` \| `"month_first"`  |                          |
| `confirm_delete`      | bool                                            |                          |
| `spell_check_notes`   | bool                                            |                          |

`date_order` beside them in a response is the resolved order, `"day_first"` or
`"month_first"`: the `dates` argument `execute` was given when `date_style` is
`locale`, and the setting otherwise.

## 8. review.get / review.start data

    {
      "pile": {
        "total": 4,
        "days": [{"day":"2026-09-05","age":2,"rows":[Row]}]
      },
      "surfaced": {
        "total": 2,
        "due": [Row],
        "reminders": [Row],
        "also_starting_today": [Row]
      },
      "gate": {
        "review_on": "2026-09-05",
        "review_before": "2026-09-04",
        "previous_review": "2026-09-05",
        "started_today": false,
        "opens_itself": true,
        "would_open": true
      }
    }

`would_open` is DOMAIN.md section 13's launch condition: `opens_itself`, the gate
is not today's, and the pile or the surfaced set has something in it. Reading it
never consumes the gate; `review.start` is what advances it.

## 9. note.check data

    {
      "misspellings": [
        {"start": 12, "end": 15, "word": "teh", "suggestions": ["the","ten"]}
      ],
      "spell_check_notes": false
    }

`start` and `end` are offsets in **grapheme clusters** from the start of the
text, end exclusive, which is the unit a note's caret counts in. `suggestions`
is present only when the request asked for it. The check runs whatever
`spell_check_notes` says — the setting governs the UI's underlines, not an
explicit request — and the setting is reported alongside. The personal
dictionary is applied. Nothing is written and no note text changes.

## 10. Tested behaviour

`src/service/tests.rs` drives `execute` one request at a time against an
in-memory store, the way the command line does. Sixty-eight tests, all
passing, over: the envelope and the working day; every error code and its
exit status; the four date words; the compound add and its single undo;
title scope on a recurring copy; multi-id atomicity; `confirm_delete`;
positions over a place with a closed task between two open ones; the full
order's exact membership; the four views; rule validation; note text through
Unicode and newlines; grapheme-cluster spelling ranges and the personal
dictionary; strict settings validation; canonical dictionary keys; that five
read operations neither generate nor spend the gate; and guarded undo.

## 10. Boundaries

- No clipboard. The raw note body for `wl-copy`/`xclip` is the adapter's own
  path, separate from this JSON.
- No desktop side effect; `settings.set` reports what changed. Run
  `jobsdone desktop` explicitly to apply the window rule.
- No date parsing of typed shapes, no flag parsing, no human rendering, no ANSI.
- No implicit generation and no implicit review gate on any read.
- No reload and no retry. One invocation loads the model once and commits
  once; a `Conflict` from storage is exit 5 and the caller decides.
