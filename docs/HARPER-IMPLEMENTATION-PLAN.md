# Harper spell checking implementation plan

## Objective and scope

Introduce offline US English spell checking in notes using Harper. Enable it
by default and provide a persistent settings toggle. Highlight spelling
issues without changing note text automatically. Add on-demand suggestions and
explicit replacement at the note caret. Personal dictionaries and additional
languages remain follow-up features. Render every text-input caret as a full block.

## Follow-up: suggestions and block caret

- **Complete — Engine:** delegate an on-demand, bounded US English suggestion
  API to an Opus 5 Claude agent in auto mode. Keep fuzzy work out of typing.
- **Complete — Interaction:** delegate a discoverable Alt+s picker for the
  misspelled word at the caret, arrows to choose, Enter to replace, Escape to
  cancel. Preserve Unicode offsets, autosave, and guard stale replacements.
  Change all slim text-input carets to a full terminal-cell block in the same UI task.
- **Complete — Integration:** reviewed and combined `06edbe5` → `a19775a` and
  `b409531` → `35a7d35`; updated user docs and all caret wireframe examples.
- **Complete — Validation:** ran required checks and tmux-only UAT for selecting,
  applying and cancelling corrections, persistence, and the block caret.

Follow-up results:

- `make check` passed: formatting, clippy with warnings denied, 505 library
  tests and 7 binary tests; one pre-existing ignored test. Integration fixed
  two stale hint expectations and the new popup test's border matcher.
- Independent Opus review confirmed replacement offsets, autosave and popup
  routing. Corrected its finding about a misleading first-search cost comment.
  Its global-caret concern is explicitly requested scope; its range hardening
  suggestion had no reachable failure under the current draft/cursor invariant.
- Tmux scratch session `jobsdone-suggestions-uat`: `recieved` offered `received`;
  Down/Escape preserved the original, Enter applied and autosaved, and corrections
  survived restart. `teh` offered `the` at 80×44; correct-word feedback worked.
- Captures verify full-block carets in notes, task entry, search, palette,
  settings, and the go-to-date input. Picker screenshot inspected visually:
  `uat/out/suggestions-picker.png`; narrow capture `suggestions-narrow.txt`.
- Release build passed. No VM used. Scratch session stopped and previous harness
  state restored. Generated HTML retains its existing fixed-grid whitespace.

The original highlighting phase below is complete; this follow-up responds to
the user's request for actionable corrections and a cursor matching its width.

## Orchestration

The primary agent owns this plan, task boundaries, integration, and final
validation. Implementation is delegated to Claude Code agents in herdr
worktrees, using Opus 5 by default. Fable 5.1 is permitted for unusually
complex tasks. Confirm the locally supported herdr commands and model IDs
before launching agents; do not silently substitute other models. Start every
Claude Code agent with explicit `--permission-mode auto`, as requested.

## Work sequence

1. **Complete — Discovery:** read repository guidance, inspect herdr and
   Claude Code capabilities, and define interfaces from current editor and
   settings code.
2. **Complete — Harper feasibility:** establish a pinned dependency/API,
   spelling-only US dialect behavior, text-position semantics, and a small
   realistic corpus. Check dependency/build impact.
3. **Complete — Parallel implementation:** delegate the spelling service and
   settings persistence/UI to separate herdr worktrees with explicit file
   ownership and acceptance criteria.
4. **Complete — Editor integration:** delegate caching and highlighted rendering
   once the service contract is settled. Preserve Unicode, wrapping, scrolling,
   caret placement, and note autosave. Suppress incomplete-word noise and
   avoid checking obvious URLs/email addresses.
5. **Complete — Review and integration:** inspect agent diffs, integrate changes,
   and resolve conflicts. Update user documentation and relevant design docs.
6. **Complete — Validation:** run formatting, clippy, and tests (`make check`),
   plus focused checks for default/missing settings, toggle persistence,
   spelling behavior, and rendering offsets. Assess startup/build footprint
   and note-edit responsiveness where feasible.
   Acceptance testing uses only the tmux-based `uat/tui` harness with scratch
   data. There is no Omarchy VM on this machine; do not attempt VM testing.

## Acceptance criteria

- Existing and new databases default to spell checking enabled.
- The settings page can persistently disable and re-enable checking.
- Only spelling diagnostics for US English appear in the note body.
- Checking runs locally and needs no runtime service or dictionary download.
- The checker does not alter stored note text or interrupt typing/autosave.
- Results are reused across redraws; disabling checking clears highlights.
- Tests cover meaningful editor and settings regressions; required checks pass.

## Agent assignments

| Agent | Worktree / branch | Model | Status |
|---|---|---|---|
| `harper-suggest-engine` | `/tmp/jobsdone-harper-suggestions-engine` / `harper-suggestions-engine` | Opus 5, auto | Complete: `06edbe5`, on-demand suggestions API and engine tests |
| `harper-suggest-ui` | `/tmp/jobsdone-harper-suggestions-ui` / `harper-suggestions-ui` | Opus 5, auto | Complete: `b409531`, correction picker and app-wide block caret |
| `harper-engine` | `/tmp/jobsdone-harper-engine` / `harper-engine` | Opus 5, auto | Complete: `4d347c6` → `6b4b530`; 41 engine tests and full checks passed |
| `harper-settings` | `/tmp/jobsdone-harper-settings` / `harper-settings` | Opus 5, auto | Settings `fa17739` → `5c7ff96`; wireframes `1e5d6b0` → `590abd3`; independent draft review complete |
| `harper-editor` | `/tmp/jobsdone-harper-editor` / `harper-editor` | Opus 5, auto | Complete: `286fb9d` → `f99a282`; real-engine validation now running centrally |

## Decisions, findings, and blockers

- Scope follows the agreed first version: highlighting and a default-on toggle.
- User explicitly authorized Claude Code delegation via herdr worktrees.
- `herdr` and `claude` executables are available locally.
- Working tree was clean before this plan was added.
- Herdr caller context is verified (`wJ:p3`). Socket access requires sandbox
  escalation, which was approved for the requested orchestration.
- Claude Code recognizes `claude-opus-5` and displays Opus 5.
- Keep Harper inside an application helper; rendering remains pure and only
  consumes cached diagnostics. Update the architecture dependency allowlist.
- Service contract requested: lazy `SpellChecker::default()` and
  `check(&mut self, text: &str)` returning ranges in grapheme-cluster indices.
  Renderer and draft caret already use grapheme clusters. Setting contract:
  private setting field with getter `spell_check_notes()` and setter
  `set_spell_check_notes(bool)`, and database key `spell_check_notes`.
- README, product, design, and domain documentation now describe the agreed
  behavior; reconcile exact labels and rendering behavior after integration.
- Settings uses a dedicated Notes group and the label "Spell-check notes in
  US English". No new key binding or database schema migration is needed.
- The tmux screenshot renderer previously ignored SGR underline on/off.
  Added underline/reset handling and verified HTML output with a small smoke
  check so acceptance screenshots can show the new annotation.
- Harper 2.8.0 is the crate being evaluated, with default features disabled.
  Its resolved dependency tree is large; engine agent is investigating which
  dependencies actually compile and measuring impact before final acceptance.
- Measured dictionary initialization: FST about 205 ms, plain
  `MutableDictionary::curated()` about 105 ms on this machine. Use the latter
  for lookup-only highlighting. Keep it lazy until a nonempty note is checked;
  no launch prewarm or async worker in this version. Report the one-time cost
  as a trial limitation. Warm checks measured in microseconds.
- Review findings sent back to implementation agents: preserve the agreed range
  API; do not mask sentence-final words as code; avoid repeated backtick scans;
  update diagnostics for direct settings changes; skip offscreen ranges when
  rendering; preserve lazy initialization for empty notes.
- Wireframes now include the fourteenth setting and sixth group, explaining
  scrolling to Notes. The initial 120×36 viewport remains unchanged.
- Independent review found decomposed accents could be split into false
  misspellings. Engine agent is adding normalization of the checking copy to
  NFC, preserving original stored text and grapheme-based ranges.
- Review's absent-tests/scratch-files notes described intermediate drafts:
  editor now has 14 focused tests and its temporary stub is excluded from the
  commit; verify all temporary engine probes are removed at integration.
- Heuristic limitations retained for this trial: all-caps identifiers/acronyms
  are skipped; paired backticks mark code, so stray delimiters can affect which
  prose is checked. No full Markdown interpretation is promised for plain notes.
- Integrated both engine and editor commits. Git accepted both module declarations
  without a text conflict; the combined compile caught the duplicate. Kept one
  private `mod spelling` declaration and reran the checks.
- Reuse `/tmp/jobsdone-harper-engine/target` for final builds to avoid compiling
  the large dependency tree twice. Root source is still what gets validated.
- All pre-existing dependency versions remain present in the resolved lockfile;
  no existing package was upgraded away merely to introduce Harper.

## Validation results

Baseline format and clippy passed. Baseline library tests: 412 passed,
1 ignored, 1 terminal-color test failed because the tool environment sets
`NO_COLOR=1`. Verify that test with `NO_COLOR` unset and use the same environment
for final terminal-color checks; no source change is needed for this setting.
The failing baseline test passed in isolation with `NO_COLOR` unset.
Baseline release binary (fresh build): 4,775,880 bytes; lockfile: 218 packages.
A comparison copy is preserved at `/tmp/jobsdone-before-harper`.
Settings agent: `env -u NO_COLOR make check` passed. Added regressions for
default/missing/invalid values, SQLite roundtrip, keyboard changes, reopening,
and visible setting value/description. Editor agent may apply the settings
commit in its worktree to test against the real interface.

Integrated validation: `env -u NO_COLOR
CARGO_TARGET_DIR=/tmp/jobsdone-harper-engine/target make check` passed:
formatting, clippy with warnings denied, 471 library tests and 7 binary tests;
one pre-existing test remains ignored. All editor tests ran against real Harper.

Tmux acceptance (`jobsdone-harper-uat`, scratch database
`/tmp/jobsdone-harper-uat-data`) passed eight captured-attribute checks:

- Active unfinished `mispeling` unmarked; separating space adds underline.
- Wrapped note flags exactly `teh` and `recieved`; URLs, email, and both NFC
  and NFD forms of `naïve` remain unmarked.
- UI toggle off clears annotations; restart preserves off.
- Re-enabling restores both annotations, including at 80×44.
- Moving inside `recieved` hides only that word's annotation.

SQLite checks confirm off/on persistence and byte-for-byte preservation of the
typed note, including decomposed accents. The test tmux session was stopped and
prior harness state restored. Captures are in `uat/out/harper-*.{txt,ansi,html}`.
The harness's configured Playwright module is missing; text/ANSI capture still
works. Rendered `harper-enabled.html` with installed Chromium into
`uat/out/harper-enabled.png` and visually inspected the underlines.

Final integrated release build passed. Binary: 6,470,248 bytes versus
4,775,880 baseline (+1,694,368 bytes, about 1.7 MB). Both integrated debug
and release binaries are copied into the usual root `target/` paths.

## Completion

All planned implementation, integration, review, documentation and available
acceptance checks are complete. Three Opus 5 Claude Code agents were used,
all in herdr worktrees and explicit auto mode; Fable was not needed. The
agents and worktrees remain available for inspection. No deployment or desktop
configuration changes were performed. Main remaining tradeoffs are the large
build dependency tree, the approximately 0.1-second first-use initialization,
and the documented dictionary/filtering limitations.
