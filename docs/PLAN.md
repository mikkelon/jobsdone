# Plan

The road from PRODUCT.md and DESIGN.md to a working program, in phases.
Each phase ends in one deliverable: a markdown document or working code in
this repository. Phases are meant to be worked on one at a time, in a
conversation dedicated to that phase, which fills in the details and
produces the deliverable. This document stays high level on purpose.

## How to use this document

- Start a phase by pointing an agent at this file and the phase number.
  The inputs to every phase are PRODUCT.md, DESIGN.md, the wireframes, and
  the deliverables of the phases before it.
- A phase is finished when its deliverable is in the repository and the
  "Done when" line holds. Do not begin the next phase before that.
- Product and design questions that come up during a phase are settled by
  editing PRODUCT.md, DESIGN.md, STACK.md, DOMAIN.md or ARCHITECTURE.md, not by deciding silently in code.
- Phases 1 to 3 produce documents. Everything from phase 4 on produces
  code, and each code phase leaves the program runnable and tested.
- The code base is modular from the first commit. `ARCHITECTURE.md`
  (phase 3) names the modules and the dependencies allowed between them,
  the scaffolding enforces those boundaries mechanically, and every code
  phase is done only if it adds no dependency the architecture forbids.
- Phase 7 is the first version worth using every day. From there the
  remaining phases are driven partly by what daily use reveals.

## Phase 1: Tech stack

Choose the language, the terminal UI toolkit, the storage mechanism, the
test approach, and how the program is built and installed on an Omarchy
machine. The choice is judged against DESIGN.md: instant start, live
terminal colours, mouse support in the terminal, box drawing, and a single
binary or script that a Hyprland keybind can launch.

- Deliverable: `STACK.md`, one decision per section with the alternatives
  that were rejected and why.
- Done when: every later phase can name its tools without a debate.

## Phase 2: Domain model

Turn the nouns and rules in PRODUCT.md into a precise model: the entities,
their states, the operations that change them, and the invariants that
must never break. This is where the hard cases are written down once:
what a moved task leaves behind, how recurring copies relate to their
schedule, what "surfaced" means, how undo is scoped, what the storage
format looks like and how it evolves. Vocabulary chosen here is the
vocabulary of the code.

- Deliverable: `DOMAIN.md`, plus the storage schema.
- Done when: every screen in the wireframes can be described as a view of
  the model with no missing concept.

## Phase 3: Architecture

Decide how the code is divided before any of it exists. Name the modules,
give each a one-line responsibility, and fix the direction dependencies may
point. The set should be small: the domain, storage, rendering, input and
key mapping, and a thin application layer that glues them. The domain is
the deep module: it owns every rule in PRODUCT.md, defines the interface
storage must implement, and imports nothing from the other modules. The
user interface is kept shallow: state goes in, a screen comes out, and keys
become named actions. Write down the rule for each seam and what would
count as breaking it.

- Deliverable: `ARCHITECTURE.md`.
- Done when: every later phase can say which module each piece of work
  belongs in, and the boundaries can be checked by a tool, not only read.

## Phase 4: Project scaffolding

Set up the repository as a real project: layout, build, test runner,
linting and formatting, a way to run the program locally, and continuous
integration. The layout follows ARCHITECTURE.md module for module, and a
mechanical boundary check, such as an import lint or a test over the
dependency graph, fails the build when a module imports something it may
not. The program itself only opens an empty window with the status line and
hint bar and quits on `q`.

- Deliverable: a runnable, testable, empty application with enforced module
  boundaries.
- Done when: a fresh clone builds, tests pass, the boundary check is part
  of the test run, and the binary opens and quits inside an Omarchy
  terminal.

## Phase 5: Core domain and storage

Implement the model from phase 2 as pure code with no user interface:
tasks, days, the backlog, ordering, done and focus, moving between places,
dates, waiting, recurring copy generation, the review pile and surfaced
set, the history record of moves, and undo. The domain knows nothing about
files. It defines the interface it needs for persistence, and a separate
storage module implements it with the format from phase 2, writing every
change immediately and restoring state on start. The domain tests run
against an in-memory implementation of that interface.

- Deliverable: the domain module and the storage module, each with a
  thorough test suite.
- Done when: every rule in PRODUCT.md and section 7 of DESIGN.md has a
  test, the program can survive being killed at any moment without losing
  a change, and no dependency crosses a seam ARCHITECTURE.md forbids.

## Phase 6: TUI shell

Build the frame everything else lives in: the one-cell margin, status line,
hint bar, two panes with a divider, the collapse to tabs under 100 columns,
popups drawn over the panes, the mapping of meanings to terminal colours,
keyboard dispatch with text-field mode, the command palette, the help
overlay, and empty states. Rendering, input, and application state are
kept apart as ARCHITECTURE.md lays out: a view is a pure function of state,
and a key press becomes a named action before anything handles it. This
phase uses placeholder data and proves the look against the wireframes and
the three theme screenshots.

- Deliverable: the shell with palette and help, driven by fake data.
- Done when: it matches the wireframes at 120x36, 160x48, and 80x44,
  follows a live Omarchy theme change, and no dependency crosses a seam
  ARCHITECTURE.md forbids.

## Phase 7: Home page

Connect the shell to the domain for the core loop: today beside the
backlog, add, edit in place, close, focus, reorder, move to today, to the
backlog, or to a chosen day, delete, and undo. Editing a recurring copy's
title asks the one deliberate question.

- Deliverable: a usable daily task manager. Start using it.
- Done when: a full work day can be planned and worked through in it
  without touching another tool, and no dependency crosses a seam
  ARCHITECTURE.md forbids.

## Phase 8: Dates, waiting, and repeat

Add the date card, the repeat card, the due and remind chips, the waiting
flag and its group in the backlog, and the creation of recurring copies on
launch for every scheduled date since the last run.

- Deliverable: parking with a date, waiting on someone, and recurring tasks.
- Done when: flows D, E, and F on the wireframe index work end to end,
  and no dependency crosses a seam ARCHITECTURE.md forbids.

## Phase 9: Morning review

Add the two-step review: the pile and the surfaced tasks, the skipping of
empty steps, once-per-day gating, resuming an interrupted review, and the
red count on the home screen.

- Deliverable: the morning review.
- Done when: flow A works, the review is never shown empty or twice, and
  no dependency crosses a seam ARCHITECTURE.md forbids.

## Phase 10: History and search

Step the day pane backwards and forwards, turn the backlog pane into the
list of days with done counts, show past days in their three states with
moved rows that jump to where the task is now, and add search across all
history with re-add from the empty result.

- Deliverable: history browsing and search.
- Done when: flows F and F2 work, a moved task is never rewritten on the
  day it was planned for, and no dependency crosses a seam
  ARCHITECTURE.md forbids.

## Phase 11: Scratchpad

Add the notes page: the list of notes beside the open note, a plain
multi-line text area, create and delete.

- Deliverable: the notes page.
- Done when: a note can be written, left, reopened, and thrown away, and
  no dependency crosses a seam ARCHITECTURE.md forbids.

## Phase 12: Omarchy integration and release

Make the program a proper citizen of the desktop: the Hyprland window rule
and keybind for the floating window, an install path for an Omarchy
machine, a README, and a check of every screen against the installed
themes in dark and light mode.

- Deliverable: an installable release and its README.
- Done when: a keybind opens the app in a centred floating terminal on a
  clean Omarchy install, and no dependency crosses a seam ARCHITECTURE.md
  forbids.

## Phase 13: Hardening

Work through what daily use and edge cases turn up: the date rolling over
while the app is open, terminal resize at any moment, large histories,
startup time, mouse edge cases, and any place where colour alone carried
meaning. Close the gap between the program and DESIGN.md wherever one is
found.

- Deliverable: a version with no known defects against PRODUCT.md and
  DESIGN.md.
- Done when: a week of daily use produces no new items, and no dependency
  crosses a seam ARCHITECTURE.md forbids.
