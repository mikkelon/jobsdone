# Spellcheck terminal checks — 7 September 2026

The app now requests a red curly underline for spelling marks. The note text
keeps its normal foreground. The normal underline is set before the styled
underline extension, allowing terminals that ignore the extension to retain
the straight line. Spell checking defaults to off; explicitly saved choices
are preserved.

## Live application checks

Launched the debug build in each installed emulator with its existing terminal
configuration, an isolated SQLite database, and a PTY input/output driver.
No personal notes or terminal configuration files were changed. `NO_COLOR`,
which is set in the agent's command environment, was removed for these checks.

| Terminal | Installed version | Actual TERM | Application checks |
|---|---|---|---|
| foot | 1.27.0-2 | xterm-256color | Passed |
| Kitty | 0.48.2-1 | xterm-kitty | Passed |
| Ghostty | 1.3.1-2 | xterm-ghostty | Passed |
| Alacritty | 0.17.0-1 | xterm-256color | Passed |

For each emulator:

- Created a note containing `This is teh mistayk in a note.`; confirmed there
  was no saved preference and no curly underline output while checking was off.
- Enabled checking through Settings using keyboard input; verified the saved
  preference and red underline colour (`58;5;1`) and curly style (`4:3`) in the
  actual app's terminal output.
- Opened `Alt+s` on `teh`, accepted the correction, and verified SQLite contained
  `This is the mistayk in a note.`.
- Disabled checking in Settings and confirmed the saved preference was false,
  then enabled it again. A subsequent foot launch retained the enabled choice.

These live behavior checks were followed by screenshot inspection in all four
emulators, confirming red squiggly underlines with unchanged text colour.

## Emulator responses and visual verification

A separate DECRQSS query asked each emulator to report its active rendition
after setting a red curly underline:

| Terminal | Response | What it establishes |
|---|---|---|
| foot | `0;4:3;58:5:1m` | Emulator confirms curly shape and red palette slot |
| Kitty | `0;58:5:1;4:3m` | Emulator confirms curly shape and red palette slot |
| Ghostty | `0;4m` | Underline confirmed; response does not establish shape or colour |
| Alacritty | No response within two seconds | Query is inconclusive |

After the user approved desktop capture, Omarchy's fullscreen screenshot command
succeeded. Each terminal was focused in turn and its rendered note inspected.
All four visibly show a red squiggly underline under `mistayk`; the neighbouring
words and the note text keep their normal colour. The inconclusive Ghostty and
Alacritty query responses above therefore do not indicate a rendering failure.

Local screenshot details, cropped to the test note and enlarged four times,
are saved under `uat/out/spellcheck-terminals/`:

- `foot.png`
- `kitty.png`
- `ghostty.png`
- `alacritty.png`

The crops exclude unrelated desktop content. All temporary test windows were
closed after verification. No rendering changes were needed after these checks.

## Automated validation

`env -u NO_COLOR make check` passes: formatting, Clippy with warnings denied,
620 tests passed, one existing test ignored. Coverage includes default-off without
loading the dictionary, saved settings, underline colour and text foreground,
wrapping, wide and combining characters, caret placement, attribute reset,
unchanged-frame diffing, and removal of marks after a correction.

The renderer adapter is private to `terminal`; it supplements Ratatui's normal
changed-cell rendering and only redraws cells carrying red spelling underlines.
It does not depend on TERM identifying the emulator, which matters for the local
foot and Alacritty configurations above.
