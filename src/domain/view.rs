//! The views of the model: what each screen is drawn from.
//!
//! Ordering, filtering and grouping are decided here and nowhere else,
//! so `ui` only formats and places what a view hands it
//! (ARCHITECTURE.md section 5, rule 3).

use std::collections::BTreeMap;

use jiff::civil::Date;
use jiff::{Span, Zoned};

use super::model::{FromPlace, Id, Model, Note, Place, REVIEW_BEFORE, REVIEW_ON, Task};
use super::rule::{Rule, Weekday};
use super::settings::WeekStart;

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
    /// `on the pile`: open, on a day before today the pile still
    /// reaches.
    pub on_the_pile: bool,
    /// `still open`: the same, on a day beyond the pile's horizon.
    pub still_open: bool,
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

/// The distinct days that have a placement, newest first, broken into
/// the stretches of the calendar the screen draws as groups. A stretch
/// with no day in it is left out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DayList {
    pub stretches: Vec<DayStretch>,
}

impl DayList {
    /// Every day of the list, newest first, whichever stretch it is in.
    pub fn days(&self) -> impl Iterator<Item = &DayListRow> {
        self.stretches.iter().flat_map(|stretch| &stretch.days)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DayStretch {
    pub stretch: Stretch,
    pub days: Vec<DayListRow>,
}

/// Which stretch of the calendar a day falls in, counted in whole weeks
/// from the Monday of the week today is in (DOMAIN.md section 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stretch {
    Later,
    ThisWeek,
    LastWeek,
    Earlier,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DayListRow {
    pub day: Date,
    /// Kept and done: `done / kept`.
    pub kept: usize,
    pub done: usize,
    /// What the day still owes the pile, which is why it is counted only
    /// for a day that has passed (DOMAIN.md section 6).
    pub open: usize,
}

/// One of the notes page's two lists, the stack or the archive, with
/// the count of each. `count` is the notes in the list, which is what
/// the home page shows beside `gn`; archived notes are not in it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotesView {
    pub rows: Vec<NoteRow>,
    pub count: usize,
    pub archived: usize,
}

/// A note as the list draws it: its first line stands for the whole body,
/// and the instant it was made is the age beside it. The list is in that
/// order, so it is the age of the note rather than of its last edit
/// (DOMAIN.md section 15).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteRow {
    pub note: Id,
    pub first_line: String,
    pub created_at: Zoned,
    /// When the note was archived, which is the age an archive row shows.
    pub archived_at: Option<Zoned>,
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

/// Every unfinished task from a day that has passed, back as far as
/// the horizon reaches.
pub fn pile(model: &Model, today: Date) -> Pile {
    let horizon = horizon_of(model, today);
    let mut dates: Vec<Date> = model
        .tasks
        .values()
        .filter(|task| task.is_live() && task.is_open())
        .filter_map(|task| task.day)
        .filter(|day| *day < today && horizon.is_none_or(|earliest| *day >= earliest))
        .collect();
    dates.sort_unstable();
    dates.dedup();
    // Newest day first; each day in position order.
    dates.reverse();

    let days: Vec<PileDay> = dates
        .into_iter()
        .map(|day| PileDay {
            day,
            age: age_of(day, today),
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
    // How far ahead a due date is allowed to see (DOMAIN.md section 8).
    let ahead = today.saturating_add(Span::new().days(i64::from(model.settings.due_ahead_days())));
    let mut view = Surfaced::default();

    for task in model.tasks.values() {
        if !task.is_live() {
            continue;
        }
        let row = || row_of(model, task, today, None);

        if task.is_open() && task.day.is_none() {
            // Waiting suppresses due, not remind: that is the whole of
            // "not nagged about" (DOMAIN.md section 9).
            if !task.waiting && task.due_on.is_some_and(|due| due <= ahead) {
                view.due.push(row());
            }
            let reminded = task.remind_on.is_some_and(|remind| {
                remind <= today && previous.is_none_or(|previous| remind > previous)
            });
            if reminded {
                view.reminders.push(row());
            }
        }
        // A copy generation made for today, which is on today. The task
        // a schedule was created from is scheduled for today as well
        // (section 10), and until a copy is actually made there is
        // nothing new to say about it: it is where it always was, and
        // the group would be claiming it had arrived on the plan.
        if task.scheduled_on == Some(today) && task.day == Some(today) {
            view.also_starting_today
                .push(row_of(model, task, today, Some(today)));
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

/// The pile a review is working down: the same days and the same tasks
/// it opened with, every row drawn as the task is now.
///
/// A task the review has closed, moved or deleted has left the pile
/// itself, and its row stays in the day it was on, so that the list
/// never moves under the person's hand (DOMAIN.md section 13). That is
/// also why a deleted task keeps its row here and nowhere else: the
/// review has to be able to say what it did to it.
pub fn pile_again(model: &Model, today: Date, opened: &Pile) -> Pile {
    let days: Vec<PileDay> = opened
        .days
        .iter()
        .map(|opened| PileDay {
            day: opened.day,
            age: age_of(opened.day, today),
            rows: opened
                .rows
                .iter()
                .filter_map(|row| model.task(row.task))
                .map(|task| row_of(model, task, today, Some(opened.day)))
                .collect(),
        })
        .collect();

    let total = days.iter().map(|day| day.rows.len()).sum();
    Pile { days, total }
}

/// The surfaced set a review is working down, drawn again the same way.
/// A row is drawn on the day the task is on now, so a task pulled onto
/// today says where it went.
pub fn surfaced_again(model: &Model, today: Date, opened: &Surfaced) -> Surfaced {
    let again = |rows: &[Row]| -> Vec<Row> {
        rows.iter()
            .filter_map(|row| model.task(row.task))
            .map(|task| row_of(model, task, today, task.day))
            .collect()
    };

    let (due, reminders) = (again(&opened.due), again(&opened.reminders));
    Surfaced {
        total: due.len() + reminders.len(),
        due,
        reminders,
        also_starting_today: again(&opened.also_starting_today),
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
pub fn day_list(model: &Model, today: Date) -> DayList {
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
        if !task.is_open() {
            row.done += 1;
        } else if placement.day < today {
            // An open task on a day that has passed is on the pile; one
            // on today or later is simply planned.
            row.open += 1;
        }
    }

    let mut stretches: Vec<DayStretch> = Vec::new();
    for row in days.into_values().rev() {
        let stretch = stretch_of(row.day, today, model.settings.week_starts_on());
        match stretches.last_mut() {
            Some(last) if last.stretch == stretch => last.days.push(row),
            _ => stretches.push(DayStretch {
                stretch,
                days: vec![row],
            }),
        }
    }
    DayList { stretches }
}

/// The first day of the week a date is in, on the day the settings say
/// a week begins on.
pub(super) fn week_start_of(date: Date, start: WeekStart) -> Date {
    // `Weekday::of` counts from Monday, so a Sunday-start week is the
    // same count shifted round by one.
    let gone = match start {
        WeekStart::Monday => Weekday::of(date) as i64,
        WeekStart::Sunday => (Weekday::of(date) as i64 + 1) % 7,
    };
    date.saturating_sub(Span::new().days(gone))
}

fn stretch_of(day: Date, today: Date, start: WeekStart) -> Stretch {
    let this = week_start_of(today, start);
    if day >= this.saturating_add(Span::new().days(7)) {
        Stretch::Later
    } else if day >= this {
        Stretch::ThisWeek
    } else if day >= this.saturating_sub(Span::new().days(7)) {
        Stretch::LastWeek
    } else {
        Stretch::Earlier
    }
}

/// The live notes that are not archived, newest first. Editing does not
/// move a note, so the order is the order they were created in (DOMAIN.md
/// section 15).
pub fn notes(model: &Model) -> NotesView {
    let (mut listed, archived) = live_notes(model);
    listed.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
    NotesView {
        count: listed.len(),
        archived: archived.len(),
        rows: listed.into_iter().map(note_row).collect(),
    }
}

/// The archived notes, the most recently archived first (DOMAIN.md
/// section 15).
pub fn archived_notes(model: &Model) -> NotesView {
    let (listed, mut archived) = live_notes(model);
    archived.sort_by(|a, b| b.archived_at.cmp(&a.archived_at).then(b.id.cmp(&a.id)));
    NotesView {
        count: listed.len(),
        archived: archived.len(),
        rows: archived.into_iter().map(note_row).collect(),
    }
}

/// The live notes, split into those in the list and those archived.
fn live_notes(model: &Model) -> (Vec<&Note>, Vec<&Note>) {
    model
        .notes
        .values()
        .filter(|note| note.is_live())
        .partition(|note| !note.is_archived())
}

fn note_row(note: &Note) -> NoteRow {
    NoteRow {
        note: note.id,
        first_line: note.body.lines().next().unwrap_or_default().to_owned(),
        created_at: note.created_at.clone(),
        archived_at: note.archived_at.clone(),
    }
}

// ---- rows ------------------------------------------------------------

/// The row a task draws as, on the day being shown or in a list with no
/// day of its own.
fn row_of(model: &Model, task: &Task, today: Date, on: Option<Date>) -> Row {
    let placement = on.and_then(|day| model.placement(task.id, day));
    let left_open = task.is_open() && task.day.is_some_and(|day| day < today);
    let within_the_horizon =
        horizon_of(model, today).is_none_or(|earliest| task.day.is_some_and(|day| day >= earliest));
    let from_backlog = placement.is_some_and(|placement| {
        matches!(placement.from_place, FromPlace::Backlog)
            && Some(model.settings.working_day(&placement.placed_at)) == on
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
        on_the_pile: left_open && within_the_horizon,
        still_open: left_open && !within_the_horizon,
        from_backlog,
        closed_on_this_day: task
            .closed_at
            .as_ref()
            .is_some_and(|at| Some(model.settings.working_day(at)) == task.day),
    }
}

/// The oldest day the pile reaches back to, or none while the horizon
/// is off. A task on a day before it stays where it is and is left out
/// of the pile and its count (DOMAIN.md section 19).
fn horizon_of(model: &Model, today: Date) -> Option<Date> {
    let days = model.settings.pile_horizon_days();
    (days > 0).then(|| today.saturating_sub(Span::new().days(i64::from(days))))
}

/// How many days ago a day was, which the screen renders relatively.
fn age_of(day: Date, today: Date) -> i64 {
    day.until(today)
        .map_or(0, |span| i64::from(span.get_days()))
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
