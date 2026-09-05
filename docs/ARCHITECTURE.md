# Architecture

How the code is divided: the modules, what each is responsible for, the
direction dependencies may point, the types and functions that cross each
seam, and the rules that keep the seams honest. Section 2 is read by a
test; the rest is read by people.

The shape in one paragraph. The domain is the deep module: it owns every
rule in PRODUCT.md and DOMAIN.md and imports nothing else in the crate. A
command is applied to an in-memory model and yields a `Change`, a list of
row writes. Storage is a dumb executor of changes against SQLite. Input
turns key presses into named actions that know nothing about tasks. The
application holds the state, asks the domain what a change would be, has
storage commit it, and applies it to its own model. Rendering is a pure
function of application state that returns where everything was drawn.
The terminal module owns the loop and the raw-mode side effects.

## 1. Modules

One crate, `jobsdone`, with `lib.rs` declaring six top-level modules and a
thin `main.rs`. Each module is `src/<module>.rs` plus, when it needs more
than one file, `src/<module>/*.rs`.

| Module     | Responsibility                                                                                                  |
|------------|-----------------------------------------------------------------------------------------------------------------|
| `domain`   | The model, every rule, commands and their inverses, the views of the model, rule dates, and the working day.     |
| `storage`  | The SQLite implementation of `Store`: migrations, loading the model, committing a change, reporting the version. |
| `input`    | A terminal event plus the current key context in, a named action out. Owns the key table.                       |
| `app`      | Application state, the launch sequence, reloading, turning actions into commands, and the screen layout.        |
| `ui`       | Drawing: application state in, a ratatui frame out, plus the layout of what was drawn.                          |
| `terminal` | Raw mode, alternate screen, mouse capture, the panic hook, and the event loop with its 250 ms tick.              |
| `main.rs`  | XDG paths, logging to the state directory, opening storage, running the terminal.                               |

Anything not on this list is not a top-level module. Helpers live inside
the module that needs them.

## 2. Allowed dependencies

The boundary test (section 6) parses this table. One row per module. The
"Internal" column lists the other top-level modules the row may name in a
`crate::` path; "Crates" lists the external crates it may use. `none`
means none. Names are separated by commas.

| Module     | Internal              | Crates                         |
|------------|-----------------------|--------------------------------|
| `domain`   | none                  | jiff, serde, serde_json        |
| `storage`  | domain                | rusqlite, jiff, serde_json     |
| `input`    | none                  | crossterm                      |
| `app`      | domain, input         | jiff, tracing                  |
| `ui`       | domain, app, input    | ratatui, jiff                  |
| `terminal` | app, ui, input        | crossterm, ratatui, tracing    |
| `main.rs`  | storage, app, terminal| jiff, tracing, tracing_subscriber, xdg |

What the table says, read as a picture, arrows pointing at what is
depended on:

    main.rs -> storage -> domain
    main.rs -> terminal -> ui -> app -> domain
                         ui -> input
                         terminal -> input
    main.rs -> app

Module names in the table may be wrapped in backticks; the test strips
them, and normalises `-` to `_` so a crate is written the way a path writes
it.

`main.rs` reads the clock once, at startup, for the `now` that `App::new`
takes; that is the only place outside `app` that names `jiff`.

Two absences are deliberate:

- `app` does not depend on `storage`. It holds a `Box<dyn Store>` that
  `main.rs` hands it, and only ever sees the trait. Nothing under `app`
  can name a SQLite type.
- `terminal` does not depend on `domain`. It moves events in and frames
  out; the application reads the clock and computes the working day.

## 3. The core seam: model, command, change, store

Everything the program does to its data goes through four domain types.

**`Model`** is the whole state as a value: every live and deleted task,
placement, schedule, note, the undo stack and the meta table, loaded from
storage in one go. It is small enough that loading it is well under a
millisecond (STACK.md section 3). Views are pure functions of a `Model`
and a date.

**`Command`** is one of the commands in DOMAIN.md section 12. It carries
task and schedule ids, never cursor positions.

**`Change`** is what a command does, as a list of row writes:
`Change { writes: Vec<Write> }`. The variants below are `Write`'s.

| Variant                     | Meaning                                       |
|-----------------------------|-----------------------------------------------|
| `PutTask(Task)`             | Insert or replace the whole row.              |
| `PutPlacement(Placement)`   | Insert; placements are never updated.         |
| `DeletePlacement(task, day)`| Only ever from the undo of a move.            |
| `PutSchedule(Schedule)`     | Insert or replace.                            |
| `PutNote(Note)`             | Insert or replace.                            |
| `PushUndo(UndoEntry)`       |                                               |
| `PopUndo(id)`               |                                               |
| `TruncateUndo(cap)`         | Delete the lowest ids beyond the cap.         |
| `SetMeta(key, value)`       |                                               |

Whole rows, not fields. A renumbered place is one `PutTask` per shifted
task. Phase 5 may add a variant; it may not add a second way to express
an edit that a whole-row put already expresses.

**`Store`** is the interface storage implements, defined in `domain`:

    trait Store {
        fn load(&self) -> Result<Model, StoreError>;
        fn commit(&mut self, change: &Change) -> Result<(), StoreError>;
        fn version(&self) -> Result<u64, StoreError>;
    }

    enum StoreError { Conflict, Other(String) }

`commit` applies every write of a change in one transaction. `version` is
`PRAGMA data_version`. `Conflict` is a unique or primary key violation;
everything else is `Other` with SQLite's message. `StoreError` implements
`Display`, which is how `main.rs` reports a failed open without naming the
type, and so without depending on `domain`. An in-memory `Store` for tests
is a `Model` and `Model::apply`, and it lives in `domain/tests.rs` as
`pub(crate)` so that every module's tests can drive an app through it.

### The path of one key press

1. `terminal` reads an event and asks `input` for an `Action`, passing
   `app.key_context()`.
2. `app.update(action)` resolves the cursor row to an id and builds a
   `Command`. It reads the clock once, here.
3. If `store.version()` differs from the version the app last saw, the
   app reloads the model first.
4. `domain::apply(&model, command, &now)` returns a `Change` or a
   `Rejected` with the sentence for the hint bar.
5. `store.commit(&change)`. On success `model.apply(&change)` and the
   version is re-read. On `Conflict` the change is dropped and the hint
   bar says another window changed things; on `Other` it says the change
   could not be saved and tracing gets the message. The model is not
   touched on failure.
6. `ui::draw(&app, frame)` returns a `Layout`; `terminal` passes it to
   `app.set_layout`.

### Ids

New rows get ids from the domain: the largest id in the model plus one.
Step 3 makes the window in which two instances could choose the same id a
few microseconds wide, and step 5's `Conflict` handling makes it harmless
when it happens.

### Where time comes from

Only `app` reads the clock, once per action, with `jiff::Zoned::now()`.
The domain receives an instant or a date and derives the working day
itself (DOMAIN.md section 2). `terminal` never sees time at all; a tick is
an action like any other.

## 4. Public surface of each module

What each module exports across a seam. A later phase that needs more
adds it here first, the way a new dependency is added to section 2 first.

### `domain`

- Types: `Model`, `Task`, `Placement`, `Schedule`, `Rule`, `Note`,
  `UndoEntry`, `Command`, `Change`, `Rejected`, `Store`, `StoreError`,
  and one result type per view: `DayView`, `BacklogView`, `Pile`,
  `Surfaced`, `SearchResults`, `DayList`.
- `apply(&Model, Command, now: &Zoned) -> Result<Change, Rejected>`: every
  user command. Pushes the undo entry as part of the change.
- `undo(&Model, now) -> Result<Change, Rejected>`: pops the top entry and
  returns its inverse's change with nothing pushed.
- `generate_copies(&Model, today) -> Change` and
  `start_review(&Model, today) -> Option<Change>`: the two system
  operations. Neither touches the undo stack.
- `Model::empty()` and `Model::apply(&mut self, &Change)`.
- Views, each `(&Model, ...dates) -> value`: `day_view`, `backlog_view`,
  `pile`, `surfaced`, `search`, `day_list`, `next_dates`, `working_day`.

### `storage`

- `Sqlite::open(path) -> Result<Sqlite, StoreError>`: opens or creates the
  database, sets the pragmas, applies pending migrations. Implements
  `Store`.

### `input`

- `Action`: the named actions. Cursor-relative (`Close` means the cursor
  row), never carrying an id. Includes `Tick`, `Resize`, `FocusGained`,
  the mouse actions `MouseDown`, `MouseUp`, `MouseDrag`, `Scroll` with
  cell coordinates, and in text fields `Insert(char)` and the editing
  keys.
- `KeyContext`: `Home { pane }`, `Notes { pane }`, `Review { step }`,
  `Popup(kind)`, each with a `text_field: bool` overlay. When the overlay
  is set, printable keys become `Insert` and only `Enter`, `Escape`,
  `Tab`, `Up`, `Down`, the editing keys and the `Alt` shortcuts keep a
  name.
- `action_for(Event, KeyContext) -> Option<Action>`.
- `bindings(KeyContext) -> &[Binding]`: the rows of the key table for a
  context, each a key, an action and a label. The hint bar, the command
  palette and the help overlay are drawn from these rows and from nothing
  else, so they cannot disagree with the dispatcher.

### `app`

- `App::new(Box<dyn Store>, now) -> Result<App, StoreError>`: loads the
  model and runs the launch sequence: `generate_copies`, then the review
  gate and `start_review` if the review opens.
- `App::update(&mut self, Action) -> Flow`, `Flow` being `Continue` or
  `Quit`. The single entry point for every event, ticks included.
- `App::key_context() -> KeyContext`.
- `Layout`: which pane and which row, with its task or note id, occupies
  which cell rectangle. `App::set_layout(Layout)` stores the last one and
  the mouse actions are resolved against it.
- Read access to the model and the application state for `ui`.

### `ui`

- `draw(&App, &mut Frame) -> Layout`.

### `terminal`

- `run(App) -> std::io::Result<()>`: enters raw mode, installs the panic
  hook, loops until `Flow::Quit`, restores the terminal. The error type is
  the terminal's own, not the store's: by rule 10 a failed commit becomes a
  hint inside the loop and never escapes it, so `terminal` has no reason to
  name a domain type and section 2 does not let it.

## 5. Seam rules

Each rule, and what breaking it looks like in a diff.

1. **The domain imports nothing from the crate and knows no I/O.** No
   rusqlite, ratatui, crossterm or tracing under `src/domain/`. Broken
   by: any of those names, or a file path, anywhere in the domain.
2. **The domain takes time as an argument.** Broken by: `Zoned::now()`,
   `SystemTime` or `Instant` anywhere under `src/domain/`.
3. **Screen contents are decided by the domain's views.** `ui` orders,
   filters and groups nothing that DOMAIN.md sections 6, 7, 13 and 14
   define; it formats and places what a view hands it. Broken by: a
   `sort_by`, or a test on `closed_at`, `focus` or `waiting` deciding
   membership of a group, inside `ui`.
4. **Rendering is pure.** `ui` takes shared references and returns a
   value. Broken by: `&mut App` or `&mut Model` in a signature under
   `src/ui/`, or a `Command` or `Change` constructed there.
5. **Actions carry no ids; commands carry no cursors.** `input` does not
   know what a task is and `app` never matches on a key code. Broken by:
   a task id inside `Action`, or `crossterm::event::KeyCode` anywhere
   under `src/app/`.
6. **Application state refers to tasks and notes by id, never by index.**
   After a reload the cursor re-resolves its id and, if the row is gone,
   clamps to the nearest one. The review session holds ids for the same
   reason. Broken by: a `usize` cursor into a list that came from the
   model.
7. **The app reloads before every dispatch, on every tick and on focus
   gained** when the stored version differs. Broken by: a command sent
   to `apply` without the version check in front of it.
8. **Uncommitted text lives only in the app.** A title being edited, the
   search string, the palette filter and a note body between keystrokes
   are application state. Each becomes one command on Enter; for note
   bodies phase 11 decides when. Broken by: a half-typed string reaching
   `apply`.
9. **The launch sequence belongs to `app`.** `main.rs` opens and migrates
   the database, builds the `App`, and calls `terminal::run`. Broken by:
   a domain call in `main.rs`.
10. **A failed commit is a hint, a failed open is an exit.** The model is
    left as it was, the failure is logged and shown, and the program
    keeps running. Failure to open or migrate the database exits with a
    message before the terminal enters raw mode. Broken by: a `panic!`,
    `unwrap` or `expect` on a `commit` result.

## 6. Enforcement

A unit test, `boundaries`, in `src/lib.rs` under `#[cfg(test)]`, runs
with `cargo test` and fails the build when a boundary is crossed. It
works from files, not from the compiler:

1. Parse the table in section 2 of this document, found by its heading.
   A missing or malformed table fails the test.
2. Read `[dependencies]` and `[dev-dependencies]` from `Cargo.toml`, with
   `-` normalised to `_`.
3. List the top-level modules: every `src/<name>.rs` and `src/<name>/`
   other than `lib.rs`, plus `main.rs`. A module not in the table fails
   the test.
4. For each module, scan every `.rs` file under it for path roots: every
   `crate::<module>` and every `<root>::` where `<root>` is a dependency
   from step 2. `std`, `core`, `alloc`, `crate`, `self` and `super` are
   never counted, nor is a module's path to itself. Bodies count, not
   only `use` lines: a fully qualified call is a dependency too.
5. Internal roots must appear in the module's Internal column. External
   roots must appear in its Crates column, except that a dev-dependency is
   allowed in a `tests.rs` file, since it cannot reach the binary.

Test code therefore lives in `src/<module>/tests.rs` and nowhere else. An
inline `#[cfg(test)] mod tests` would mean the scanner had to match braces
to know what is test code, which is the point at which reading Rust with a
scanner stops being reliable. The cost is that a unit test never sits beside
the function it tests.

Step 4 works on the source with comments, string and character literals
removed, so a path named in a doc comment or in an error message is not read
as a dependency. The whole check is a `cargo test` run, so it only reports on
code that compiles.

A second test, `seam_rules`, checks the four rules in section 5 that a
scanner can decide: rule 2 (no clock under `domain`), rule 4 (no `&mut` state
under `ui`), rule 5 (no `KeyCode` under `app`), and rule 10 (no `unwrap` or
`expect` on the same line as a `commit`, outside test files). The other six
rules are read by people.

Because the test reads this document, changing a boundary means editing
section 2, in the commit that needs it, with the reason in the message.

## 7. What later phases put where

| Work                                              | Module            |
|---------------------------------------------------|-------------------|
| A rule about tasks, days, order, dates, schedules | `domain`          |
| A new view, count or annotation on a screen       | `domain` (the view), `ui` (its formatting) |
| A migration or a change to how rows are written   | `storage`         |
| A new key, or a key that means something new      | `input`           |
| A popup, a page, cursor behaviour, the review session | `app`         |
| Colours, box drawing, date formatting, the collapse to tabs | `ui`    |
| Resize, focus, panic, the tick                    | `terminal`        |
| Paths, logging setup, the desktop pieces          | `main.rs`, the Makefile |
