# Product

A personal task manager for Omarchy, built around a daily work routine.
It is opened every morning to review what happened, plan the day, and keep
a small stack of throwaway notes at hand. It is for one person on one
machine.

## Core loop

The product is shaped by one habit: the morning review.

1. Open the app at the start of the work day.
2. Deal with the review pile: every unfinished task that was planned for a
   day that has now passed. Each one is either closed (it was actually done),
   moved onto today, sent back to the backlog, or deleted.
3. Look at what has become due or has a reminder today.
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
  tasks whose planned day is in the past, however old. A task leaves the pile
  only when it is closed, moved, or deleted. Nothing is carried over
  automatically.

Tasks can be moved freely between a day and the backlog.

### Day plan

Tasks on a day are an ordered list, and the order is under manual control.
One or a few tasks on a day can be marked as focus items, the things that
must get done today, and they stand apart from the rest of the list.

### Dates on backlog tasks

A backlog task can carry a date without being planned for a day:

- **Due by.** A deadline. The task surfaces in the morning review once the
  date is reached or passed.
- **Remind on.** A nudge. The task surfaces in the morning review on that
  date.

In both cases the task stays in the backlog until it is deliberately pulled
onto a day. Surfacing is a prompt, not a move.

### Waiting

A task can be flagged as waiting, meaning it is blocked on someone or
something else. Waiting tasks are shown separately in the backlog and are
not nagged about in the morning review. Clearing the flag returns the task
to the ordinary backlog.

### Recurring tasks

A task can be given a repeat schedule, such as every work day, every Monday,
or the first of the month. On each scheduled date a fresh copy of the task
appears on that day's plan. Each copy is an ordinary task from then on: it
can be closed, reordered, marked as focus, or moved, without affecting the
schedule or other copies.

A copy that is not closed on its day lands in the review pile like any other
task. Editing the schedule or the title changes future copies only. Removing
the schedule stops new copies; existing ones stay where they are.

## History

Past days are kept and can be browsed. Stepping back through earlier days
shows what was planned and what was closed on each of them. A task that was
planned for a day and later moved elsewhere still shows on that day, marked
as moved and pointing at where it is now, so the record of what was planned
is never rewritten. Completed tasks can be searched by title across all
history.

## Scratchpad

The scratchpad is a small stack of short-lived text notes, the equivalent
of a post-it block on the desk. A note is for things like "remember to
mention X" or drafting a message before sending it.

- A note is plain text. No formatting, no title, no attachments.
- Notes are independent of tasks. They are not attached to or linked from
  anything.
- Notes are created and thrown away individually. A note stays until it is
  deleted; there is no automatic expiry or cleanup.
- Notes are not long-term storage and the product does not try to organise
  them.

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
