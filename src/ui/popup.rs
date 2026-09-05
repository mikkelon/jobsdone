//! The popups drawn over a page: the command palette, search, the help
//! overlay, the move, date and repeat cards, and the question a recurring
//! copy asks.
//!
//! They are lazygit-shaped: a centred box with an accent border, drawn
//! over the panes with nothing behind it dimmed (DESIGN.md section 2).
//! Their contents come from the key table, so they cannot teach a key the
//! dispatcher does not have.

use jiff::Span;
use jiff::civil::{Date, Weekday as Civil};
use ratatui::style::{Color, Modifier, Style};

use super::{Canvas, Rows, accent, bold, count, cursor, day_label, dim, place_label, plain};
use crate::app::{App, DateDraft, DateKind, MoveTarget, Popup, RepeatDraft, RowId};
use crate::domain::{Row, Weekday};
use crate::input::{
    self, Action, Binding, KeyContext, NotesPane, Pane, PopupKind, ReviewStep, Shown,
};

pub(super) fn draw(canvas: &mut Canvas, app: &App, rows: &Rows) {
    let Some(popup) = app.popup() else {
        return;
    };
    match popup.kind {
        PopupKind::Palette => palette(canvas, app, popup, rows),
        PopupKind::Search => search(canvas, app, popup, rows),
        PopupKind::Help => help(canvas, rows),
        PopupKind::Move => move_card(canvas, app, popup, rows),
        PopupKind::Date => date_card(canvas, app, popup, rows),
        PopupKind::Repeat => repeat_card(canvas, app, popup, rows),
        PopupKind::CopyQuestion => copy_question(canvas, app, popup, rows),
    }
}

/// A box of `width` by `height` at the top left given, cleared and framed.
fn frame(canvas: &mut Canvas, x: u16, y: u16, width: u16, height: u16) {
    let inner = width.saturating_sub(2) as usize;
    canvas.put(x, y, &format!("┌{}┐", "─".repeat(inner)), accent());
    for row in y + 1..y + height - 1 {
        canvas.put(x, row, "│", accent());
        canvas.put(x + 1, row, &" ".repeat(inner), plain());
        canvas.put(x + width - 1, row, "│", accent());
    }
    canvas.put(
        x,
        y + height - 1,
        &format!("└{}┘", "─".repeat(inner)),
        accent(),
    );
}

/// The rule that separates a box's input, its list and its footer.
fn divide(canvas: &mut Canvas, x: u16, y: u16, width: u16) {
    canvas.hline(x + 1, y, width - 2, dim());
}

/// `:  wa▏`, with the caret where the next character goes.
fn input(canvas: &mut Canvas, x: u16, y: u16, prompt: &str, popup: &Popup) {
    canvas.put(x + 2, y, prompt, bold());
    let typed: String = popup.text.chars().take(popup.caret).collect();
    let rest: String = popup.text.chars().skip(popup.caret).collect();
    let at = canvas.put(x + 5, y, &typed, plain());
    let at = canvas.put(at, y, "▏", bold());
    canvas.put(at, y, &rest, plain());
}

/// A footer of key-and-label pairs, in the accent so a key never looks
/// like its description. It stops at the box rather than running through
/// its right border.
fn footer(canvas: &mut Canvas, x: u16, y: u16, width: u16, parts: &[(&str, &str)]) {
    let mut at = x + 2;
    let edge = x + width - 2;
    for (key, label) in parts {
        at = canvas.put(at, y, super::clip(key, edge.saturating_sub(at)), accent());
        at = canvas.put(
            at + 1,
            y,
            super::clip(label, edge.saturating_sub(at + 1)),
            dim(),
        ) + 1;
    }
}

/// Where a box of this size sits: centred across, and in the body rows.
fn place(canvas: &Canvas, rows: &Rows, width: u16, height: u16) -> (u16, u16) {
    let x = canvas.width().saturating_sub(width) / 2;
    let body = rows.bottom - rows.top + 1;
    let y = rows.top + body.saturating_sub(height) / 2;
    (x, y)
}

// ---- the command palette ---------------------------------------------

const PALETTE_WIDTH: u16 = 60;

fn palette(canvas: &mut Canvas, app: &App, popup: &Popup, rows: &Rows) {
    let commands = app.palette_rows();
    let width = PALETTE_WIDTH.min(canvas.width().saturating_sub(4));
    // Two borders, the input, its rule, the group label, the footer's
    // rule, and the footer.
    let around = 7;
    let room = (rows.bottom - rows.top + 1).saturating_sub(around) as usize;
    let shown = commands.len().min(room);
    let height = shown as u16 + around;
    let (x, y) = place(canvas, rows, width, height);

    frame(canvas, x, y, width, height);
    input(canvas, x, y + 1, ":", popup);
    divide(canvas, x, y + 2, width);
    canvas.put(x + 2, y + 3, input::name(app.page_context()), dim());

    for (at, command) in commands.iter().take(shown).enumerate() {
        let row = y + 4 + at as u16;
        canvas.put(x + 2, row, &sentence(command.label), plain());
        canvas.rput(x + width - 2, row, command.shown, accent());
        if at == popup.selected {
            canvas.restyle(x + 1, row, width - 2, cursor());
        }
    }

    let last = y + height - 3;
    divide(canvas, x, last, width);
    footer(
        canvas,
        x,
        last + 1,
        width,
        &[
            ("⏎", "run"),
            ("esc", "close · the key on the right is for next time"),
        ],
    );
}

/// A label the hint bar writes in lower case reads as a command with a
/// capital.
fn sentence(label: &str) -> String {
    let mut letters = label.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + letters.as_str(),
        None => String::new(),
    }
}

// ---- search ----------------------------------------------------------

const SEARCH_WIDTH: u16 = 80;

fn search(canvas: &mut Canvas, app: &App, popup: &Popup, rows: &Rows) {
    let results = app.search_results();
    let width = SEARCH_WIDTH.min(canvas.width().saturating_sub(4));

    // The groups, as lines: a heading each, and a blank between them.
    let mut lines: Vec<Line> = Vec::new();
    if !results.open.is_empty() {
        lines.push(Line::Heading("OPEN"));
        lines.extend(results.open.iter().map(|found| Line::Found(found, false)));
    }
    if !results.closed.is_empty() {
        if !lines.is_empty() {
            lines.push(Line::Blank);
        }
        lines.push(Line::Heading("CLOSED"));
        lines.extend(results.closed.iter().map(|found| Line::Found(found, true)));
    }
    if lines.is_empty() {
        lines.push(Line::Nothing);
    }

    let around = 6;
    let room = (rows.bottom - rows.top + 1).saturating_sub(around) as usize;
    let shown = lines.len().min(room);
    let height = shown as u16 + around;
    let (x, y) = place(canvas, rows, width, height);

    frame(canvas, x, y, width, height);
    input(canvas, x, y + 1, "/", popup);
    canvas.rput(
        x + width - 2,
        y + 1,
        &format!("{} matches", results.total),
        dim(),
    );
    divide(canvas, x, y + 2, width);

    let mut found_at = 0;
    for (at, line) in lines.iter().take(shown).enumerate() {
        let row = y + 3 + at as u16;
        match line {
            Line::Blank => {}
            Line::Nothing => {
                canvas.put(x + 2, row, "Nothing matches, open or closed.", dim());
            }
            Line::Heading(text) => {
                canvas.put(x + 2, row, text, dim());
            }
            Line::Found(found, closed) => {
                let (mark, style) = if *closed {
                    ("[x]", Style::new().fg(Color::Green))
                } else {
                    ("[ ]", plain())
                };
                canvas.put(x + 2, row, mark, style);
                canvas.put(x + 6, row, &found.title, plain());
                canvas.rput(
                    x + width - 2,
                    row,
                    &beside(found, *closed, app.today()),
                    dim(),
                );
                if found_at == popup.selected {
                    canvas.restyle(x + 1, row, width - 2, cursor());
                }
                found_at += 1;
            }
        }
    }

    let last = y + height - 3;
    divide(canvas, x, last, width);
    if results.total == 0 {
        let add = format!("add \"{}\" to today", popup.text.trim());
        footer(canvas, x, last + 1, width, &[("⏎", &add), ("esc", "close")]);
    } else {
        footer(
            canvas,
            x,
            last + 1,
            width,
            &[
                ("⏎", "go to that day"),
                ("alt-t", "re-add to today as a new task"),
                ("esc", "close"),
            ],
        );
    }
}

enum Line<'a> {
    Heading(&'static str),
    Found(&'a Row, bool),
    Blank,
    Nothing,
}

/// What a result says about itself on the right: the day it is on, which
/// is the day Enter goes to (DOMAIN.md section 14), and for an open task
/// the flags it carries there.
fn beside(found: &Row, closed: bool, today: jiff::civil::Date) -> String {
    let mut parts = vec![place_label(found.place, today)];
    if !closed {
        if found.focus {
            parts.push("focus".to_owned());
        }
        if found.waiting {
            parts.push("waiting".to_owned());
        }
    }
    if found.repeat.is_some() {
        parts.push("↻".to_owned());
    }
    parts.join(" · ")
}

// ---- the move card and the copy question -----------------------------

const CARD_WIDTH: u16 = 50;

/// A card over the row it is about: what it does in its border, and the
/// task it is about beside that.
fn card(canvas: &mut Canvas, x: u16, y: u16, width: u16, height: u16, title: &str, about: &str) {
    frame(canvas, x, y, width, height);
    let at = canvas.put(
        x + 2,
        y,
        super::clip(&format!(" {title} "), width.saturating_sub(4)),
        accent(),
    );
    if !about.is_empty() {
        canvas.put(
            at,
            y,
            super::clip(&format!("{about} "), (x + width).saturating_sub(at + 2)),
            dim(),
        );
    }
}

/// The task a card is about, by the id it captured when it opened.
fn about(app: &App, popup: &Popup) -> String {
    match popup.target {
        Some(RowId::Task(task)) => app
            .model()
            .live_task(task)
            .map_or_else(|| "a task".to_owned(), |task| task.title.clone()),
        Some(RowId::Schedule(id)) => app
            .model()
            .schedule(id)
            .map_or_else(|| "a task".to_owned(), |schedule| schedule.title.clone()),
        // The card that goes to a day is about no row at all.
        _ => String::new(),
    }
}

/// A card's footer, drawn from the rows of its own key table so that it
/// cannot offer a key the dispatcher does not have.
fn keys_of(context: KeyContext) -> Vec<(&'static str, &'static str)> {
    input::bindings(context)
        .iter()
        .filter_map(|binding| {
            binding
                .bar
                .slot(binding.label)
                .map(|(_, name)| (binding.shown, name))
        })
        .collect()
}

/// The days a task can be sent to, each with the date it works out as.
/// The keys and their names come from the key table; only the dates are
/// the application's.
fn move_card(canvas: &mut Canvas, app: &App, popup: &Popup, rows: &Rows) {
    let choices = app.move_choices();
    let width = CARD_WIDTH.min(canvas.width().saturating_sub(4));
    let height = choices.len() as u16 + 4;
    let (x, y) = place(canvas, rows, width, height);
    card(canvas, x, y, width, height, "Move", &about(app, popup));

    for (at, choice) in choices.iter().enumerate() {
        let row = y + 2 + at as u16;
        canvas.put(x + 2, row, choice.key, accent());
        canvas.put(x + 8, row, choice.label, plain());
        let day = match choice.target {
            MoveTarget::Day(day) => day_label(day),
            MoveTarget::Backlog => "no day".to_owned(),
            MoveTarget::Pick => "calendar".to_owned(),
        };
        canvas.rput(x + width - 2, row, &day, dim());
        if at == popup.selected {
            canvas.restyle(x + 1, row, width - 2, cursor());
        }
    }
}

// ---- the date card ---------------------------------------------------

/// Wide enough for the five picks and their dates, and for a calendar
/// with room around it.
const DATE_WIDTH: u16 = 54;

/// The columns the month grid takes: seven days of two figures, each
/// under a space.
const CALENDAR: u16 = 21;

/// Due by, remind on, or the day the move card was asked to pick. One
/// card with three ways to a date: type it, pick it, or walk the month.
fn date_card(canvas: &mut Canvas, app: &App, popup: &Popup, rows: &Rows) {
    let Some(draft) = popup.date() else {
        return;
    };
    let choices = app.date_choices();
    let weeks = weeks_of(draft.on);
    let width = DATE_WIDTH.min(canvas.width().saturating_sub(4));
    let body = rows.bottom - rows.top + 1;

    // The border, a blank, the field, a blank, the picks, the calendar
    // under a blank, then a blank, the rule, the footer and the border.
    let around = choices.len() as u16 + 8;
    let month = 3 + weeks.len() as u16;
    // A window with no room for the calendar keeps the card that types
    // and picks rather than losing the card altogether.
    let shown = around + month <= body;
    let height = around + if shown { month } else { 0 };
    let (x, y) = place(canvas, rows, width, height);

    card(
        canvas,
        x,
        y,
        width,
        height,
        name_of(draft.kind),
        &about(app, popup),
    );
    if let Some(other) = switch(draft.kind) {
        canvas.rput(x + width - 2, y, other, dim());
    }

    // What has been typed, and the day it and the calendar agree on.
    let typed: String = popup.text.chars().take(popup.caret).collect();
    let rest: String = popup.text.chars().skip(popup.caret).collect();
    let at = canvas.put(x + 3, y + 2, &typed, plain());
    let at = canvas.put(at, y + 2, "▏", bold());
    canvas.put(at, y + 2, &rest, plain());
    canvas.rput(x + width - 2, y + 2, &day_label(draft.on), dim());

    for (at, choice) in choices.iter().enumerate() {
        let row = y + 4 + at as u16;
        canvas.put(x + 2, row, choice.key, accent());
        canvas.put(x + 8, row, choice.label, plain());
        let day = match choice.date {
            Some(day) => day_label(day),
            None => no_date(draft.kind).to_owned(),
        };
        canvas.rput(x + width - 2, row, &day, dim());
    }

    if shown {
        calendar(
            canvas,
            x + 2,
            y + choices.len() as u16 + 5,
            &weeks,
            draft,
            app.today(),
        );
    }
    let last = y + height - 3;
    divide(canvas, x, last, width);
    footer(
        canvas,
        x,
        last + 1,
        width,
        &keys_of(KeyContext::Popup {
            kind: PopupKind::Date,
            text_field: !draft.in_calendar,
        }),
    );
}

/// What the card is called, which is what it is setting.
fn name_of(kind: DateKind) -> &'static str {
    match kind {
        DateKind::Due => "Due by",
        DateKind::Remind => "Remind on",
        DateKind::Move => "Move",
        DateKind::Go => "Go to day",
    }
}

/// The other mode of the card, offered in its border. The move card's
/// day is not a date the task carries, so it switches to nothing.
fn switch(kind: DateKind) -> Option<&'static str> {
    match kind {
        DateKind::Due => Some(" alt-r remind on "),
        DateKind::Remind => Some(" alt-d due by "),
        DateKind::Move | DateKind::Go => None,
    }
}

/// What the "clear date" row answers with.
fn no_date(kind: DateKind) -> &'static str {
    match kind {
        DateKind::Due => "no due date",
        DateKind::Remind => "no reminder",
        DateKind::Move | DateKind::Go => "no day",
    }
}

/// The whole weeks a month is spread over, Monday first, so that the
/// days either side of it are drawn dim rather than left blank.
fn weeks_of(on: Date) -> Vec<Vec<Date>> {
    let first = on.first_of_month();
    let mut day = first
        .nth_weekday_of_month(1, Civil::Monday)
        .unwrap_or(first);
    if day > first {
        day = day.saturating_sub(Span::new().days(7));
    }
    let last = on.last_of_month();

    let mut weeks = Vec::new();
    while day <= last {
        let week: Vec<Date> = (0..7)
            .map(|at| day.saturating_add(Span::new().days(at)))
            .collect();
        day = day.saturating_add(Span::new().days(7));
        weeks.push(week);
    }
    weeks
}

/// The month the card is on: its name, the weekdays, and the days, with
/// the day the card is on marked and today in bold.
fn calendar(
    canvas: &mut Canvas,
    x: u16,
    y: u16,
    weeks: &[Vec<Date>],
    draft: &DateDraft,
    today: Date,
) {
    let month = draft.on.strftime("%B %Y").to_string();
    let left = |text: &str| x + CALENDAR.saturating_sub(count(text)) / 2;
    canvas.put(left(&month), y, &month, dim());
    canvas.put(x, y + 1, " Mo Tu We Th Fr Sa Su", dim());

    for (down, week) in weeks.iter().enumerate() {
        for (across, day) in week.iter().enumerate() {
            let style = if *day == draft.on {
                // The one thing to press Enter on, marked the way the
                // cursor row is.
                accent().add_modifier(Modifier::REVERSED)
            } else if *day == today {
                bold()
            } else if day.month() != draft.on.month() {
                dim()
            } else {
                plain()
            };
            canvas.put(
                x + across as u16 * 3,
                y + 2 + down as u16,
                &format!(" {:>2}", day.day()),
                style,
            );
        }
    }
}

// ---- the repeat card -------------------------------------------------

/// The five shapes of DOMAIN.md section 10 and the end of them all, with
/// what the selected one is adjusted to on the right, and the dates it
/// would fall on next underneath.
fn repeat_card(canvas: &mut Canvas, app: &App, popup: &Popup, rows: &Rows) {
    let Some(draft) = popup.repeat() else {
        return;
    };
    let shapes = crate::app::repeat_shapes();
    let width = DATE_WIDTH.min(canvas.width().saturating_sub(4));
    // The border, a blank, the shapes, a blank, the preview, the rule,
    // the footer, a blank and the border.
    let height = shapes.len() as u16 + 8;
    let (x, y) = place(canvas, rows, width, height);
    card(canvas, x, y, width, height, "Repeat", &about(app, popup));

    for (at, shape) in shapes.iter().enumerate() {
        let row = y + 2 + at as u16;
        let named = input::bindings(KeyContext::Popup {
            kind: PopupKind::Repeat,
            text_field: false,
        })
        .iter()
        .find(|binding| {
            binding
                .keys
                .first()
                .is_some_and(|(_, action)| action == shape)
        });
        let Some(binding) = named else { continue };

        canvas.put(x + 2, row, binding.shown, accent());
        canvas.put(x + 8, row, binding.label, plain());
        shape_of(canvas, x, width, row, *shape, draft, at == popup.selected);
        if at == popup.selected {
            canvas.restyle(x + 1, row, width - 2, cursor());
        }
    }

    let preview: Vec<String> = app
        .repeat_preview()
        .iter()
        .map(|date| day_label(*date))
        .collect();
    let next = if preview.is_empty() {
        "Next: nothing; a repeat with no day never comes round".to_owned()
    } else {
        format!("Next: {}", preview.join(" · "))
    };
    // The preview, its rule and the footer sit at the bottom of the
    // card, above the blank row that keeps the border off the text.
    let last = y + height - 4;
    canvas.put(x + 2, last - 1, super::clip(&next, width - 4), dim());
    divide(canvas, x, last, width);
    footer(
        canvas,
        x,
        last + 1,
        width,
        &keys_of(KeyContext::Popup {
            kind: PopupKind::Repeat,
            text_field: false,
        }),
    );
}

/// What a shape has to say on the right of its row: the days it repeats
/// on, and, on the selected row, what `h` and `l` are pointing at.
fn shape_of(
    canvas: &mut Canvas,
    x: u16,
    width: u16,
    y: u16,
    shape: Action,
    draft: &RepeatDraft,
    selected: bool,
) {
    let right = x + width - 2;
    match shape {
        Action::EveryWorkDay => {
            canvas.rput(right, y, "Mon–Fri", dim());
        }
        Action::EveryWeek => {
            // Seven cells of four: the days in the set are bracketed and
            // the one the keys are on is marked.
            let left = right.saturating_sub(4 * 7 - 1);
            for (at, day) in Weekday::ALL.iter().enumerate() {
                let on = draft.weekdays.contains(day);
                let text = if on {
                    format!("[{}]", super::short_weekday(*day))
                } else {
                    format!(" {} ", super::short_weekday(*day))
                };
                let cell = left + at as u16 * 4;
                canvas.put(cell, y, &text, if on { bold() } else { dim() });
                if selected && at == draft.weekday {
                    canvas.restyle(cell, y, 3, accent().add_modifier(Modifier::REVERSED));
                }
            }
        }
        Action::EveryMonth => {
            let day = super::month_day_label(draft.month_day);
            canvas.rput(right, y, "· or last", dim());
            canvas.rput(right.saturating_sub(10), y, &format!("[ {day} ]"), bold());
        }
        Action::EveryFewWeeks => {
            let from = format!("weeks from {}", day_label(draft.from));
            canvas.rput(right, y, &from, dim());
            canvas.rput(
                right.saturating_sub(count(&from) + 1),
                y,
                &format!("[ {} ]", draft.weeks),
                bold(),
            );
        }
        Action::StopRepeat => {
            canvas.rput(right, y, "copies stay", dim());
        }
        _ => {}
    }
}

/// The one deliberate question, with both answers spelled out because
/// PRODUCT.md gives each of them a meaning (DESIGN.md section 8).
fn copy_question(canvas: &mut Canvas, app: &App, popup: &Popup, rows: &Rows) {
    let answers: Vec<&Binding> = input::bindings(KeyContext::Popup {
        kind: PopupKind::CopyQuestion,
        text_field: false,
    })
    .iter()
    .filter(|binding| binding.shown != "esc")
    .collect();

    let width = CARD_WIDTH.min(canvas.width().saturating_sub(4));
    let height = answers.len() as u16 + 6;
    let (x, y) = place(canvas, rows, width, height);
    card(canvas, x, y, width, height, "Rename", &about(app, popup));

    canvas.put(x + 2, y + 2, "This task repeats. Rename:", dim());
    for (at, answer) in answers.iter().enumerate() {
        let row = y + 4 + at as u16;
        canvas.put(x + 2, row, answer.shown, accent());
        canvas.put(x + 8, row, &sentence(answer.label), plain());
    }
}

// ---- the help overlay ------------------------------------------------

/// One line of a help column.
enum Help {
    Heading(&'static str),
    Key(&'static Binding),
    Blank,
}

fn context(pane: Pane) -> KeyContext {
    KeyContext::Home {
        pane,
        day: Shown::Today,
        field: None,
    }
}

/// The same two panes with the day pane stepped off today, which is what
/// gives them their other set of keys.
fn browsing(pane: Pane) -> KeyContext {
    KeyContext::Home {
        pane,
        day: Shown::Past,
        field: None,
    }
}

/// A row is the same row in two contexts when it says the same thing.
fn same(one: &Binding, other: &Binding) -> bool {
    one.shown == other.shown && one.label == other.label
}

/// Every row of a context that teaches a key.
fn named(context: KeyContext) -> Vec<&'static Binding> {
    input::bindings(context)
        .iter()
        .filter(|binding| !binding.keys.is_empty())
        .collect()
}

/// The same, minus the rows already written somewhere the eye has been:
/// the "everywhere" column, and the heading above this one.
fn only(context: KeyContext, written: &[&'static Binding]) -> Vec<Help> {
    named(context)
        .into_iter()
        .filter(|binding| !written.iter().any(|other| same(other, binding)))
        .map(Help::Key)
        .collect()
}

/// The whole key map, grouped the way the contexts group it. Nothing here
/// is written twice: a row in both home panes is "everywhere".
fn columns() -> Vec<(&'static str, Vec<Help>)> {
    let day = input::bindings(context(Pane::Day));
    let backlog = input::bindings(context(Pane::Backlog));
    let shared: Vec<&'static Binding> = day
        .iter()
        .filter(|binding| !binding.keys.is_empty())
        .filter(|binding| backlog.iter().any(|other| same(binding, other)))
        .collect();

    // A heading takes the keys of the one above it as written already,
    // so the same row is never twice in one column.
    let mut above = shared.clone();
    above.extend(named(context(Pane::Backlog)));
    let mut third = only(context(Pane::Backlog), &shared);
    third.push(Help::Blank);
    third.push(Help::Heading("DAYS"));
    third.extend(only(browsing(Pane::Backlog), &above));
    third.push(Help::Blank);
    third.push(Help::Heading("REVIEW"));
    third.extend(only(
        KeyContext::Review {
            step: ReviewStep::Pile,
            text_field: false,
        },
        &shared,
    ));

    let mut above = shared.clone();
    above.extend(named(context(Pane::Day)));
    let mut second = only(context(Pane::Day), &shared);
    second.push(Help::Blank);
    second.push(Help::Heading("PAST DAY"));
    second.extend(only(browsing(Pane::Day), &above));
    second.push(Help::Blank);
    second.push(Help::Heading("NOTES"));
    second.extend(only(
        KeyContext::Notes {
            pane: NotesPane::List,
            text_field: false,
        },
        &shared,
    ));

    vec![
        (
            "EVERYWHERE",
            shared.into_iter().map(Help::Key).collect::<Vec<_>>(),
        ),
        ("DAY", second),
        ("BACKLOG", third),
    ]
}

fn help(canvas: &mut Canvas, rows: &Rows) {
    let columns = columns();
    let tallest = columns
        .iter()
        .map(|(_, lines)| lines.len())
        .max()
        .unwrap_or(0) as u16;

    let width = canvas.width().saturating_sub(4);
    let body = rows.bottom - rows.top + 1;
    let height = (tallest + 5).min(body);
    let x = 2;
    let y = rows.top + body.saturating_sub(height) / 2;

    frame(canvas, x, y, width, height);
    canvas.put(x + 2, y, " Keys ", accent());
    canvas.rput(x + width - 2, y, " ? or esc close ", dim());

    let column = (width - 6) / columns.len() as u16;
    for (at, (title, lines)) in columns.iter().enumerate() {
        let left = x + 3 + at as u16 * column;
        canvas.put(left, y + 2, title, dim());
        for (down, line) in lines.iter().enumerate() {
            let row = y + 3 + down as u16;
            if row >= y + height - 1 {
                break;
            }
            match line {
                Help::Blank => {}
                Help::Heading(text) => {
                    canvas.put(left, row, text, dim());
                }
                Help::Key(binding) => {
                    let after = canvas.put(left, row, binding.shown, accent());
                    canvas.put(
                        after + 1,
                        row,
                        super::clip(
                            binding.label,
                            column.saturating_sub(count(binding.shown) + 2),
                        ),
                        plain(),
                    );
                }
            }
        }
    }
}
