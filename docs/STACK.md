# Stack

The tools the program is built from, one decision per section, with the
alternatives that were considered and why they lost. Every choice is
judged against DESIGN.md: instant start, colours from the terminal, mouse
and box drawing in the terminal, and a single binary a Hyprland keybind
can launch on an Omarchy machine.

Versions are not pinned here. `Cargo.lock` pins them.

## 1. Language: Rust

Rust on the stable toolchain from rustup, edition 2024. The program is one
crate named `jobsdone`, split into `lib.rs` and a thin `main.rs` so the
domain and storage can be tested without a terminal.

Rust produces a single static binary that starts in a few milliseconds,
has the strongest terminal UI toolkit available today, and its enums and
exhaustive matching fit a domain made of places, states and invariants.
Compile times are the price.

Rejected:

- **Go.** Single binary and fast start as well, with a mature terminal
  stack, but a weaker type system for a model built on states that must
  never be combined wrongly.
- **TypeScript on Node.** Closest to the previous version of the product,
  but Node adds 40 to 80 ms to every keybind press and a single binary
  needs extra packaging.
- **Python.** Textual is a good toolkit, but start time is the slowest of
  the four and single-binary packaging the most fragile.

## 2. Terminal UI: ratatui with crossterm

ratatui draws the screen; crossterm is its backend and reads keys, mouse
and focus events. The event loop is synchronous: it waits for the next
event with a 250 ms timeout, and every timeout is a tick. There is no
async runtime.

The program uses only the terminal's default foreground and background,
bold, dim, and the eight ANSI colours, so it follows the terminal's
Omarchy theme live with no code of its own. Mouse support, box drawing,
and the bold, dim and reverse attributes are all in crossterm.

Rejected:

- **tokio and async event streams.** The program has no network and no
  long-running work; a runtime adds start time and buys nothing.
- **termion or termwiz backends.** Smaller communities and no advantage
  over crossterm for this program.

## 3. Storage: SQLite

One SQLite database file, opened through `rusqlite` with its `bundled`
feature so SQLite is compiled into the binary and nothing has to be
installed. WAL journal mode. Schema migrations are numbered SQL files
applied on start; the schema itself is defined in phase 2.

Storage persists individual operations, each in its own transaction:
close task 42, insert a task at position 3 on a day, and so on. The
program never writes its state as a whole. This is what makes several
windows safe, and it is the reason a database won over a file.

Several instances of the program may run at once, for example one left
in a tile and one opened floating on another workspace. Each instance
runs `PRAGMA data_version` on every tick and on focus gained, and reloads
its state from the database when the value has changed. Reloading the
whole state is well under a millisecond at this product's scale.
Consequences the later phases carry:

- Recurring copy generation is idempotent, enforced by a unique
  constraint on schedule and date, so two instances generating the same
  day is harmless.
- Undo is a stack of inverse operations stored in the database, shared
  by every instance and surviving restarts. An inverse whose
  precondition no longer holds, because another instance changed
  something in between, is dropped with a message rather than applied.
- The morning review's once-per-day gate is a stored date, so a second
  instance skips a review the first has started.

Rejected:

- **A single JSON file rewritten atomically.** Human-readable and free of
  dependencies, but it has no story for two writers and every change
  would rewrite everything.
- **An append-only event log.** Matches "history is never rewritten"
  literally, but it is the most design work and history rows that are
  never updated give the same guarantee.
- **Enforcing a single instance** with a lock file and a Hyprland focus
  call. It removes staleness entirely, but pressing the keybind while a
  tile is open on another workspace would jump to that workspace instead
  of opening a window where you are.

## 4. Dates and times: jiff

`jiff` for every date and time. Its civil `Date` type is the domain's
notion of a day, and its zoned arithmetic handles local time and daylight
saving without ceremony, which matters for "every work day", "first of
the month", the date rolling over while the program is open, and the
time a task was closed.

Rejected:

- **chrono.** The incumbent, but time zone handling is bolted on and its
  civil date arithmetic is less direct.
- **time.** Solid, but no built-in time zone database.

## 5. Tests: cargo test, unit tests only

Every test runs with `cargo test`. The domain is tested as pure functions
over `Model` and `Change`; the storage module is tested against a
temporary database.

Module boundaries are checked by a unit test in the crate. It reads every
`.rs` file of each top-level module, not only its `use` lines, and fails
when a file names a module or a crate the allowlist forbids: a fully
qualified call in a body is a dependency too, and that is the form a
`use`-line check would miss. ARCHITECTURE.md owns the allowlist and its
section 6 owns the details.

Test code lives in `src/<module>/tests.rs` rather than in an inline
`#[cfg(test)] mod tests`, so the scanner never has to match braces to know
whether it is looking at test code. `main.rs` is the exception, because a
`src/main/` directory would read as a module of its own; what the command
line means is tested inline there, as the scanner's own tests are in
`lib.rs`.

Rejected:

- **Snapshot tests of rendered screens.** Would turn the wireframes into
  an oracle, but the maintenance cost is not worth it for a program used
  by one person who looks at it all day.
- **End-to-end tests in a pseudo-terminal.** Slow, flaky, and the unit
  tests cover the rules that matter.
- **A Cargo workspace with one crate per module.** The compiler would
  enforce the boundaries, but it is heavy for a program of this size.

## 6. Tooling and continuous integration

`rustfmt` and `clippy` with their defaults. A `Makefile` with:

- `make check`: format check, clippy with warnings denied, tests.
- `make fmt` and `make test`: the halves of it, on their own.
- `make run`: the program against a scratch database in `.dev`, so
  development never touches the real one.
- `make install` and `make uninstall`: see section 7.

GitHub Actions runs `make check` on every push. The repository is
`jobsdone` on the author's personal GitHub account, private for now and
written as if it were public.

Rejected:

- **justfile.** Nicer syntax, but `make` is on every machine already.
- **Local-only checks.** A hosted run on a clean machine is what catches
  a missing file or an untracked dependency.

## 7. Build and install

`scripts/install`, behind `make install`, installs for the current user
and runs again to update in place; `scripts/uninstall` takes everything
out and leaves the data. It works on any Linux and does more on Omarchy.

    scripts/install [--keybind [KEYS] | --no-keybind] [--floating | --tiled] [--size WxH]

`--keybind` binds KEYS, or `SUPER + SHIFT + J` when none are given;
`--no-keybind` binds nothing; with neither, the keybind is offered at the
prompt when a terminal is attached and skipped otherwise. The window flags
are settings rather than script arguments: they are passed to `jobsdone
desktop`, and a flag left off keeps the setting as it is, so a second
install does not undo what the settings page said.

Everywhere:

- The binary, with `cargo install --path . --root ~/.local`, so it lands
  in `~/.local/bin`. The desktop session that runs a keybind or a
  launcher entry has that directory on its PATH; it does not have
  `~/.cargo/bin`, least of all on a machine where Rust was installed for
  this program alone.
- A launcher entry at `~/.local/share/applications/jobsdone.desktop`
  with the icon from `assets/`. Off Omarchy it is a `Terminal=true`
  entry, which any desktop opens in its own terminal emulator.

On Omarchy, recognised by `/usr/share/omarchy` and `omarchy-launch-tui`:

- The launcher entry has the shape `omarchy-tui-install` writes:
  `xdg-terminal-exec --app-id=org.omarchy.jobsdone -e jobsdone`, so the
  user's default terminal opens it under an app id the window rule can
  match. `org.omarchy.<name>` is the id Omarchy gives every TUI it
  launches, and the same id is what `o.bind` with `{ tui = "jobsdone" }`
  produces.
- The keybind, in the install script's own block in
  `~/.config/hypr/bindings.lua`, the file Omarchy keeps for personal
  bindings, between `-- jobsdone: keybind (begin)` and
  `-- jobsdone: keybind (end)`:

      o.bind("SUPER + SHIFT + J", "Jobsdone", { tui = "jobsdone" })

  The line is written only when the keys are free, as `hyprctl binds`
  reports them (or the bindings file, when Hyprland is not running), and
  the person says yes at the prompt or passes `--keybind`. The keys asked
  about are the keys given: the script reads them into the modmask and key
  Hyprland answers with (SUPER 64, ALT 8, CTRL 4, CAPS 2, SHIFT 1), so any
  keys are checked as exactly as the default ones. A bind of our own is not
  somebody else's, and neither is one the person already has in this
  block, whose keys a plain re-install keeps.
- The window rule, in the program's own block, between
  `-- jobsdone: window (begin)` and `-- jobsdone: window (end)`:

      o.window("org.omarchy.jobsdone", { float = true, center = true, size = { 870, 650 } })

  870 by 650 pixels is 120 by 36 cells in foot with Omarchy's default
  font (JetBrainsMono Nerd Font 9) and 14-pixel padding, measured on a
  clean install. Take the padding off each side and a cell is about 7.02
  by 17.28 pixels, which is where the settings page's other four sizes
  come from: 730x550 is 100 by 30 cells, 1010x755 is 140 by 42, 1150x860
  is 160 by 48 and 1290x960 is 180 by 54 (DOMAIN.md section 19).
  Hyprland sizes in logical pixels, so the counts hold on a scaled
  monitor too. The rule is the `floating_window` and `window_size`
  settings written out (section 8), so the settings page changes it with
  no install at all; a tiled window is the two markers with nothing
  between them. `jobsdone desktop [--floating | --tiled]
  [--size WxH]` is what writes it, and the install script runs it once the
  binary is in place, passing on the window flags it was given. Writing
  reloads Hyprland and fails loudly if `hyprctl configerrors` has anything
  to say. From the settings page the rule is written when the keys have
  been quiet for a tick rather than on the keystroke, so a held key costs
  one reload; a floating window is then given the size on the spot, with
  `hyprctl dispatch 'hl.dsp.window.resize({ x = W, y = H, relative = false })'`
  and `'hl.dsp.window.center()'`, so the size being chosen is the size on
  screen. Hyprland reads a dispatch as a Lua expression, the form its own
  configuration binds keys with, and answers `ok` or the reason; the
  program trusts the answer, not the exit code, which is clean either way.

Some machines have one block instead, `-- jobsdone: begin` to
`-- jobsdone: end`, holding both. An install that finds it takes it out,
keeping the keys it bound, and writes the two blocks in its place;
`scripts/uninstall` removes all three.

foot is the reference terminal. alacritty, ghostty and kitty are reached
through the same `xdg-terminal-exec`, each installed and made the default
with `omarchy-install-terminal`, and all four float the app at 870 by
650 on a clean install. The grid differs with the font metrics:

| Terminal  | Cells at 870 by 650 |
|-----------|---------------------|
| foot      | 120 by 36           |
| alacritty | 120 by 36           |
| ghostty   | 119 by 38           |
| kitty     | 118 by 38           |

The app lays itself out for whatever grid it gets, so the last two lose a
column or two of the 120 the wireframes are drawn at and gain two rows.
The app id reaches alacritty only through Omarchy's own desktop entry for
it, which maps the flag to `--class=`; the stock entry has no mapping and
the window then keeps the class `Alacritty` and tiles.

Rejected:

- **A PKGBUILD.** The Arch-native way to install, but nobody else is
  installing yet. It can be added when someone is.
- **A release tarball with an install script.** Packaging work for no
  present user.
- **Hardcoding `foot` in the keybind.** It would allow
  `--window-size-chars=120x36` and an exact grid in any font, but it
  ignores the terminal the person chose, and the pixel rule gets the
  same grid in the default setup.
- **Omarchy's `TUI.float` app id.** It floats without any rule of ours,
  but at Omarchy's fixed 875 by 600, which is 121 by 33 cells, and one
  size for every TUI cannot be changed for this one.

## 8. Files on disk

The program follows the XDG base directory specification and has no
configuration directory: what can be configured is in the database, in
the `settings` table, which the settings page writes (DOMAIN.md section
19).

| Kind      | Path                             | Contents                        |
|-----------|----------------------------------|---------------------------------|
| data      | `$XDG_DATA_HOME/jobsdone/`       | the SQLite database             |
| state     | `$XDG_STATE_HOME/jobsdone/`      | the log file                    |

With the defaults these are `~/.local/share/jobsdone/` and
`~/.local/state/jobsdone/`. A backup of the data directory is a backup of
everything that matters, settings included.

The one file the program writes outside its own directories is the
Hyprland window rule, in `$XDG_CONFIG_HOME/hypr/bindings.lua`, between
its own markers. That file is Hyprland's rather than this program's; the
block in it belongs to the program, and the `floating_window` and
`window_size` settings are what it is written from (section 7).

The environment is read for four things:

| Variable                       | Effect                                      |
|--------------------------------|---------------------------------------------|
| `LC_ALL`, `LC_TIME`, `LANG`    | which way round dates are written, the first one set winning, unless `date_style` says outright |
| `HYPRLAND_INSTANCE_SIGNATURE`  | whether there is a Hyprland running to reload |
| `JOBSDONE_DATA_DIR`            | the data directory, in place of the XDG one |
| `JOBSDONE_LOG`                 | the log level, `info` if unset              |

The last two are for development and are not documented for users. The
log level is not read from `RUST_LOG`, so a variable set for some other
tool cannot change what this one writes to disk. The locale is read once,
at startup, in `main.rs`: month-first for the territories that write
dates that way (US, PH, FM, MH, PW, GU, PR, VI, AS, MP, UM), and
day-first for everything else, `C` and `POSIX` and an unset locale
included.

Rejected:

- **Everything under one directory.** One place to look, but the log
  would sit next to the data it is not.
- **A configuration file in `~/.config/jobsdone/`.** The most familiar
  place to put settings, but a second file format to design, parse,
  validate and keep in step with the database, for fourteen values that
  several open windows already follow each other through `data_version`
  to pick up. In the database they are one backup, one writer and one
  reload path.

## 9. Errors and logging

Errors and diagnostics are written with `tracing` to a file in the state
directory, started again once it passes a megabyte. A panic hook restores
the terminal before the panic message is printed, so a crash never leaves
the terminal in raw mode. Nothing routine is written to stderr; a program
launched from a keybind has no stderr anyone reads.

The exception is a failure to start: the XDG directories, opening or
migrating the database, or building the application. That message goes to
the log and to stderr, and when there is a terminal on the other end the
program waits for Enter before exiting, because a keybind's window would
otherwise close and take the message with it.

The commands that are not the app say what they did on stdout and exit:
`jobsdone desktop` prints what the window will do now, that it floats at
a size or that it tiles, and `--help` and `--version` print themselves. A
command line nobody can read goes to stderr with the usage and exits 2; a
window manager that would not take the rule exits 1 with what it said,
the settings having been saved before it was asked. A machine with no
Hyprland is not a failure: the settings are saved for when there is one,
the line says so, and the exit is 0. None of these enters raw mode, so
none of them waits for Enter.

## 10. Spell checking: harper-core

`harper-core` provides the note spelling dictionary and word parser. Its
bundled English dictionary works offline, with no runtime downloads or
external service. Default features are disabled because this app does not
need a thesaurus or concurrent dictionary storage.

The private `app::spelling` helper uses `PlainEnglish` to identify words,
URLs and email addresses, then asks `MutableDictionary` for word metadata
and American English membership. It normalizes a temporary checking copy
to NFC so composed and decomposed accents receive the same verdict. The
stored note stays unchanged; returned positions count grapheme clusters,
the same unit the caret and renderer use.

The application caches results for the displayed note and hides the word
under the editing caret. Drawing only reads these ranges. Empty notes and
a session that never checks notes do not initialize the dictionary.

The broader Harper pipeline is unnecessary for highlighting. Building a
`Document` invokes grammatical analysis; the `SpellCheck` linter also
computes correction suggestions. The FST dictionary accelerates suggestion
searches but adds initialization cost. Plain dictionary lookups provide
what this version displays.

Measured on this machine in release mode, dictionary initialization costs
about 105–110 ms once per process; subsequent short-note checks take
microseconds. The cold cost falls on the first nonempty note checked,
including the first typed character of a new note. Checking remains on the
main thread. These are local measurements, not timing guarantees.

Harper adds substantial build dependencies through `harper-brill` and its
Burn-based tagger, even though the app does not invoke the tagger. The
lockfile grows from 218 to 631 packages, including optional backends and
other platforms. The engine agent counted 157 additional compiled build
units on this target. The final integrated release binary is 6,470,248
bytes, versus 4,775,880 before this feature: an increase of 1,694,368 bytes.
Unused grammatical-analysis code can be removed by the linker, but its
compile dependencies still affect clean builds.

Alternatives considered:

- **Hunspell or Nuspell:** can be bundled offline, but require native build
  integration and separately selected dictionaries.
- **Spellbook:** avoids a native toolchain and can embed Hunspell dictionary
  files. Harper was selected for this English-only trial, accepting its
  larger build dependency tree.
- **Harper's full linter pipeline:** simpler to call, but performs grammar
  analysis and suggestion work that the notes editor does not display.

Known limits: words with an uppercase letter after the first are skipped
as identifiers or acronyms, including genuine typos written in capitals.
Proper-noun capitalization follows Harper's dictionary. Paired backticks
mark code-like text; this is not a full Markdown parser. Only note bodies
are checked, and no corrections are applied automatically.
