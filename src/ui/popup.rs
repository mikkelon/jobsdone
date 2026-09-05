//! The popups drawn over a page: the command palette, search, the help
//! overlay, the move card and the question a recurring copy asks.
//!
//! They are lazygit-shaped: a centred box with an accent border, drawn
//! over the panes with nothing behind it dimmed (DESIGN.md section 2).
//! Their contents come from the key table, so they cannot teach a key the
//! dispatcher does not have.

use ratatui::style::{Color, Style};

use super::{Canvas, Rows, accent, bold, count, cursor, day_label, dim, place_label, plain};
use crate::app::{App, MoveTarget, Popup};
use crate::domain::Row;
use crate::input::{self, Binding, KeyContext, NotesPane, Pane, PopupKind, ReviewStep};

pub(super) fn draw(canvas: &mut Canvas, app: &App, rows: &Rows) {
    let Some(popup) = app.popup() else {
        return;
    };
    match popup.kind {
        PopupKind::Palette => palette(canvas, app, popup, rows),
        PopupKind::Search => search(canvas, app, popup, rows),
        PopupKind::Help => help(canvas, rows),
        PopupKind::Move => move_card(canvas, app, popup, rows),
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

/// What a result says about itself on the right: where an open task is,
/// and when a closed one was closed.
fn beside(found: &Row, closed: bool, today: jiff::civil::Date) -> String {
    let mut parts = Vec::new();
    if closed {
        if let Some(at) = &found.closed_at {
            parts.push(day_label(at.date()));
        }
    } else {
        parts.push(place_label(found.place, today));
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

/// A card over the row it is about, named after the task in its border.
fn card(canvas: &mut Canvas, rows: &Rows, title: &str, lines: u16) -> (u16, u16) {
    let width = CARD_WIDTH.min(canvas.width().saturating_sub(4));
    let height = lines + 4;
    let (x, y) = place(canvas, rows, width, height);
    frame(canvas, x, y, width, height);
    canvas.put(
        x + 2,
        y,
        super::clip(&format!(" {title} "), width.saturating_sub(4)),
        accent(),
    );
    (x, y)
}

/// The days a task can be sent to, each with the date it works out as.
/// The keys and their names come from the key table; only the dates are
/// the application's.
fn move_card(canvas: &mut Canvas, app: &App, popup: &Popup, rows: &Rows) {
    let choices = app.move_choices();
    let name = popup
        .target
        .and_then(|task| app.model().live_task(task))
        .map_or("a task", |task| task.title.as_str());
    let width = CARD_WIDTH.min(canvas.width().saturating_sub(4));
    let (x, y) = card(canvas, rows, &format!("Move {name}"), choices.len() as u16);

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

    let name = popup
        .target
        .and_then(|task| app.model().live_task(task))
        .map_or("a task", |task| task.title.as_str());
    let (x, y) = card(
        canvas,
        rows,
        &format!("Rename {name}"),
        answers.len() as u16 + 2,
    );

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
    KeyContext::Home { pane, field: None }
}

/// A row is the same row in two contexts when it says the same thing.
fn same(one: &Binding, other: &Binding) -> bool {
    one.shown == other.shown && one.label == other.label
}

/// Every row of a context that teaches a key, minus the ones that are
/// everywhere.
fn only(context: KeyContext, shared: &[&'static Binding]) -> Vec<Help> {
    input::bindings(context)
        .iter()
        .filter(|binding| !binding.keys.is_empty())
        .filter(|binding| !shared.iter().any(|common| same(common, binding)))
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

    let mut third = only(context(Pane::Backlog), &shared);
    third.push(Help::Blank);
    third.push(Help::Heading("REVIEW"));
    third.extend(only(
        KeyContext::Review {
            step: ReviewStep::Pile,
            text_field: false,
        },
        &shared,
    ));

    let mut second = only(context(Pane::Day), &shared);
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
