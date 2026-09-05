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

Every test runs with `cargo test`. The domain is tested against an
in-memory implementation of the storage interface; the storage module is
tested against a temporary database.

Module boundaries are checked by a unit test in the crate: it reads the
`use crate::` lines of each top-level module and fails when one names a
module the allowlist forbids. ARCHITECTURE.md owns the allowlist.

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
- `make install`: see section 7.

GitHub Actions runs `make check` on every push. The repository is
`jobsdone` on the author's personal GitHub account, private for now and
written as if it were public.

Rejected:

- **justfile.** Nicer syntax, but `make` is on every machine already.
- **Local-only checks.** A hosted run on a clean machine is what catches
  a missing file or an untracked dependency.

## 7. Build and install on Omarchy

`cargo install --path .` puts the binary in `~/.cargo/bin`. `make install`
runs that and also installs the desktop pieces: the Hyprland window rule
on the app id `jobsdone`, and a keybind that runs

    foot --app-id jobsdone -e jobsdone

Omarchy configures Hyprland in Lua, so the snippet is Lua and is written
in phase 12. The program never positions or sizes its own window.

foot is the reference terminal. alacritty and ghostty are checked in
phase 12 and supported as far as they behave the same. The install is
aimed at the author's own machine.

Rejected:

- **A PKGBUILD.** The Arch-native way to install, but nobody else is
  installing yet. It can be added when someone is.
- **A release tarball with an install script.** Packaging work for no
  present user.

## 8. Files on disk

The program follows the XDG base directory specification and has no
configuration directory, because it has no configuration.

| Kind      | Path                             | Contents                        |
|-----------|----------------------------------|---------------------------------|
| data      | `$XDG_DATA_HOME/jobsdone/`       | the SQLite database             |
| state     | `$XDG_STATE_HOME/jobsdone/`      | the log file                    |

With the defaults these are `~/.local/share/jobsdone/` and
`~/.local/state/jobsdone/`. A backup of the data directory is a backup of
everything that matters.

Rejected:

- **Everything under one directory.** One place to look, but the log
  would sit next to the data it is not.
- **`~/.config/jobsdone/`.** The most familiar per-app directory, but the
  specification reserves it for configuration, and this program has none.

## 9. Errors and logging

Errors and diagnostics are written with `tracing` to a file in the state
directory. A panic hook restores the terminal before the panic message is
printed, so a crash never leaves the terminal in raw mode. Nothing is
written to stderr on purpose; a program launched from a keybind has no
stderr anyone reads.
