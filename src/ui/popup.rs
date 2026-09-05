//! The popups drawn over a page: the command palette, search, and the
//! help overlay.
//!
//! They are lazygit-shaped: a centred box with an accent border, drawn
//! over the panes with nothing behind it dimmed (DESIGN.md section 2).
//! Their contents come from the key table, so they cannot teach a key the
//! dispatcher does not have.

use ratatui::style::{Color, Style};

use super::{Canvas, Rows, accent, bold, count, cursor, dim, plain};
use crate::app::demo::Match;
use crate::app::{App, Popup};
use crate::input::{self, Binding, KeyContext, NotesPane, Pane, PopupKind, ReviewStep};

pub(super) fn draw(canvas: &mut Canvas, app: &App, rows: &Rows) {
    let Some(popup) = app.popup() else {
        return;
    };
    match popup.kind {
        PopupKind::Palette => palette(canvas, app, popup, rows),
        PopupKind::Search => search(canvas, app, popup, rows),
        PopupKind::Help => help(canvas, rows),
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
        &format!("{} matches", results.count()),
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
                canvas.put(x + 6, row, found.title, plain());
                canvas.rput(x + width - 2, row, found.right, dim());
                if found_at == popup.selected {
                    canvas.restyle(x + 1, row, width - 2, cursor());
                }
                found_at += 1;
            }
        }
    }

    let last = y + height - 3;
    divide(canvas, x, last, width);
    if results.is_empty() {
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
    Found(&'a Match, bool),
    Blank,
    Nothing,
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
        text_field: false,
    }
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
