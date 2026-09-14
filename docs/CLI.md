# Jobsdone CLI

`jobsdone` opens the terminal app. Subcommands read or change the same local
database and exit. Run `jobsdone --help` for the command list, and append `--help`
to a command for its arguments and JSON request fields.

## Agent discovery

Every binary embeds [the jobsdone skill](../skills/jobsdone/SKILL.md).
`jobsdone --skill` prints its complete Markdown, including the `name` and
`description` frontmatter, without opening a database. `jobsdone --help`
advertises this entry point. A release therefore carries instructions matching
its implementation, available offline even when only the binary was installed.

There is no automatic skill installation and no symlink into an agent's settings.
An agent can read `jobsdone --skill` directly. To install the skill persistently,
choose the project or personal skill directory recognized by your agent, create
a `jobsdone` subdirectory, and save the output there as `SKILL.md`. Inspect any
existing file before replacing it. Re-export after upgrading jobsdone, or follow
the installed guide's instruction to read `jobsdone --skill` for current guidance.

This supports agents that discover tools through `--help`; it does not register
jobsdone automatically with an agent that has never inspected the CLI. Users who
want automatic skill selection can install the exported file explicitly.

## Data and output

The default database and `JOBSDONE_DATA_DIR` override are shared with the terminal
UI. `--data-dir PATH` selects a separate directory for one invocation. Use a
scratch directory when experimenting.

Default output is readable text. `--json` (equivalently `--format json`) selects
structured output. JSON is the only machine interchange format. Success responses
go to stdout; errors go to stderr and exit nonzero. Help, version and `--skill`
do not require database access.

Success responses have this envelope:

```json
{
  "schema_version": 1,
  "ok": true,
  "data": {},
  "context": {
    "today": "2026-09-07",
    "date_order": "day_first",
    "recurrence_pending": false
  }
}
```

The command determines the shape of `data`; [the JSON reference](CLI-REQUESTS.md)
documents each request and response. It is a public response object, not
the database's internal representation. Error responses have `schema_version`,
`ok: false`, and `error` with `code` and `message`.

| Exit status | Meaning |
|---|---|
| 0 | Success |
| 1 | Storage or runtime failure |
| 2 | Invalid command or input |
| 3 | Object not found |
| 4 | Domain rule rejects the operation |
| 5 | Concurrent change conflicts with the operation |

Use `--input FILE` or `--input -` to supply a command's JSON body. Unknown fields,
invalid types, and conflicting body/flag assignments are errors. Bodies contain
the selected operation's input fields; response objects are not request bodies.
Note commands separately accept raw text using `--stdin` and `--file`.

## Changes, order, and undo

IDs identify tasks, notes and schedules. Titles need not be unique. Multi-property
and multi-ID operations validate the whole request before writing, save atomically,
and create one undo entry for the logical invocation.

Reordering can name the final one-based `--position`, or an anchor task with
`--before` or `--after`. Positions count open tasks in the same place, including
focus/waiting groups but excluding completed tasks and screen headings. Complete
list ordering requires exactly the current open IDs, each once. History and
completed entries remain intact.

Undo history is shared with the UI. Read `jobsdone undo get`, then use
`jobsdone undo apply --entry ID` to avoid undoing an intervening action. Entry IDs
are never reused by the schema-4 allocator, even after undo empties the stack or
the application restarts. Reread undo state after upgrading: older versions may
already have recycled IDs, and the migration cannot reconstruct that history.
A stale ID returns conflict (exit 5) without changing data. Settings,
dictionary edits, recurrence refresh and review gate changes are not undoable.

Deletion respects `confirm_delete`: when enabled, explicitly pass `--yes`.
The CLI never waits for an interactive confirmation.

## Days, recurrence, and review

The working day uses the app's `day_starts_at` setting, default 05:00 local time.
`context.today` reports the date used for the operation. Machine dates are ISO;
relative date arguments are resolved against the working day.

Read operations do not generate recurring copies or consume the morning review
gate. `context.recurrence_pending` indicates whether catch-up is owed. Run
`jobsdone refresh` explicitly to create missed copies according to the settings,
then inspect the day or review. This is the headless equivalent of recurrence
generation on a UI launch.

`review get` inspects the pile and surfaced tasks. `review start` advances the
once-a-day gate. Resolve tasks through ordinary close, move and delete commands.
The UI's transient review cursor and progress markers do not become CLI state.

The database rejects changes computed from a stale snapshot while holding the
write transaction. A conflict saves no part of the request. Reload and reassess
before retrying; another window may have changed the order or the undo target.

## Window settings

Settings are saved in the database. To apply changed floating/window-size settings
to the desktop's window rule, run `jobsdone desktop`. Ordinary task and settings
queries do not require a display server.

An open TUI reloads changes made by the CLI. If both edit the same note,
the TUI preserves its unsaved text in a separate recovery note and keeps the
CLI version intact. It does not attempt to merge the two bodies.
