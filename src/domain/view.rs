//! The views of the model: what each screen is drawn from.
//!
//! Ordering, filtering and grouping are decided here and nowhere else,
//! so `ui` only formats and places what a view hands it
//! (ARCHITECTURE.md section 5, rule 3).

use std::collections::BTreeMap;

use jiff::Zoned;
use jiff::civil::Date;

use super::model::{FromPlace, Id, Model, Note, Place, REVIEW_BEFORE, REVIEW_ON, Task};
use super::rule::Rule;
use super::working_day;

/// A task as a screen draws it, with every decision the domain owns
/// already made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub task: Id,
    pub title: String,
    /// Where the task is now, which on a Moved row is what it points at.
    pub place: Place,
    pub closed_at: Option<Zoned>,
    pub focus: bool,
    pub waiting: bool,
    pub due: Option<DueChip>,
    pub remind: Option<Date>,
    /// The rule of the schedule this task is a copy of.
    pub repeat: Option<Rule>,
    /// `was focus`: closed and focus.
    pub was_focus: bool,
    /// `on the pile`: open, on a day before today.
    pub on_the_pile: bool,
    /// `←backlog`: it came from the backlog on the day being drawn.
    pub from_backlog: bool,
    /// Whether the Done group shows a time or a date: a task closed from
    /// the review is closed on its old day at a later date.
    pub closed_on_this_day: bool,
}

/// A due date and whether it has passed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DueChip {
    pub on: Date,
    pub overdue: bool,
}

/// The four groups of a day, in the order they are drawn. A group with
/// nothing in it is not drawn at all.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DayView {
    pub day: Date,
    pub focus: Vec<Row>,
    pub plan: Vec<Row>,
    pub done: Vec<Row>,
    pub moved: Vec<Row>,
    pub counts: DayCounts,
}

/// `planned` is every live placement for the day; the rest split it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DayCounts {
    pub planned: usize,
    pub open: usize,
    pub done: usize,
    pub moved: usize,
}

/// The two groups of the backlog and the schedule list under them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BacklogView {
    pub ordinary: Vec<Row>,
    pub waiting: Vec<Row>,
    pub schedules: Vec<ScheduleRow>,
    /// Live open backlog tasks, and how many of them are waiting.
    pub open: usize,
    pub waiting_count: usize,
}

/// A live, unstopped schedule, by title and rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleRow {
    pub schedule: Id,
    pub title: String,
    pub rule: Rule,
}

/// Every open task on a day before today, grouped by day, newest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pile {
    pub days: Vec<PileDay>,
    pub total: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PileDay {
    pub day: Date,
    /// `today − day` in days, which the screen renders relatively.
    pub age: i64,
    pub rows: Vec<Row>,
}

/// The second step of the review.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Surfaced {
    pub due: Vec<Row>,
    pub reminders: Vec<Row>,
    /// Copies starting today, shown for information: nothing is decided
    /// about them, so they are outside `total`.
    pub also_starting_today: Vec<Row>,
    /// How many rows the step asks a decision about.
    pub total: usize,
}

impl Surfaced {
    /// Whether the step is skipped. Copies starting today are worth
    /// showing on their own, so they count here.
    pub fn is_empty(&self) -> bool {
        self.due.is_empty() && self.reminders.is_empty() && self.also_starting_today.is_empty()
    }
}

/// Matches in two groups: open tasks, then closed ones by place day.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchResults {
    pub open: Vec<Row>,
    pub closed: Vec<Row>,
    pub total: usize,
}

/// The distinct days that have a placement, newest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DayList {
    pub days: Vec<DayListRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DayListRow {
    pub day: Date,
    /// Kept, done and still open: `done / kept`, and "· n open".
    pub kept: usize,
    pub done: usize,
    pub open: usize,
}

/// The scratchpad's list, and the count the home page shows beside `n`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotesView {
    pub rows: Vec<NoteRow>,
    pub count: usize,
}

/// A note as the list draws it: its first line stands for the whole body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteRow {
    pub note: Id,
    pub first_line: String,
    pub updated_at: Zoned,
}

// ---- the views -------------------------------------------------------

/// A day is drawn the same way whether it is today, past or future.
pub fn day_view(model: &Model, day: Date, today: Date) -> DayView {
    let mut view = DayView {
        day,
        ..DayView::default()
    };

    for task in model.place(Place::Day(day)) {
        let row = row_of(model, task, today, Some(day));
        if task.is_open() {
            if task.focus {
                view.focus.push(row);
            } else {
                view.plan.push(row);
            }
        } else {
            view.done.push(row);
        }
    }
    view.done.sort_by(|a, b| {
        a.closed_at
            .cmp(&b.closed_at)
            .then_with(|| a.task.cmp(&b.task))
    });

    let mut moved: Vec<(&Zoned, Row)> = Vec::new();
    for placement in model.placements.values().filter(|p| p.day == day) {
        let Some(task) = model.live_task(placement.task_id) else {
            continue;
        };
        if task.day == Some(day) {
            continue;
        }
        moved.push((&placement.placed_at, row_of(model, task, today, Some(day))));
    }
    moved.sort_by(|a, b| a.0.cmp(b.0).then_with(|| a.1.task.cmp(&b.1.task)));
    view.moved = moved.into_iter().map(|(_, row)| row).collect();

    view.counts = DayCounts {
        planned: view.focus.len() + view.plan.len() + view.done.len() + view.moved.len(),
        open: view.focus.len() + view.plan.len(),
        done: view.done.len(),
        moved: view.moved.len(),
    };
    view
}

/// The backlog: one flat list, with the waiting tasks shown apart.
pub fn backlog_view(model: &Model, today: Date) -> BacklogView {
    let mut view = BacklogView::default();

    for task in model.place(Place::Backlog) {
        if !task.is_open() {
            continue;
        }
        let row = row_of(model, task, today, None);
        if task.waiting {
            view.waiting.push(row);
        } else {
            view.ordinary.push(row);
        }
    }
    view.waiting_count = view.waiting.len();
    view.open = view.ordinary.len() + view.waiting_count;

    view.schedules = model
        .schedules
        .values()
        .filter(|schedule| !schedule.is_stopped())
        .map(|schedule| ScheduleRow {
            schedule: schedule.id,
            title: schedule.title.clone(),
            rule: schedule.rule.clone(),
        })
        .collect();
    view
}

/// Every unfinished task from a day that has passed, however old.
pub fn pile(model: &Model, today: Date) -> Pile {
    let mut dates: Vec<Date> = model
        .tasks
        .values()
        .filter(|task| task.is_live() && task.is_open())
        .filter_map(|task| task.day)
        .filter(|day| *day < today)
        .collect();
    dates.sort_unstable();
    dates.dedup();
    // Newest day first; each day in position order.
    dates.reverse();

    let days: Vec<PileDay> = dates
        .into_iter()
        .map(|day| PileDay {
            day,
            age: day
                .until(today)
                .map_or(0, |span| i64::from(span.get_days())),
            rows: model
                .place(Place::Day(day))
                .into_iter()
                .filter(|task| task.is_open())
                .map(|task| row_of(model, task, today, Some(day)))
                .collect(),
        })
        .collect();

    let total = days.iter().map(|day| day.rows.len()).sum();
    Pile { days, total }
}

/// The tasks a date or a schedule puts in front of the person today.
pub fn surfaced(model: &Model, today: Date) -> Surfaced {
    let previous = previous_review(model, today);
    let mut view = Surfaced::default();

    for task in model.tasks.values() {
        if !task.is_live() {
            continue;
        }
        let row = || row_of(model, task, today, None);

        if task.is_open() && task.day.is_none() {
            // Waiting suppresses due, not remind: that is the whole of
            // "not nagged about" (DOMAIN.md section 9).
            if !task.waiting && task.due_on.is_some_and(|due| due <= today) {
                view.due.push(row());
            }
            let reminded = task.remind_on.is_some_and(|remind| {
                remind <= today && previous.is_none_or(|previous| remind > previous)
            });
            if reminded {
                view.reminders.push(row());
            }
        }
        if task.scheduled_on == Some(today) {
            view.also_starting_today
                .push(row_of(model, task, today, task.day));
        }
    }

    // Overdue first, then by due date.
    view.due
        .sort_by(|a, b| due_key(a).cmp(&due_key(b)).then(a.task.cmp(&b.task)));
    view.reminders
        .sort_by(|a, b| a.remind.cmp(&b.remind).then(a.task.cmp(&b.task)));
    view.also_starting_today.sort_by_key(|row| row.task);
    view.total = view.due.len() + view.reminders.len();
    view
}

/// The date of the last review started on a day before today, which is
/// the lower bound of the reminder window (DOMAIN.md section 8).
pub fn previous_review(model: &Model, today: Date) -> Option<Date> {
    match model.meta_date(REVIEW_ON) {
        Some(on) if on < today => Some(on),
        Some(_) => model.meta_date(REVIEW_BEFORE),
        None => None,
    }
}

/// Live tasks whose title contains the text, case-insensitively.
pub fn search(model: &Model, text: &str, today: Date) -> SearchResults {
    let needle = text.to_lowercase();
    let mut view = SearchResults::default();

    for task in model.tasks.values() {
        if !task.is_live() || !task.title.to_lowercase().contains(&needle) {
            continue;
        }
        let row = row_of(model, task, today, task.day);
        if task.is_open() {
            view.open.push(row);
        } else {
            view.closed.push(row);
        }
    }

    // Open tasks by place, days before the backlog; closed newest first.
    view.open.sort_by(|a, b| {
        place_key(a.place)
            .cmp(&place_key(b.place))
            .then(a.task.cmp(&b.task))
    });
    view.closed.sort_by(|a, b| {
        b.place
            .cmp(&a.place)
            .then_with(|| b.closed_at.cmp(&a.closed_at))
            .then(b.task.cmp(&a.task))
    });
    view.total = view.open.len() + view.closed.len();
    view
}

/// The day list the backlog pane becomes when history is browsed.
pub fn day_list(model: &Model) -> DayList {
    let mut days: BTreeMap<Date, DayListRow> = BTreeMap::new();

    for placement in model.placements.values() {
        let Some(task) = model.live_task(placement.task_id) else {
            continue;
        };
        let row = days.entry(placement.day).or_insert(DayListRow {
            day: placement.day,
            kept: 0,
            done: 0,
            open: 0,
        });
        if task.day != Some(placement.day) {
            continue;
        }
        row.kept += 1;
        if task.is_open() {
            row.open += 1;
        } else {
            row.done += 1;
        }
    }

    let mut days: Vec<DayListRow> = days.into_values().collect();
    days.reverse();
    DayList { days }
}

/// The live notes, newest first. Editing does not move a note, so the
/// order is the order they were created in (DOMAIN.md section 15).
pub fn notes(model: &Model) -> NotesView {
    let mut live: Vec<&Note> = model.notes.values().filter(|note| note.is_live()).collect();
    live.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));

    NotesView {
        count: live.len(),
        rows: live
            .into_iter()
            .map(|note| NoteRow {
                note: note.id,
                first_line: note.body.lines().next().unwrap_or_default().to_owned(),
                updated_at: note.updated_at.clone(),
            })
            .collect(),
    }
}

// ---- rows ------------------------------------------------------------

/// The row a task draws as, on the day being shown or in a list with no
/// day of its own.
fn row_of(model: &Model, task: &Task, today: Date, on: Option<Date>) -> Row {
    let placement = on.and_then(|day| model.placement(task.id, day));
    let from_backlog = placement.is_some_and(|placement| {
        matches!(placement.from_place, FromPlace::Backlog)
            && Some(working_day(&placement.placed_at)) == on
    });

    Row {
        task: task.id,
        title: task.title.clone(),
        place: task.place(),
        closed_at: task.closed_at.clone(),
        focus: task.focus,
        waiting: task.waiting,
        due: task.due_on.map(|on| DueChip {
            on,
            overdue: on < today,
        }),
        remind: task.remind_on,
        repeat: task
            .schedule_id
            .and_then(|id| model.schedule(id))
            .map(|schedule| schedule.rule.clone()),
        was_focus: !task.is_open() && task.focus,
        on_the_pile: task.is_open() && task.day.is_some_and(|day| day < today),
        from_backlog,
        closed_on_this_day: task
            .closed_at
            .as_ref()
            .is_some_and(|at| Some(working_day(at)) == task.day),
    }
}

fn due_key(row: &Row) -> (bool, Option<Date>) {
    match row.due {
        Some(due) => (!due.overdue, Some(due.on)),
        None => (true, None),
    }
}

/// Days come before the backlog, newest day first.
fn place_key(place: Place) -> (bool, Option<Date>) {
    match place {
        Place::Day(day) => (false, Some(day)),
        Place::Backlog => (true, None),
    }
}
