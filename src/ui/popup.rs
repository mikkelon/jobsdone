//! The popups drawn over a page: the command palette, search, the help
//! overlay, the move, date and repeat cards, and the two questions, the
//! one a recurring copy asks and the one `confirm_delete` puts in front
//! of `x`.
//!
//! They are lazygit-shaped: a centred box with an accent border, drawn
//! over the panes with nothing behind it dimmed (DESIGN.md section 2).
//! Their contents come from the key table, so they cannot teach a key the
//! dispatcher does not have.

use jiff::Span;
use jiff::civil::{Date, Weekday as Civil};
use ratatui::style::{Color, Modifier, Style};

use super::{
    Canvas, DateOrder, Rows, accent, bold, count, cursor, day_label, dim, place_label, plain,
};
use crate::app::{App, DateDraft, DateKind, MoveTarget, Popup, RepeatDraft, RowId};
use crate::domain::{self, Row, WeekStart, Weekday, WorkDays};
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
        PopupKind::DeleteQuestion => delete_question(canvas, app, popup, rows),
    }
}

/// A box of `width` by `height` at the top left given, cleared and framed.
pub(super) fn frame(canvas: &mut Canvas, x: u16, y: u16, width: u16, height: u16) {
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
///
/// `taken` is how many cells at the right of the box something else has
/// already had, so the line stops short of it rather than running under
/// it (F14).
fn input(canvas: &mut Canvas, x: u16, y: u16, width: u16, prompt: &str, popup: &Popup, taken: u16) {
    canvas.put(x + 2, y, prompt, bold());
    // From under the prompt to the box's other side.
    let room = width.saturating_sub(7 + taken);
    super::caret_line(canvas, x + 5, y, room, &popup.text, popup.caret);
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

    // Two sections, as wireframe 11 draws them: what a key would do to the
    // row the cursor is on, then what it does to the app. The rows come
    // in that order, so the headings fall where the kind changes.
    let mut lines: Vec<Command> = Vec::new();
    let mut section = None;
    for command in &commands {
        let on_row = command.acts_on_the_row();
        if section != Some(on_row) {
            if section.is_some() {
                lines.push(Command::Blank);
            }
            lines.push(Command::Heading(if on_row {
                about_the_row(app)
            } else {
                "APP".to_owned()
            }));
            section = Some(on_row);
        }
        lines.push(Command::Row(command));
    }

    // Two borders, the input, its rule, the footer's rule, and the footer.
    let around = 6;
    let room = (rows.bottom - rows.top + 1).saturating_sub(around) as usize;
    let shown = lines.len().min(room);
    let height = shown as u16 + around;
    let (x, y) = place(canvas, rows, width, height);

    frame(canvas, x, y, width, height);
    input(canvas, x, y + 1, width, ":", popup, 0);
    divide(canvas, x, y + 2, width);

    let mut command_at = 0;
    for (at, line) in lines.iter().take(shown).enumerate() {
        let row = y + 3 + at as u16;
        match line {
            Command::Blank => {}
            Command::Heading(text) => {
                canvas.put(x + 2, row, super::clip(text, width - 4), dim());
            }
            Command::Row(command) => {
                // Undo names what it would take back, which is the one
                // label the table cannot know.
                let label = match (command.keys.first(), app.next_undo()) {
                    (Some((_, Action::Undo)), Some(what)) => {
                        format!("{}: {what}", sentence(command.label))
                    }
                    _ => sentence(command.label),
                };
                let room = width.saturating_sub(6 + super::count(command.shown));
                canvas.put(x + 2, row, super::clip(&label, room), plain());
                canvas.rput(x + width - 2, row, command.shown, accent());
                if command_at == popup.selected {
                    canvas.restyle(x + 1, row, width - 2, cursor());
                }
                command_at += 1;
            }
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

/// One drawn line of the palette.
enum Command<'a> {
    Heading(String),
    Row(&'a Binding),
    Blank,
}

/// What the palette's first section is about: the row the cursor is on,
/// which is what a key in that section would act on. A pane with no row
/// under the cursor falls back to the name the hint bar gives the page.
fn about_the_row(app: &App) -> String {
    let title = match app.cursor(app.focused()) {
        Some(RowId::Task(id)) => app.model().task(id).map(|task| task.title.clone()),
        Some(RowId::Schedule(id)) => app.model().schedule(id).map(|it| it.title.clone()),
        Some(RowId::Note(id)) => app
            .model()
            .note(id)
            .map(|note| note.body.lines().next().unwrap_or_default().to_owned()),
        Some(RowId::Day(day)) => Some(day_label(day, app.dates())),
        // No key of the settings page acts on its row, so its palette is
        // the app's section alone and this heading is never drawn.
        Some(RowId::Setting(_)) | None => None,
    };
    match title {
        Some(title) => format!("FOR \"{}\"", title.to_uppercase()),
        None => input::name(app.page_context()).to_owned(),
    }
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
    // The count is measured first and the query stops two cells short of
    // it, so a long query scrolls with its caret instead of running under
    // the count (F14).
    let matches = super::counted(results.total, "match", "matches");
    let matches = super::clip(&matches, width.saturating_sub(4));
    input(canvas, x, y + 1, width, "/", popup, count(matches) + 2);
    canvas.rput(x + width - 2, y + 1, matches, dim());
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
                // A result is laid out the way a task row is: where the
                // task is now is measured first and the title takes what
                // is left of the line, so a long title stops before the
                // location instead of running through it and out of the
                // box (F15).
                let side = beside(found, *closed, app.today(), app.dates());
                let side = super::clip(&side, width.saturating_sub(8));
                canvas.rput(x + width - 2, row, side, dim());
                let right = (x + width - 2).saturating_sub(count(side) + 2);
                canvas.put(
                    x + 6,
                    row,
                    super::clip(&found.title, right.saturating_sub(x + 6)),
                    plain(),
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
fn beside(found: &Row, closed: bool, today: jiff::civil::Date, dates: DateOrder) -> String {
    let mut parts = vec![place_label(found.place, today, dates)];
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
        Some(RowId::Note(note)) => app.model().note(note).map_or_else(
            || "a note".to_owned(),
            |note| note.body.lines().next().unwrap_or_default().to_owned(),
        ),
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
            MoveTarget::Day(day) => day_label(day, app.dates()),
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

/// The shapes the field reads (DOMAIN.md section 2), shown while it is
/// empty so that they are found without leaving it.
const DATE_SHAPES: &str = "e.g. 12 sep, +3, -3, mon, tomorrow";

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
    let start = app.settings().week_starts_on();
    let weeks = weeks_of(draft.on, start);
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

    // What has been typed, and the day it and the calendar agree on; or
    // that the typed line is not a date yet, since Enter would say so.
    let typed = popup.text.trim();
    let day = if typed.is_empty() || domain::parse_date(typed, app.today()).is_some() {
        day_label(draft.on, app.dates())
    } else {
        "not a date".to_owned()
    };
    let room = width.saturating_sub(6 + count(&day));
    super::caret_line(canvas, x + 3, y + 2, room, &popup.text, popup.caret);
    if typed.is_empty() {
        canvas.put(x + 5, y + 2, DATE_SHAPES, dim());
    }
    canvas.rput(x + width - 2, y + 2, &day, dim());

    for (at, choice) in choices.iter().enumerate() {
        let row = y + 4 + at as u16;
        canvas.put(x + 2, row, choice.key, accent());
        canvas.put(x + 8, row, choice.label, plain());
        let day = match choice.date {
            Some(day) => day_label(day, app.dates()),
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
            start,
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

/// The whole weeks a month is spread over, from the day a week begins
/// on, so that the days either side of it are drawn dim rather than left
/// blank.
fn weeks_of(on: Date, start: WeekStart) -> Vec<Vec<Date>> {
    let first = on.first_of_month();
    let mut day = first
        .nth_weekday_of_month(1, first_column(start))
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

/// The day of the week the leftmost column of the calendar is.
fn first_column(start: WeekStart) -> Civil {
    match start {
        WeekStart::Monday => Civil::Monday,
        WeekStart::Sunday => Civil::Sunday,
    }
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
    start: WeekStart,
) {
    let month = draft.on.strftime("%B %Y").to_string();
    let left = |text: &str| x + CALENDAR.saturating_sub(count(text)) / 2;
    canvas.put(left(&month), y, &month, dim());
    let heads: Vec<&str> = Weekday::week(start)
        .iter()
        .map(|day| super::short_weekday(*day))
        .collect();
    canvas.put(x, y + 1, &format!(" {}", heads.join(" ")), dim());

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
        shape_of(
            canvas,
            x + width - 2,
            row,
            *shape,
            draft,
            at == popup.selected,
            app,
        );
        if at == popup.selected {
            canvas.restyle(x + 1, row, width - 2, cursor());
        }
    }

    let preview: Vec<String> = app
        .repeat_preview()
        .iter()
        .map(|date| day_label(*date, app.dates()))
        .collect();
    let next = if shapes.get(popup.selected) == Some(&Action::StopRepeat) {
        // The one row that ends a schedule rather than describing one, so
        // it has no next date to preview and says what stopping does
        // instead (DOMAIN.md section 10).
        "No new copies; the ones already made stay.".to_owned()
    } else if preview.is_empty() {
        "Next: nothing; this rule falls on no day.".to_owned()
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
    right: u16,
    y: u16,
    shape: Action,
    draft: &RepeatDraft,
    selected: bool,
    app: &App,
) {
    let dates = app.dates();
    let start = app.settings().week_starts_on();
    match shape {
        Action::EveryWorkDay => {
            canvas.rput(
                right,
                y,
                &work_days_label(app.settings().work_days(), start),
                dim(),
            );
        }
        Action::EveryWeek => {
            // Seven cells of four: the days in the set are bracketed and
            // the one the keys are on is marked.
            let left = right.saturating_sub(4 * 7 - 1);
            for (at, day) in Weekday::week(start).iter().enumerate() {
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
            let unit = if draft.weeks == 1 { "week" } else { "weeks" };
            let from = format!("{unit} from {}", day_label(draft.from, dates));
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

/// The work days, as `Mon–Fri` where they run together in the week and
/// as the days themselves where they do not.
fn work_days_label(days: WorkDays, start: WeekStart) -> String {
    let week = Weekday::week(start);
    let at: Vec<usize> = week
        .iter()
        .enumerate()
        .filter(|(_, day)| days.contains(**day))
        .map(|(at, _)| at)
        .collect();
    let named = |at: &usize| super::weekday_name(week[*at]);
    let runs = at.windows(2).all(|pair| pair[1] == pair[0] + 1);
    match at.as_slice() {
        [] => String::new(),
        [one] => named(one).to_owned(),
        [first, .., last] if runs => format!("{}–{}", named(first), named(last)),
        _ => at.iter().map(named).collect::<Vec<_>>().join(" "),
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

/// The question `x` asks while `confirm_delete` is on: the row named in
/// quotes, so that the answer is about the row on screen and not about
/// whichever one the cursor is nearest (DESIGN.md section 8).
fn delete_question(canvas: &mut Canvas, app: &App, popup: &Popup, rows: &Rows) {
    let answers = input::bindings(KeyContext::Popup {
        kind: PopupKind::DeleteQuestion,
        text_field: false,
    });

    let width = CARD_WIDTH.min(canvas.width().saturating_sub(4));
    let height = answers.len() as u16 + 6;
    let (x, y) = place(canvas, rows, width, height);
    card(canvas, x, y, width, height, "Delete", "");

    let asked = format!("Delete \"{}\"?", about(app, popup));
    canvas.put(
        x + 2,
        y + 2,
        super::clip(&asked, width.saturating_sub(4)),
        dim(),
    );
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
            asks: true,
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

    // The settings page shares nothing but the keys that are everywhere,
    // so its column is the whole of its own table.
    let fourth = only(KeyContext::Settings { field: false }, &shared);

    let mut everywhere: Vec<Help> = shared.into_iter().map(Help::Key).collect();
    everywhere.push(Help::Key(&input::CTRL_C));

    vec![
        ("EVERYWHERE", everywhere),
        ("DAY", second),
        ("BACKLOG", third),
        ("SETTINGS", fourth),
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
