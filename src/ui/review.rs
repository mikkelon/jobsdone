//! The morning review: the step on screen, the panel of outcomes beside
//! it, and how far down the list the person has got.
//!
//! It takes the whole window, because the review is a ritual rather than
//! a sidebar (DESIGN.md section 5). The list is the pile or the surfaced
//! set as the review opened it; what to draw in each row is the domain's
//! and the words for what the review did to it are the session's.

use jiff::civil::Date;

use super::{
    Canvas, Column, HEADER_GAP, Kind, Look, NARROW, Rows, bold, clip, count, cursor, day_label,
    dim, group_rule, item_width, keys, place_label, plain, quiet, row_area, scroll_to, task_row,
    title_field, words,
};
use crate::app::{App, Decided, Editor, Layout, List, ListArea, Rect as Cells, Review, RowId};
use crate::domain::{DateOrder, Place, Row};
use crate::input::{self, Field, ReviewStep};

/// The columns the panel takes, as wireframes 01 and 02 draw it.
const PANEL: u16 = 40;

/// `MORNING REVIEW step 1 of 2 · the pile     7 unfinished from past days`
pub(super) fn status(canvas: &mut Canvas, review: &Review, y: u16, narrow: bool) {
    let (at, of) = review.steps();
    let (handled, total) = review.progress();
    let name = "MORNING REVIEW";

    // A narrow window has no panel to carry the progress, so the status
    // line does.
    let right = if narrow {
        let progress = if total == 0 {
            "nothing to decide".to_owned()
        } else {
            format!("{handled} of {total}")
        };
        vec![quiet(&progress)]
    } else {
        vec![quiet(&count_of(review)), keys(&[("esc", "skip for now")])]
    };

    // The step keeps the gap clear of what the right end says, so the two
    // are never read as one word (DESIGN.md section 6). A line too tight
    // for the name of the step drops it whole rather than cutting a word
    // in half; the count of the steps is the part that has to be there.
    let gaps = 3 * (right.len().saturating_sub(1)) as u16;
    let taken: u16 = right.iter().map(item_width).sum();
    let room = (canvas.width() - 1).saturating_sub(1 + count(name) + 1 + taken + gaps + HEADER_GAP);
    let steps = format!("step {at} of {of}");
    let step = format!("{steps} · {}", subtitle(review.step()));
    let step = if count(&step) <= room { step } else { steps };
    let left = vec![vec![words(name, bold()), words(clip(&step, room), dim())]];

    canvas.segments(1, y, &left, 1);
    canvas.rsegments(canvas.width() - 1, y, &right, 3);
}

fn subtitle(step: ReviewStep) -> &'static str {
    match step {
        ReviewStep::Pile => "the pile",
        ReviewStep::Surfaced => "due & reminders",
    }
}

/// What the step is a count of, which is what makes the number mean
/// something without reading the list.
fn count_of(review: &Review) -> String {
    let (_, total) = review.progress();
    match review.step() {
        ReviewStep::Pile => format!("{total} unfinished from past days"),
        // A step whose rows are all information asks nothing, so it says
        // what it is rather than counting none of it (DESIGN.md section 5).
        ReviewStep::Surfaced if total == 0 => {
            let starting = review
                .surfaced()
                .map_or(0, |surfaced| surfaced.also_starting_today.len());
            format!("{starting} starting today, nothing to decide")
        }
        ReviewStep::Surfaced => format!("{total} surfaced today"),
    }
}

pub(super) fn draw(canvas: &mut Canvas, app: &App, rows: &Rows, layout: &mut Layout) {
    let Some(review) = app.review() else {
        return;
    };
    let width = canvas.width();
    // Wide enough for the panel beside the list, or the list alone: a
    // narrow window keeps the list, and the hint bar names every key the
    // panel would have named.
    let divider = (width >= NARROW).then(|| width - PANEL - 1);

    let list = Column {
        x: 0,
        width: divider.unwrap_or(width),
        top: rows.top,
        bottom: rows.bottom,
    };
    // The heading sits over the panel, where there is one.
    heading(
        canvas,
        app,
        divider.map_or(0, |divider| divider + 1),
        rows.headers,
    );

    if let Some(divider) = divider {
        canvas.put(divider, rows.headers + 1, "\u{252c}", dim());
        canvas.vline(divider, rows.top, rows.bottom - rows.top + 1, dim());
        panel(
            canvas,
            review,
            Column {
                x: divider + 1,
                width: PANEL,
                top: rows.top,
                bottom: rows.bottom,
            },
        );
    }
    the_list(canvas, app, review, list, layout);
}

/// The task the panel is about, over the panel, so that the keys under it
/// are read as keys for this row.
fn heading(canvas: &mut Canvas, app: &App, x: u16, y: u16) {
    let title = app
        .cursor(List::Review)
        .and_then(RowId::task)
        .and_then(|task| app.model().task(task))
        .map(|task| task.title.to_uppercase())
        .unwrap_or_default();
    canvas.put(x + 1, y, &title, bold());
}

// ---- the list --------------------------------------------------------

/// One drawn line of the step, so that a list longer than the window
/// scrolls a line at a time.
enum Line<'a> {
    Blank,
    Rule(String, usize),
    Task(&'a Row, Kind, String),
}

fn lines_of<'a>(review: &'a Review, today: Date, dates: DateOrder) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let mut group = |label: String, rows: &'a [Row], starting: bool| {
        if rows.is_empty() {
            return;
        }
        if !lines.is_empty() {
            lines.push(Line::Blank);
        }
        lines.push(Line::Rule(label, rows.len()));
        for row in rows {
            let answered = review.decision(row.task);
            let kind = match (answered, row.waiting) {
                (Some(_), _) => Kind::Handled,
                (None, true) => Kind::Waiting,
                (None, false) => Kind::Open,
            };
            lines.push(Line::Task(
                row,
                kind,
                note_of(answered, row, today, dates, starting),
            ));
        }
    };

    if let Some(pile) = review.pile().filter(|_| review.step() == ReviewStep::Pile) {
        for day in &pile.days {
            group(day_heading(day.day, day.age, dates), &day.rows, false);
        }
    }
    if let Some(surfaced) = review
        .surfaced()
        .filter(|_| review.step() == ReviewStep::Surfaced)
    {
        group("Due".to_owned(), &surfaced.due, false);
        group("Reminders".to_owned(), &surfaced.reminders, false);
        group(
            "Also starting today".to_owned(),
            &surfaced.also_starting_today,
            true,
        );
    }
    lines
}

/// `Yesterday · Thu 4 Sep`, `Mon 1 Sep`, `Fri 22 Aug · 2 weeks ago`: the
/// day, and how long ago where the date alone does not say it.
fn day_heading(day: Date, age: i64, dates: DateOrder) -> String {
    let date = day_label(day, dates);
    match age {
        1 => format!("Yesterday · {date}"),
        7..=13 => format!("{date} · last week"),
        14.. => format!("{date} · {} weeks ago", age / 7),
        _ => date,
    }
}

/// The words at the right of a row: what the review did to it, or what
/// the row is while it waits to be answered.
fn note_of(
    answered: Option<Decided>,
    row: &Row,
    today: Date,
    dates: DateOrder,
    starting: bool,
) -> String {
    match answered {
        Some(Decided::Done) => return "✓ closed".to_owned(),
        Some(Decided::Moved(place)) => return format!("→ {}", place_label(place, today, dates)),
        Some(Decided::Deleted) => return "✗ deleted".to_owned(),
        Some(Decided::Kept) => return "kept".to_owned(),
        Some(Decided::Dated) => return "re-dated".to_owned(),
        Some(Decided::Waiting) => return "→ waiting".to_owned(),
        None => {}
    }
    if starting {
        // Nothing is asked of these: the app is saying where the copy it
        // made this morning is. It says it from the row rather than from
        // the group, so it cannot claim a plan the task is not on.
        return match row.place {
            Place::Day(day) if day == today => "on today's plan".to_owned(),
            place => format!("now in {}", place_label(place, today, dates)),
        };
    }
    // The day this task was a must-do for has passed.
    if row.focus {
        "was focus".to_owned()
    } else {
        String::new()
    }
}

fn the_list(canvas: &mut Canvas, app: &App, review: &Review, column: Column, layout: &mut Layout) {
    let Column { x, width, .. } = column;
    let height = column.height();
    layout.lists.push(ListArea {
        list: List::Review,
        area: Cells {
            x,
            y: column.top,
            width,
            height,
        },
    });

    let today = app.today();
    let lines = lines_of(review, today, app.dates());
    let on = app.cursor(List::Review);
    let writing = app
        .editor()
        .filter(|editor| editor.list == List::Review && editor.field == Field::Renaming);

    let anchor = lines.iter().position(|line| match line {
        Line::Task(row, ..) => on == Some(RowId::Task(row.task)),
        _ => false,
    });
    let first = scroll_to(lines.len(), anchor, height as usize);

    for (at, line) in lines.iter().skip(first).take(height as usize).enumerate() {
        let y = column.top + at as u16;
        let (row, kind, note) = match line {
            Line::Blank => continue,
            Line::Rule(label, count) => {
                group_rule(canvas, x, width, y, label, Some(*count));
                continue;
            }
            Line::Task(row, kind, note) => (row, kind, note),
        };
        match writing.filter(|editor| editor.task == Some(row.task)) {
            Some(editor) => a_title_being_typed(canvas, column, y, *kind, row, editor),
            None => {
                task_row(
                    canvas,
                    column,
                    y,
                    row,
                    Look {
                        kind: *kind,
                        today,
                        dates: app.dates(),
                        narrow: layout.narrow,
                        moving: false,
                        note: Some(note),
                        whole: false,
                    },
                );
            }
        }
        if on == Some(RowId::Task(row.task)) && writing.is_none() {
            canvas.restyle(x, y, width, cursor());
        }
        layout
            .rows
            .push(row_area(List::Review, RowId::Task(row.task), column, y, 1));
    }
}

fn a_title_being_typed(
    canvas: &mut Canvas,
    column: Column,
    y: u16,
    kind: Kind,
    row: &Row,
    editor: &Editor,
) {
    let mark = super::mark_of(row, kind).0;
    title_field(canvas, column.x, column.width, y, mark, editor);
}

// ---- the panel -------------------------------------------------------

/// The outcomes, how far down the list the person is, and the one thing
/// to press. The keys and their names come from the key table, so the
/// panel cannot offer one the dispatcher does not have.
fn panel(canvas: &mut Canvas, review: &Review, column: Column) {
    let Column { x, width, top, .. } = column;
    let (handled, total) = review.progress();
    // A step whose rows are all information asks nothing, so the panel
    // offers no outcomes: they are the keys for the row the cursor is on,
    // and beside a row nobody is being asked about, each one would act on
    // the wrong thing (DESIGN.md section 5).
    let outcomes = input::decisions(review.step());
    let outcomes = if total == 0 { &outcomes[..0] } else { outcomes };

    for (at, outcome) in outcomes.iter().enumerate() {
        let y = top + at as u16;
        canvas.key(x + 2, y, outcome.key);
        canvas.put(x + 8, y, outcome.label, plain());
        if !outcome.note.is_empty() {
            canvas.rput(x + width - 2, y, outcome.note, dim());
        }
    }

    let said = match review.step() {
        ReviewStep::Pile => "handled",
        ReviewStep::Surfaced => "decided",
    };
    let y = if outcomes.is_empty() {
        top
    } else {
        top + outcomes.len() as u16 + 1
    };
    if total == 0 {
        // Nothing was asked, so there is nothing to be part-way through:
        // a full bar over a total of none read as a step already done.
        canvas.put(x + 2, y, "Nothing to decide here.", dim());
    } else {
        canvas.put(x + 2, y, &format!("{handled} of {total} {said}"), dim());
        bar(canvas, x + 2, y + 1, width - 4, handled, total);
    }

    if review.step() == ReviewStep::Pile {
        let moves = input::bindings(KEYS_OF_THE_PILE)
            .iter()
            .find(|binding| binding.label == "move")
            .map_or("j", |binding| binding.shown);
        canvas.put(
            x + 2,
            y + 2,
            &format!("{moves} any order · u undo last"),
            dim(),
        );
    }
    button(canvas, review, column);
}

const KEYS_OF_THE_PILE: crate::input::KeyContext = crate::input::KeyContext::Review {
    step: ReviewStep::Pile,
    last: false,
    asks: true,
    text_field: false,
};

/// How much of the step is answered, as a bar of `w` cells.
fn bar(canvas: &mut Canvas, x: u16, y: u16, w: u16, handled: usize, total: usize) {
    let full = if total == 0 {
        0
    } else {
        // Rounded, so that one row of seven is a tenth of the bar rather
        // than nothing at all.
        ((w as usize * 2 * handled + total) / (2 * total)) as u16
    };
    canvas.put(x, y, &"█".repeat(full as usize), plain());
    canvas.put(
        x + full,
        y,
        &"░".repeat(w.saturating_sub(full) as usize),
        dim(),
    );
}

/// The one thing to press, in a box at the foot of the panel: on to the
/// next step, or off the review and into the day.
fn button(canvas: &mut Canvas, review: &Review, column: Column) {
    let Column {
        x, width, bottom, ..
    } = column;
    let (at, of) = review.steps();
    let (handled, total) = review.progress();
    let mut label = if at == of {
        "Start the day".to_owned()
    } else {
        "Continue".to_owned()
    };
    // What leaving now would leave behind, which is what makes leaving a
    // choice rather than an accident.
    if review.step() == ReviewStep::Pile && handled < total {
        label.push_str(&format!(", {} left on the pile", total - handled));
    }

    let (box_x, box_width, y) = (x + 1, width - 2, bottom - 2);
    super::popup::frame(canvas, box_x, y, box_width, 3);
    let at = canvas.put(box_x + 2, y + 1, &label, plain());
    canvas.key(at + 1, y + 1, "⏎");
}
