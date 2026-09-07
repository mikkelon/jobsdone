# CLI implementation record

## Objective and decisions

Implement a complete noninteractive CLI alongside the existing terminal UI, using
its database, domain rules and undo history. The user selected JSON only and
explicitly dropped TOON. No arguments still open the TUI; `desktop` is retained.

- Human text by default; `--json` / `--format json` for versioned machine output.
  Success goes to stdout, errors to stderr. Stable exit statuses: 0 success,
  1 runtime/storage, 2 invalid request, 3 missing object, 4 domain rejection,
  5 concurrent conflict. Help, version and skill are plain-text discovery commands.
- JSON input through `--input FILE|-`; exact note text through `--stdin`/`--file`.
  Reject unknown fields, conflicting flags/body fields and operation spoofing.
- Commands express complete intentions: multi-property create/update, multi-ID
  close/reopen/move/delete, direct reorder before/after/position and full order.
  Validate atomically and create at most one undo entry per invocation.
  Positions count open tasks in a place, starting at one; full order requires
  exactly those IDs, preserving completed slots and history.
- Reads do not generate recurring copies or consume the morning review gate.
  `refresh` and `review start` are explicit. Resolve dates using the loaded
  working-day setting; return date context and recurrence-pending state.
- Shared guarded undo, strict settings validation, explicit deletion confirmation.
  SQLite coherent loads and whole-model comparison inside the write transaction
  reject stale changes, including changes to read dependencies and undo targets.
- Embed `skills/jobsdone/SKILL.md` in every binary, advertise `--skill` in help,
  and print it offline without a DB. No automatic installation or symlinks.
  Users can explicitly export it into their agent's recognized skill directory.
  The exported guide directs agents to the installed binary for current guidance.

## Environment and ownership

Original checkout: `/home/movergaard/projects/personal/jobsdone`, branch `main`,
initial HEAD `b659a2560cb3d213c4e6fb2a0d452a96ffb8d57d`.
Pre-existing spelling, navigation, rendering, docs and tests were snapshotted with
a temporary Git index without changing the original index or working files.
Snapshot: `7c1d3b01019ccdeb921da7582a6d58092d9445c8`, branch
`cli/implementation-base`. Delivery applies only the task delta to the original
working tree; existing user changes are not committed on main.

Orchestration: herdr in workspace wJ, caller pane wJ:p3. All implementation agents
used the user-selected **Claude Code + Opus 5**, exact model `claude-opus-5`,
`--permission-mode auto`. Installed Claude Code 2.1.263 was verified and used
through its installed executable PATH, avoiding the normal wrapper's mise update.
No model substitution, publishing or unrelated configuration changes.

| Agent | Branch / task-owned checkout | Workspace / pane | Ownership | Status |
|---|---|---|---|---|
| jd-foundation | cli/foundation / /tmp/jobsdone-cli-foundation | w15 / w15:p1 | domain grouping, storage, shared app commit/reload and tests | Complete; removed |
| jd-service | cli/service / /tmp/jobsdone-cli-service | w16 / w16:p1 | typed service requests, domain operations, DTOs and service tests | Complete; removed |
| jd-adapter | cli/adapter / /tmp/jobsdone-cli-adapter | w17 / w17:p1 | CLI parsing/help/rendering, main transport, subprocess tests | Complete; removed |
| Lead | cli/integration / /tmp/jobsdone-cli-integration | w18 / w18:p1 | contracts, module wiring, docs, bundled skill, review and validation | Complete; removed |

The lead supplied API and request contracts before dependent implementation.
The service owns date resolution against one loaded snapshot. The shared app
commit helper updates memory only after persistence. Domain `apply_many` builds a
single compound inverse compatible with persisted undo records. Note replacement
through the CLI is undoable; TUI autosave retains its existing undo semantics.
The lead unified the UI/service undo cap rather than keeping duplicate constants.

## Reviewed integrations

| Source | Integrated result |
|---|---|
| Foundation 7508fe4 | 4ab59dd; dependency picks e3ad148 in service and 3c35d13 in adapter |
| Foundation split 49eb620, 05b53b0, da9ccf1 and tests 13c73c4 | Merge 5e14ad1; verified split tree equals the initially shared 7508fe4 tree |
| Service 1174d72 | 88ab8f9; adapter dependency 15322ad |
| Service tests/fixes 5b75ba5 | afb56b6; adapter dependency 7b14684 |
| Adapter 6922beb | a80b7f9 |
| Foundation UI concurrency 25ae835 | Merged into cli/integration |
| Foundation note save correction 978786b | Merge e3c0a61 |
| Adapter subprocess tests/parser corrections 2adf36f | 757eeea |

Review strengthened row-level conflict detection to full snapshot equality,
protecting ordering calculations, settings dependencies and guarded undo. UI
`data_version` is sampled before loading, never advanced after an own commit,
so external writes cannot be marked as seen without being loaded. Clean open
note drafts follow external edits. Diverging unsaved text is saved as a recovery
note, preserving the externally edited original. Save decides and commits against
one model, including leave/quit without an intervening tick; a later competing
write is refused by storage. Regression tests cover these paths.

Adapter review fixed a `history list` alias-precedence bug, let JSON bodies supply
required fields, applied `--data-dir` to TUI launch, rejected commandless input
flags, and supplied JSON results for desktop operations. Original main parser
tests moved to the CLI module with the parser; executable behavior is covered
separately by subprocess tests.

## Validation completed

Required repository check, on the combined implementation in a private target:

```
env -u NO_COLOR CARGO_TARGET_DIR=/tmp/jobsdone-cli-integration/target make check
```

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo test`: 765 library tests passed, one pre-existing ignored; all 26 CLI
  subprocess tests passed; no failures. Doc tests passed (none present).
- Independent acceptance: 48 JSON invocations against temporary SQLite databases
  passed. Covers direct and full ordering with completed slots, multi-ID rollback,
  one-step compound undo, byte-exact Unicode/multiline notes and note undo,
  confirmation, settings rollback, schedules, recurrence/read separation, review,
  history/search/dictionary, invalid fields, guarded undo and offline skill output.
- Live TUI/CLI smoke: launched the freshly built binary in a private tmux session
  with scratch data and isolated config/state. An open TUI displayed CLI task
  creation and an externally replaced open note; quitting preserved CLI text.
  Session and scratch directory removed afterward.
- Bundled skill: skill-creator `quick_validate.py` passed using `/usr/bin/python`.
- `git diff --check`: passed.
- Before delivery, byte comparison confirmed every original snapshot file except
  the task's living plan still matched the original working tree, including the
  user's initially untracked files.

Earlier shared-target checks were not used as final evidence: concurrent agent
builds replaced artifacts. The private target removed that ambiguity. The tool
environment supplies `NO_COLOR=1`, which conflicts with an existing palette test;
final checks unset it without changing the application or that test.

Supporting agent checks included 68 new service tests, domain compound inverses,
real two-connection SQLite conflicts and snapshot coherence, UI recovery, and
CLI grammar tests. No production data or user agent directories were touched.
Live desktop-rule application and real clipboard writes were not exercised;
clipboard parsing and note lookup are covered, and desktop parsing is covered.

Detailed logs and independent scratch harnesses are retained under
`/tmp/jobsdone-cli-run/` for this session; committed subprocess and unit tests are
the repeatable repository checks. Public docs: `docs/CLI.md`,
`docs/CLI-REQUESTS.md`, README, and the bundled `skills/jobsdone/SKILL.md`.

## Delivery and cleanup

All three agents finished and exited. Their branches were checked for ancestry
or cherry-pick equivalence; no unique source changes were discarded. Temporary
module wiring and the adapter's local skill copy were backed up under
`/tmp/jobsdone-cli-run/` before removing their task scaffolding. Clean worktrees
w15, w16 and w17 were removed through herdr without force. Branches are retained
for provenance. No unrelated workspace or server was stopped.

Lead integration commit: `5c29ed2`. The reviewed delta was applied to main's
working tree without changing its index. Byte comparison against cli/integration
confirmed all delivered files match, including the preserved original user work.
`cargo clean -p jobsdone` removed obsolete shared build artifacts, then `cargo
build` succeeded in the original checkout. The fresh executable is
`target/debug/jobsdone`. All implementation and validation work is complete.
Final integration record commit: `76f29b5`. Clean integration workspace w18
was removed through herdr without force. Final worktree inventory contains only
the original main checkout; task branches remain for provenance. The rebuilt
main-checkout binary prints a skill byte-identical to its source asset.
