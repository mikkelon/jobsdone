# CLI implementation plan

## Objective and decisions

Implement a complete noninteractive interface to jobsdone's existing capabilities,
using the same database, domain rules, and undo history as the terminal UI.
`jobsdone` still opens the UI; `jobsdone desktop` remains supported. JSON only:
TOON was explicitly dropped by the user. Human-readable output is the default;
`--json` / `--format json` selects a versioned machine response.

Commands express completed intentions, including multi-property creation/updates,
multi-task moves, relative/absolute reordering, and complete list ordering. One
logical mutation commits atomically and creates at most one undo entry. Reject
ambiguous scope, unknown fields, invalid IDs, duplicate IDs, and incomplete full
orders. Positions are one-based within the open tasks in a place, not screen rows.
Support explicit JSON request bodies from files/stdin as well as ordinary flags.

Reads do not generate copies or consume the review gate. Explicit refresh generates
recurrences; review start advances the gate. Resolve today against the configured
working-day boundary. Use ISO dates and stable IDs in machine output. Support
guarded undo. CLI never unexpectedly prompts, opens an editor, or starts raw mode.
Deletion obeys confirm_delete through an explicit --yes option. Strengthen SQLite
concurrency for UI and CLI together so stale whole-row changes cannot overwrite
another process. Keep settings and dictionary validation in the domain.

## Execution environment

- Integration checkout: /home/movergaard/projects/personal/jobsdone, branch main.
- Initial HEAD: b659a2560cb3d213c4e6fb2a0d452a96ffb8d57d.
- Existing uncommitted spelling, word navigation, UI, documentation and test changes
  must be preserved. Snapshot them into a separate integration base without changing
  the original index or reverting any files; implementation commits stay separate.
- Agent kind: Claude Code (`claude`). Exact model: `claude-opus-5` (recognized in
  installed Claude Code 2.1.263). Permission mode: `auto`. No fallback authorized.
- Herdr context: HERDR_ENV=1, workspace wJ, caller pane wJ:p3.
- Installed executable verified directly; normal claude wrapper attempts a mise
  update. Use installed executable through a task-owned PATH entry in new panes.
- Repository guidance: docs/ARCHITECTURE.md has mechanically tested dependency
  boundaries. Update the table for new modules. Required check: make check.

## Public contract

CLI response: `{schema_version:1, ok:true, data:..., context:{today,...}}`.
Errors: `{schema_version:1, ok:false, error:{code,message}}`; success on stdout,
diagnostics/errors on stderr; JSON errors use the same schema. Exit codes: 0 success,
2 usage/input, 3 missing object, 4 domain rejection, 5 conflict, 1 storage/runtime.
No output-format autodetection. --input FILE (or - for stdin) supplies operation
fields for the selected command; flags and body must not silently override each
other. Raw note body has distinct --stdin/--file options.

Service boundary (owned by service agent):
`service::execute(store: &mut dyn Store, request: serde_json::Value,
now: &jiff::Zoned, dates: domain::DateOrder) -> Result<serde_json::Value, service::Error>`.
Error has public `code: String`, `message: String`, `exit_code: u8`.
Requests use `op` strings (task.list, task.get, task.add, task.update, task.close,
task.reopen, task.move, task.reorder, task.delete, day.reorder, day.get, backlog.get,
history.list, search, review.get, review.start, refresh, schedule.list/get/create/
update/stop/preview, note.list/get/create/update/delete/check, settings.get/set,
dictionary.list/add/update/delete, undo.get/apply). Document exact fields before
adapter implements dependent mappings. Service returns a complete success envelope.

Foundation boundary: domain::apply_many(&Model, Vec<Command>, &Context) produces
one Change/undo entry; use a serializable composite inverse compatible with existing
undo rows. Shared app::operations::commit_change(&mut dyn Store, &mut Model,
&Change) -> Result<(), StoreError> used by UI and service. Sqlite Store preserves
the existing trait API while atomically rejecting stale loaded snapshots on commit.

## Assignments and sequence

1. In progress: lead prepares snapshot, checks tools, establishes contracts and plan.
2. Pending: foundation agent owns src/storage.rs, src/storage/tests.rs,
   src/domain/command.rs, src/domain.rs, src/domain/tests.rs, src/app.rs and
   src/app/operations.rs. Atomic grouped commands/undo, stale-write protection,
   shared commit helper, focused concurrency and undo tests. No CLI/service edits.
3. Pending: service agent owns src/service.rs and src/service/**. Complete reads,
   typed validated requests, compound domain mutations, stable DTOs, settings,
   dictionary, recurrence/review, notes/spelling, guarded undo. Depends on foundation
   APIs above; may compile with temporary uncommitted module declaration until lead
   supplies integration wiring. Sends exact request field schema early.
4. Pending: adapter agent owns src/cli.rs, src/cli/**, src/main.rs, tests/cli.rs,
   Cargo.toml/Cargo.lock if needed. Parser, help, text/JSON rendering, stdin/file
   transport and subprocess acceptance tests. Depends on service request contract.
5. Pending: lead owns src/lib.rs module wiring, docs/ARCHITECTURE.md, README CLI
   documentation, docs/CLI.md, this plan, integration fixes and combined validation.

Per-agent branches, worktree paths, workspace/pane IDs, source/integrated commit
hashes and actual check results will be recorded at launch and handoff. Worktrees
are based on an explicit snapshot. No agent may merge, push, publish or edit
unrelated configuration. Each returns clean descriptive commits and test evidence.

## Acceptance and validation

- Full capabilities: tasks, schedules and previews, notes, planning/history/search,
  review/refresh, settings/dictionary, undo; existing desktop behavior retained.
- Atomic multi-field/multi-ID operations; failed validation saves nothing; one undo
  restores the entire operation. Full order requires exact membership.
- Read operations have no recurrence/review side effects. Help/version require no DB.
- Notes preserve Unicode/newlines through file/stdin; JSON bodies reject unknowns.
- JSON contains stable IDs, schema version and structured errors. No ANSI or chatter.
- Concurrent connections cannot lose edits or collide silently on generated IDs;
  bounded busy behavior. UI observes CLI changes through existing reload mechanism.
- Focused domain/storage/service tests, fresh binary subprocess acceptance suite,
  make check on integrated code, scratch database only. No production data touched.

## Integration and cleanup record

No agents launched yet. No implementation commits integrated. Validation pending.
No worktrees created yet. Preserve initial working-tree changes throughout.
