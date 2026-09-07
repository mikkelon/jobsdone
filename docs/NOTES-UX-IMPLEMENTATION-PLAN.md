# Notes editor movement implementation plan

## Objective and acceptance

Fix notes editing so the viewport stays steady while the caret moves within
visible rows, Up/Down follow soft-wrapped screen rows and retain a preferred
display column, and mouse clicks place the caret at the displayed character.
Preserve grapheme boundaries, autosave, spelling, and other text fields.
Check long notes, short intervening rows, explicit newlines, wrap boundaries,
Unicode, clicks after scrolling, and terminal resizing.
User refinement: harden window resizing during notes editing. Verify both width
and height changes, crossing split-pane/narrow layouts, caret visibility after
reflow, and immediate keyboard/mouse correctness on the new geometry. Include
resize at a wrap boundary and short preferred-column targets in regression tests.

## Run configuration

- Integration branch: `main`; clean initial checkout.
- Explicit base: `2a5650f3047b4d478a3c4102ff8027742c705303`.
- Herdr environment verified: `HERDR_ENV=1`, caller workspace `wJ`.
- Coding agent: Claude Code 2.1.263; model `claude-opus-5` (user selected
  Claude Code with Opus 5); permission mode `auto`. Installed binary includes
  this exact model identifier and CLI supports auto mode.
- Authorized model alternatives: none.
- No repository AGENTS.md found.

## Findings and contracts

Rendering is pure; application state owns persistent viewport and movement state.
Caret positions count grapheme clusters; screen columns count terminal cells.
Originally the UI wrapped in `src/ui.rs`, recalculated scrolling from the caret
on every draw, and had no note-character hit map. `App::step_the_caret` used hard
newline starts. The implementation puts the shared mapping in `app::wrap` and
adds `unicode_width` to the application's documented dependencies.

Review findings relayed during implementation: include the reserved caret cell
in the mouse hit area; clicks below content go to the note end; full-width rows
must retain a visible caret and all their characters; clicking from preview into
editing must retain the clicked viewport. A one-cell text reserve and minimal
documented fixture updates are authorized within this UX change.

## Sequence and assignments

1. Discovery and acceptance preparation: complete.
2. Shared note layout, persistent viewport, keyboard and mouse implementation:
   complete. One agent owned the coupled application, UI and regression tests.
3. Lead review complete; integrated source `9bde2b43066fc7f4cf7a6b41e8112d059b3f2684`
   as `610166d` on `main`. All concrete review findings resolved.
4. Combined `make check`, fresh binary build, and scratch terminal acceptance:
   complete. Lead ran final acceptance and inspected visual captures.
5. Verify patch equivalence, clean worktree and idle processes; remove task-owned
   herdr worktree: complete. Patch equivalence verified with `git cherry`;
   clean worktree removed without force after the agent exited.

Agent `notes-caret-ux`: branch `fix/notes-caret-ux`, checkout
`/tmp/jobsdone-notes-caret-ux`, workspace `wY`, pane `wY:p1`.
Owns application, UI, relevant input/terminal plumbing, regression tests and
necessary architecture documentation. Lead reserves this plan and acceptance
artifacts. Source `9bde2b4`; integrated `610166d`.

## Validation and completion record

Repository harness: `make check` runs formatting, clippy with warnings denied,
and tests. `uat/tui` uses tmux with scratch data. Lead acceptance uses isolated
tmux socket `jobsdone-notes-ux-accept`, session `notes`, and scratch data
`/tmp/jobsdone-notes-ux-accept-data`; existing UAT state remains untouched.
Driver: `/tmp/notes-ux-accept.py`; captures: `uat/out/notes-ux/`.

Baseline: `env -u NO_COLOR cargo test -q` passes 565 library and 7 binary tests,
1 ignored. Plain `cargo test` fails the existing palette test because the host
sets `NO_COLOR=1`; unsetting it restores expected terminal-colour output.
Live baseline reproduces all three defects: caret remains on bottom row,
viewport shifts on each Up, soft-wrap Up skips rendered rows, and mouse edits
do not reach the clicked character.

Agent handoff: formatting, clippy with warnings denied, and tests reported green
(606 library tests passed plus 7 binary tests, 1 ignored). Home/End now use visual
row edges; preferred columns persist through resize. Windows too small to draw a
note have no available geometry and fall back to hard-line navigation until drawn.
Lead validation on the combined implementation:

- `env -u NO_COLOR make check`: passed formatting, clippy with warnings denied,
  606 library tests and 7 binary tests; 1 existing ignored test.
- `env -u NO_COLOR cargo build -q`: passed; acceptance used this fresh binary.
- All 21 live acceptance checks passed: steady viewport, visual-row movement,
  preferred columns through short rows, scrolled and preview clicks, Unicode
  combining-character insertion, caret visibility at full-row boundaries, and
  Up/click behavior after resizing to 80×24, 160×36, 60×14 and 120×12.
- Acceptance harness corrected to handle a two-row note area and use terminal
  cell coordinates for Unicode instead of the duplicate list-preview text.
  These were harness assumptions; corrected checks passed without code changes.
- Visually inspected `scroll-after.png` and `final-inspection.png` in
  `uat/out/notes-ux/`. Chromium rendered the existing harness HTML because its
  configured Playwright installation path was unavailable.
- No real note data or existing UAT state was changed. Task tmux sessions stopped.
  Source branch retained; its worktree/workspace removed. `git worktree list`
  now shows only the integration checkout. Reusable build cache remains outside
  the removed checkout at `/tmp/jobsdone-notes-build`.
- Lead corrected two documentation phrases after integration; no code changes
  followed the passing final checks. No unresolved implementation findings.
