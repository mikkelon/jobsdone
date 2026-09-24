# Product

A personal task manager for Omarchy, built around a daily work routine.
It is opened every morning to review what happened, plan the day, and keep
a small stack of throwaway notes at hand. It is for one person on one
machine.

## Core loop

The product is shaped by one habit: the morning review.

1. Open the app at the start of the work day. The review opens itself on
   the first open of the day, unless it is set to wait for `gr` instead.
2. Deal with the review pile: every unfinished task that was planned for a
   day that has now passed, back as far as the pile is set to reach. Each
   one is either closed (it was actually done), moved onto today, sent back
   to the backlog, or deleted.
3. Look at what has become due or has a reminder today, and at what falls
   due within however many days the settings look ahead.
4. Plan today: pull tasks in from the backlog, add new ones, order them, and
   mark the few that must get done.
5. Work through the day, closing tasks as they finish and jotting notes as
   needed.

Everything else in the product exists to make this loop fast.

## Tasks

A task is deliberately small: a title, whether it is done, and where it
lives. There are no descriptions, subtasks, tags, projects, or priorities.

A task lives in exactly one of these places:

- **A day.** The task is planned for a specific date. Today's day is the
  working list.
- **The backlog.** The task has no day. This is one flat list that holds
  everything not pinned to a date: long-running work, things to do when
  there is time, and things waiting on someone else.
- **The review pile.** Not a place a task is put, but a view: all unfinished
  tasks whose planned day is in the past, back as far as the pile is set to
  reach. A task leaves the pile only when it is closed, moved, or deleted.
  Nothing is carried over automatically, and a task the pile no longer
  reaches stays on its day, marked as still open.

Tasks can be moved freely between a day and the backlog.

### Day plan

Tasks on a day are an ordered list, and the order is under manual control.
One or a few tasks on a day can be marked as focus items, the things that
must get done today, and they stand apart from the rest of the list.

### Dates on backlog tasks

A backlog task can carry a date without being planned for a day:

- **Due by.** A deadline. The task surfaces in the morning review once the
  date is reached or passed, or as many days before it as the settings
  say.
- **Remind on.** A nudge. The task surfaces in the morning review on that
  date.

In both cases the task stays in the backlog until it is deliberately pulled
onto a day. Surfacing is a prompt, not a move.

### Waiting

A backlog task can be flagged as waiting, meaning it is blocked on
someone or something else. Waiting tasks are shown separately in the
backlog and are not nagged about in the morning review: a due date on a
waiting task does not surface, a reminder still does. Flagging a task
that is on a day sends it to the backlog as waiting. Pulling a waiting
task onto a day clears the flag, and so does clearing it in place.

### Recurring tasks

A task can be given a repeat schedule, such as every work day, every Monday,
or the first of the month. On each scheduled date a fresh copy of the task
appears on that day's plan. Each copy is an ordinary task from then on: it
can be closed, reordered, marked as focus, or moved, without affecting the
schedule or other copies.

A copy that is not closed on its day lands in the review pile like any other
task. Coming back from time away means a copy for every scheduled date since
the last open, which is where the cost of being away is meant to be seen;
the catch-up setting cuts that to the last few days for someone who would
rather start clean. Editing the schedule or the title changes future copies
only. Removing the schedule stops new copies; existing ones stay where they
are.

## History

Past days are kept and can be browsed. Stepping back through earlier days
shows what was planned and what was closed on each of them. A task that was
planned for a day and later moved elsewhere still shows on that day, marked
as moved and pointing at where it is now, so the record of what was planned
is never rewritten. Completed tasks can be searched by title across all
history.

## Notes

Notes are a small stack of short-lived texts, the equivalent of a post-it
block on the desk. The live ones are the Stack; a note is written in the
scratchpad, the pane beside the list. A note is for things like "remember to
mention X" or drafting a message before sending it.

- A note is plain text. No formatting, no title, no attachments.
- Spelling is checked locally against US English and potential mistakes are
  underlined. Checking is on by default and can be disabled in settings.
  Suggestions are available on request; choosing one replaces that word.
  Checking never changes text automatically or prevents writing in another language.
- A personal dictionary accepts names and other words across all notes.
  Entries ignore capitalization, persist locally, and can be added, edited,
  or removed from settings. Adding a word does not change the note's text.
- The entire note can be copied to the clipboard from the list or while
  editing, preserving its text and the editing position.
- Notes are independent of tasks. They are not attached to or linked from
  anything.
- Notes are created and thrown away individually. A note stays until it is
  deleted or archived; there is no automatic expiry or cleanup.
- Archiving puts a note out of the way without throwing it away; archived
  notes are kept, can be opened and edited, and come back with the same key.
  The notes page is still not a place to organise notes: there are no
  folders, tags, or titles.
- The Stack and the Archive can each be filtered by typing a few letters
  of what a note says.
- Notes are not long-term storage and the product does not try to organise
  them.

## Settings

The program runs on its defaults, and a page reached with `gs` changes the
few things worth changing. There is no configuration file: the settings
are kept with the tasks, so every open window picks a change up at once.

| Setting                     | Default                | What it changes |
|-----------------------------|------------------------|-----------------|
| Day starts at               | 05:00                  | The hour a new day begins, so a late night belongs to the day it started in. |
| Week starts on              | Monday                 | Where the history's weeks are broken, and the first column of a calendar. |
| Work days                   | Monday to Friday       | What "every work day" repeats on, and what "next work day" means. |
| Open the review on launch   | on                     | Off, the morning review waits for `gr` instead of opening itself. |
| Surface due tasks early     | on its day             | How many days before its due date a backlog task is put in front of you in the morning review. At 0 it surfaces on the due date and on every day after until it is dealt with; at 3 it also surfaces on the three days before. |
| Catch up recurring tasks    | every missed day       | A recurring task gets a fresh copy on every day its schedule names. After days away from the app, the copies for the days you missed are made on the next launch, each landing on the review pile. This caps how far back that goes: at 7, only the last week's missed copies are made and older ones are skipped for good. Every missed day makes them all. |
| Hide pile tasks older than  | never                  | How far back the review pile reaches. Older tasks stay on their day. |
| Floating window             | on                     | On Omarchy, whether the app floats or tiles. |
| Window size                 | 870x650 · 120 by 36 cells | The floating window's size in logical pixels. `h` and `l` step through five sizes, named by the cells they give in foot with Omarchy's default font; Enter types any other. 870 by 650 is 120 by 36 cells, the size the screens are designed at. |
| Mouse                       | on                     | Off, the terminal's own selection and scrollback come back and the keyboard does everything. |
| Hint bar messages stand for | 4 seconds              | How long "closed X · u undo" stays when no key follows. |
| Date order                  | as the locale writes it | `Fri 5 Sep` or `Fri Sep 5`, everywhere a date is written. |
| Confirm before delete       | off                    | On, `x` asks first instead of deleting and offering to undo. |
| Spell-check notes in US English | off                 | Underline possible US English spelling mistakes in notes. Checking works offline. |

Colour and font are not here: they come from the terminal, which Omarchy
themes.

## Out of scope

These are deliberate exclusions, not gaps:

- **Sync between devices.** The product runs on a single machine. No cloud,
  no phone.
- **Sharing or collaboration.** Nobody else sees or edits anything.
- **Integrations.** No calendar, issue tracker, or other external system is
  read from or written to.
- **Rich task structure.** No descriptions, subtasks, tags, projects,
  priorities, time slots, or durations.
- **Drawing.** Notes are text only. Sketching belongs in a drawing tool.
