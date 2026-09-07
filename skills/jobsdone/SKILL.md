---
name: jobsdone
description: Read and manage tasks, daily plans, recurring schedules, scratchpad notes, and settings in the local jobsdone task manager through its CLI. Use when the user wants to work with jobsdone data.
---

# Jobsdone

Use the installed `jobsdone` CLI for application operations. Run
`jobsdone --help`, then the relevant subcommand's `--help` for its arguments and
JSON request fields. `jobsdone --skill` returns the guide bundled with that binary;
prefer it if this installed skill copy differs from the running version.

## Reading and making changes

- Use `--json` for structured results. Successful responses go to stdout; errors
  go to stderr with a nonzero exit status. Check `schema_version`, `ok`, and
  `error.code` before interpreting a response. An error is not an empty task list.
- Identify tasks, notes, and schedules by the IDs returned by reads. Titles can
  repeat. List positions describe order and are not object identities.
- Express the user's intended change in one invocation when supported: create a
  task with all its properties, update several properties, or move several IDs.
  A compound mutation is atomic and has one undo entry. Invalid requests save
  nothing.
- Reorder directly using `--before`, `--after`, or `--position`. Positions count
  open tasks in a place, starting at one. A complete list order must contain
  exactly that place's open task IDs. Read the list before constructing it.
- Use explicit JSON bodies via `--input FILE` or `--input -` for structured
  requests. Follow command help for the body's schema; output objects contain
  metadata and are not interchangeable with input requests. Do not send TOON.
- Use note `--stdin` or `--file` for raw multiline text. Preserve the text rather
  than compressing, trimming, or correcting it unless asked. For shell heredocs,
  quote the delimiter so note content is not expanded as shell code.

For example, a complete JSON task creation uses an explicit place object:

```sh
jobsdone task add --json --input - <<'JSON'
{"title":"Prepare demo","place":{"kind":"day","day":"today"},"focus":true}
JSON
```

A backlog place is `{"kind":"backlog"}`. On updates, an omitted due/reminder
field leaves it alone and `null` clears it. Ordinary flags are usually shorter
for a small change; JSON is useful when the request has several properties.

## Planning semantics

`today` is the configured working day, which defaults to rolling over at 05:00
local time. Use the response's `context.today` instead of assuming the calendar
date. Prefer ISO dates or the documented relative date expressions.

A task lives on a day or in the backlog. A due date is a deadline, a reminder is
a nudge, and neither schedules the task onto a day. Marking a day task as waiting
moves it to the backlog. Moving a waiting task onto a day clears waiting. Focus
belongs to tasks on a day.

Reads do not create recurring copies or start the morning review. The response's
`context.recurrence_pending` indicates whether recurrence catch-up is pending.
Use `jobsdone refresh` when the requested workflow calls for an up-to-date daily
plan, then read the plan. Refresh may create copies for missed days according to
settings. Use `review start` only when starting the review is part of the task;
use `review get` to inspect it without consuming the once-a-day gate.

The review pile is a view of unfinished tasks on past days. Resolve its tasks
through ordinary close, move, or delete operations. History retains moved tasks
on their former days. A recurring copy is an ordinary task; editing that copy
and editing its future schedule have different scopes. Specify the title scope
explicitly when changing future copies. Stopping a schedule keeps existing copies.

## Undo and conflicts

Undo history is shared with the terminal UI and other CLI invocations. Inspect
`jobsdone undo get` and use `jobsdone undo apply --entry ID` to guard against
undoing a newer, unrelated action. Settings, dictionary edits, refresh, and review
gate changes do not create task undo entries.

A conflict means another connection changed the loaded state. Read fresh state
and reassess the intended operation before retrying. Do not blindly repeat task
creation after an ambiguous process interruption; inspect whether it succeeded.

Deletion uses the app's confirmation setting. `--yes` explicitly confirms an
already requested deletion; the flag does not expand the user's requested scope.

For scratch work, set `JOBSDONE_DATA_DIR` or use `--data-dir` to select a separate
database. For normal work, use the user's existing database. Application data is
managed through the CLI; do not bypass its validation and undo rules with SQL.
