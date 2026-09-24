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
use crate::app::{App, Card, DateDraft, DateKind, Layout, MoveTarget, Popup, RepeatDraft, RowId};
use crate::domain::{self, Row, WeekStart, Weekday, WorkDays};
use crate::input::{
    self, Action, Binding, KeyContext, NotesList, NotesPane, Pane, PopupKind, ReviewStep, Shown,
};

pub(super) fn draw(canvas: &mut Canvas, app: &App, rows: &Rows, layout: &mut Layout) {
    let Some(popup) = app.popup() else {
        return;
    };
    if canvas.width() < 40 || canvas.height() < 12 {
        for row in 0..canvas.height() {
            canvas.put(0, row, &" ".repeat(canvas.width() as usize), plain());
        }
        canvas.put(0, 0, "Resize to 40x12", accent());
        canvas.put(0, 2, "Esc back · Ctrl+C quit", plain());
        layout.input_blocked = true;
        layout.calendar_available = Some(false);
        return;
    }
    // Small terminals lend their status and hint rows to the card.
    let expanded;
    let rows = if canvas.height() < 22 {
        expanded = Rows {
            top: 0,
            bottom: canvas.height() - 1,
            ..*rows
        };
        &expanded
    } else {
        rows
    };
    if popup.kind == PopupKind::Date {
        layout.calendar_available = Some(calendar_visible(app, canvas.width(), canvas.height()));
    }
    match popup.kind {
        PopupKind::Palette => palette(canvas, app, popup, rows),
        PopupKind::Search => search(canvas, app, popup, rows),
        PopupKind::Help => help(canvas, app, popup, rows, layout),
        PopupKind::Move => move_card(canvas, app, popup, rows),
        PopupKind::Date => date_card(canvas, app, popup, rows),
        PopupKind::Repeat => repeat_card(canvas, app, popup, rows),
        PopupKind::CopyQuestion => copy_question(canvas, app, popup, rows),
        PopupKind::DeleteQuestion => delete_question(canvas, app, popup, rows),
        PopupKind::Spelling => spelling_card(canvas, popup, rows),
        PopupKind::Dictionary => dictionary_card(canvas, app, popup, rows),
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

/// `:  wa█`, with the caret where the next character goes.
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
        at = canvas.key(at, y, super::clip(key, edge.saturating_sub(at)));
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
    let y =
        (rows.top + body.saturating_sub(height) / 2).min(canvas.height().saturating_sub(height));
    (x, y)
}

fn position(canvas: &mut Canvas, x: u16, y: u16, width: u16, selected: usize, total: usize) {
    let label = format!(" {} / {} ↑↓ ", selected.saturating_add(1).min(total), total);
    canvas.rput(
        x + width - 2,
        y,
        super::clip(&label, width.saturating_sub(4)),
        accent(),
    );
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

    let selected_line = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| matches!(line, Command::Row(_)))
        .nth(popup.selected)
        .map(|(index, _)| index)
        .unwrap_or(0);
    let first = super::scroll_to(lines.len(), Some(selected_line), shown);
    if lines.len() > shown {
        position(canvas, x, y, width, popup.selected, commands.len());
    }
    let mut command_at = lines[..first]
        .iter()
        .filter(|line| matches!(line, Command::Row(_)))
        .count();
    for (at, line) in lines.iter().skip(first).take(shown).enumerate() {
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
                canvas.rkey(x + width - 2, row, command.shown);
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

    let selected_line = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| matches!(line, Line::Found(..)))
        .nth(popup.selected)
        .map(|(index, _)| index)
        .unwrap_or(0);
    let first = super::scroll_to(lines.len(), Some(selected_line), shown);
    if lines.len() > shown {
        position(canvas, x, y, width, popup.selected, results.total);
    }
    let mut found_at = lines[..first]
        .iter()
        .filter(|line| matches!(line, Line::Found(..)))
        .count();
    for (at, line) in lines.iter().skip(first).take(shown).enumerate() {
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
        footer(canvas, x, last + 1, width, &[("esc", "close"), ("⏎", &add)]);
    } else {
        footer(
            canvas,
            x,
            last + 1,
            width,
            if width < 70 {
                &[("esc", "close"), ("⏎", "open"), ("alt-t", "copy to today")]
            } else {
                &[
                    ("⏎", "go to that day"),
                    ("alt-t", "re-add to today as a new task"),
                    ("esc", "close"),
                ]
            },
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
///
/// `tight` takes the shorter of the two names each row has, which is the
/// one the hint bar uses in a narrow window: a card with more keys than
/// a footer has room for says all of them in fewer words rather than
/// losing the last of them off the edge.
fn keys_of(context: KeyContext, tight: bool) -> Vec<(&'static str, &'static str)> {
    input::bindings(context)
        .iter()
        .filter_map(|binding| {
            let bar = if tight { binding.narrow } else { binding.bar };
            bar.slot(binding.label)
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
    let shown = choices
        .len()
        .min((rows.bottom - rows.top + 1).saturating_sub(4) as usize);
    let first = super::scroll_to(choices.len(), Some(popup.selected), shown);
    let height = shown as u16 + 4;
    let (x, y) = place(canvas, rows, width, height);
    card(canvas, x, y, width, height, "Move", &about(app, popup));

    for (at, choice) in choices.iter().skip(first).take(shown).enumerate() {
        let row = y + 2 + at as u16;
        canvas.key(x + 2, row, choice.key);
        canvas.put(
            x + 8,
            row,
            super::clip(choice.label, width.saturating_sub(10)),
            plain(),
        );
        let day = match choice.target {
            MoveTarget::Day(day) => day_label(day, app.dates()),
            MoveTarget::Backlog => "no day".to_owned(),
            MoveTarget::Pick => "calendar".to_owned(),
        };
        if width >= 46 {
            canvas.rput(x + width - 2, row, &day, dim());
        }
        if first + at == popup.selected {
            canvas.restyle(x + 1, row, width - 2, cursor());
        }
    }
    footer(
        canvas,
        x,
        y + height - 2,
        width,
        &[("⏎", "move"), ("esc", "back"), ("↑↓", "pick")],
    );
}

/// What the dictionary offers in place of one misspelt word, as a list
/// to walk, with the offer to keep the word under it. The card is about
/// a word rather than a row, so the word itself stands where a card
/// usually names the task it is about.
///
/// A window with no room for every suggestion shows the ones around the
/// one selected rather than losing the card: the list is short, but the
/// card is centred in the panes and a short window has few rows to give
/// it. The row that keeps the word is never scrolled away, because a
/// card with nothing to offer is that row and nothing else.
fn spelling_card(canvas: &mut Canvas, popup: &Popup, rows: &Rows) {
    let Some(spelling) = popup.spelling() else {
        return;
    };
    let width = CARD_WIDTH.min(canvas.width().saturating_sub(4));
    let body = rows.bottom - rows.top + 1;

    // Two borders and a blank row above the words, and under them the
    // rule, the row that keeps the word, and a blank.
    let around = 6;
    let room = body.saturating_sub(around) as usize;
    // With nothing offered, the one line says so, which is the same one
    // row to leave room for.
    let offered = spelling.suggestions.len().max(1);
    let shown = offered.min(room).max(1);
    let height = shown as u16 + around;
    let (x, y) = place(canvas, rows, width, height);
    card(canvas, x, y, width, height, "Spelling", &spelling.word);

    if spelling.suggestions.is_empty() {
        canvas.put(
            x + 2,
            y + 2,
            super::clip(
                "The dictionary has nothing to put in its place.",
                width.saturating_sub(4),
            ),
            dim(),
        );
    } else {
        // The row that keeps the word is under the list rather than in
        // it, so the list scrolls to the last suggestion at most.
        let last = spelling.suggestions.len() - 1;
        let first = super::scroll_to(
            spelling.suggestions.len(),
            Some(popup.selected.min(last)),
            shown,
        );
        for (at, word) in spelling
            .suggestions
            .iter()
            .skip(first)
            .take(shown)
            .enumerate()
        {
            let row = y + 2 + at as u16;
            canvas.put(
                x + 2,
                row,
                super::clip(word, width.saturating_sub(4)),
                plain(),
            );
            if first + at == popup.selected {
                canvas.restyle(x + 1, row, width - 2, cursor());
            }
        }
    }

    // The offer that answers a word the dictionary was never going to
    // know, which is a different kind of answer from the words above it
    // and is drawn under a rule for that reason.
    let rule = y + 2 + shown as u16;
    divide(canvas, x, rule, width);
    let keep = rule + 1;
    // The word is in the card's border already, so the row says what it
    // does and nothing else: a long name would otherwise push the action
    // off the edge of a narrow card.
    canvas.put(
        x + 2,
        keep,
        super::clip("Add to dictionary", width.saturating_sub(4)),
        plain(),
    );
    if popup.selected == spelling.add_row() {
        canvas.restyle(x + 1, keep, width - 2, cursor());
    }
    footer(
        canvas,
        x,
        y + height - 2,
        width,
        &[("⏎", "choose"), ("esc", "back"), ("↑↓", "pick")],
    );
}

// ---- the personal dictionary -----------------------------------------

/// Wide enough for a word and the five keys of the footer.
const DICTIONARY_WIDTH: u16 = 60;

/// The words the checker is told to know, as a list to walk, with the
/// field open over it while one is being written.
///
/// The list is the model's, in the order its keys sort, so the words
/// read alphabetically whatever case they were typed in. An empty
/// dictionary says what the list is for and names the key that fills it
/// (DESIGN.md section 10).
fn dictionary_card(canvas: &mut Canvas, app: &App, popup: &Popup, rows: &Rows) {
    let words = app.dictionary_rows();
    let writing = popup
        .dictionary()
        .and_then(|draft| draft.field.as_ref())
        .is_some();
    let width = DICTIONARY_WIDTH.min(canvas.width().saturating_sub(4));
    let body = rows.bottom - rows.top + 1;

    // Two borders, a blank row above the words, the rule and the footer,
    // and the field between them while there is one.
    let around = 5 + u16::from(writing);
    let room = body.saturating_sub(around) as usize;
    let lines = words.len().max(1);
    let shown = lines.min(room).max(1);
    let height = shown as u16 + around;
    let (x, y) = place(canvas, rows, width, height);
    card(canvas, x, y, width, height, "Personal dictionary", "");

    if words.is_empty() {
        canvas.put(
            x + 2,
            y + 2,
            super::clip("No words yet. Press a to add one.", width.saturating_sub(4)),
            dim(),
        );
    } else {
        let first = super::scroll_to(words.len(), Some(popup.selected), shown);
        for (at, (_, word)) in words.iter().skip(first).take(shown).enumerate() {
            let row = y + 2 + at as u16;
            canvas.put(
                x + 2,
                row,
                super::clip(word, width.saturating_sub(4)),
                plain(),
            );
            // The row keeps its mark while a word is being written over
            // it, because that is the row being written.
            if first + at == popup.selected {
                canvas.restyle(x + 1, row, width - 2, cursor());
            }
        }
    }

    let rule = y + 2 + shown as u16;
    divide(canvas, x, rule, width);
    if writing {
        canvas.put(x + 2, rule + 1, "word", bold());
        super::caret_line(
            canvas,
            x + 7,
            rule + 1,
            width.saturating_sub(9),
            &popup.text,
            popup.caret,
        );
    }
    let context = KeyContext::Popup {
        kind: PopupKind::Dictionary,
        text_field: writing,
    };
    if width < 50 && !writing {
        footer(
            canvas,
            x,
            y + height - 2,
            width,
            &[("esc", "back"), ("a", "add"), ("e", "edit"), ("x", "del")],
        );
    } else {
        footer(canvas, x, y + height - 2, width, &keys_of(context, true));
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
pub(super) fn calendar_visible(app: &App, width: u16, height: u16) -> bool {
    let Some(draft) = app.popup().and_then(Popup::date) else {
        return false;
    };
    let body = if height < 22 {
        height
    } else {
        height.saturating_sub(8)
    };
    let weeks = weeks_of(draft.on, app.settings().week_starts_on()).len() as u16;
    width >= 58 && body >= app.date_choices().len() as u16 + 11 + weeks
}

fn compact_date(canvas: &mut Canvas, app: &App, popup: &Popup, rows: &Rows, width: u16) {
    let Some(draft) = popup.date() else { return };
    let choices = app.date_choices();
    let height = choices.len() as u16 + 7;
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
    super::caret_line(canvas, x + 2, y + 1, width - 4, &popup.text, popup.caret);
    let resolved = if popup.text.trim().is_empty()
        || domain::parse_date(popup.text.trim(), app.today()).is_some()
    {
        day_label(draft.on, app.dates())
    } else {
        "Not a date: try tomorrow or +3".to_owned()
    };
    canvas.put(x + 2, y + 2, super::clip(&resolved, width - 4), dim());
    let switches = matches!(draft.kind, DateKind::Due | DateKind::Remind);
    if switches {
        footer(
            canvas,
            x,
            y + 3,
            width,
            &[("alt-d", "due"), ("alt-r", "remind")],
        );
    } else {
        canvas.put(x + 2, y + 3, "Type a date, e.g. +3 or mon", dim());
    }
    for (at, choice) in choices.iter().enumerate() {
        let row = y + 4 + at as u16;
        canvas.key(x + 2, row, choice.key);
        canvas.put(x + 8, row, choice.label, plain());
    }
    divide(canvas, x, y + height - 3, width);
    footer(
        canvas,
        x,
        y + height - 2,
        width,
        &[("⏎", "set"), ("esc", "cancel")],
    );
}

fn date_card(canvas: &mut Canvas, app: &App, popup: &Popup, rows: &Rows) {
    let Some(draft) = popup.date() else {
        return;
    };
    let choices = app.date_choices();
    let start = app.settings().week_starts_on();
    let weeks = weeks_of(draft.on, start);
    let width = DATE_WIDTH.min(canvas.width().saturating_sub(4));

    // The border, a blank, the field, a blank, the picks, the calendar
    // under a blank, then a blank, the rule, the footer and the border.
    let around = choices.len() as u16 + 8;
    let month = 3 + weeks.len() as u16;
    // A window with no room for the calendar keeps the card that types
    // and picks rather than losing the card altogether.
    let shown = calendar_visible(app, canvas.width(), canvas.height());
    if !shown {
        compact_date(canvas, app, popup, rows, width);
        return;
    }
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
        canvas.key(x + 2, row, choice.key);
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
        &keys_of(
            KeyContext::Popup {
                kind: PopupKind::Date,
                text_field: !draft.in_calendar,
            },
            false,
        ),
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
/// blank. Cells beyond the civil date range are empty.
fn weeks_of(on: Date, start: WeekStart) -> Vec<Vec<Option<Date>>> {
    let first = on.first_of_month();
    let before = (first.weekday().to_monday_zero_offset()
        - first_column(start).to_monday_zero_offset())
    .rem_euclid(7) as usize;
    let rows = (before + on.last_of_month().day() as usize).div_ceil(7);
    // Derive each cell from the first representable day of the month.
    // There are at most six rows, even where adding a week would overflow.
    (0..rows)
        .map(|row| {
            (0..7)
                .map(|column| {
                    let offset = (row * 7 + column) as i64 - before as i64;
                    first.checked_add(Span::new().days(offset)).ok()
                })
                .collect()
        })
        .collect()
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
    weeks: &[Vec<Option<Date>>],
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
            let Some(day) = day else { continue };
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
    let compact = width < 50;
    let around = 8 + u16::from(compact);
    let shown = shapes
        .len()
        .min((rows.bottom - rows.top + 1).saturating_sub(around).max(1) as usize);
    let first = super::scroll_to(shapes.len(), Some(popup.selected), shown);
    let height = shown as u16 + around;
    let (x, y) = place(canvas, rows, width, height);
    card(canvas, x, y, width, height, "Repeat", &about(app, popup));

    for (at, shape) in shapes.iter().enumerate().skip(first).take(shown) {
        let row = y + 2 + (at - first) as u16;
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

        canvas.key(x + 2, row, binding.shown);
        canvas.put(x + 8, row, binding.label, plain());
        if !compact || at == popup.selected {
            shape_of(
                canvas,
                x + width - 2,
                if compact { y + 2 + shown as u16 } else { row },
                *shape,
                draft,
                at == popup.selected,
                app,
            );
        }
        if at == popup.selected {
            canvas.restyle(x + 1, row, width - 2, cursor());
        }
    }

    if shapes.len() > shown {
        position(canvas, x, y, width, popup.selected, shapes.len());
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
        &[("⏎", "save"), ("esc", "back"), ("↑↓", "pick")],
    );
    footer(
        canvas,
        x,
        y + height - 2,
        width,
        &[("h/l", "adjust"), ("space", "toggle day")],
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
        canvas.key(x + 2, row, answer.shown);
        canvas.put(
            x + 8,
            row,
            super::clip(&sentence(answer.label), width.saturating_sub(10)),
            plain(),
        );
    }
    footer(canvas, x, y + height - 2, width, &[("esc", "cancel")]);
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
        canvas.key(x + 2, row, answer.shown);
        canvas.put(
            x + 8,
            row,
            super::clip(&sentence(answer.label), width.saturating_sub(10)),
            plain(),
        );
    }
}

// ---- the help overlay ------------------------------------------------

/// Help starts with the originating context; Tab expands it to every mode.
struct HelpLine {
    key: String,
    text: String,
    heading: bool,
}

fn help_sections(context: KeyContext, all: bool) -> Vec<(&'static str, KeyContext)> {
    let mut sections = vec![(input::name(context), context)];
    if all {
        // The keys of the width the window is at, since `h`/`l` and `tab`
        // mean different things at the two.
        let narrow = match context {
            KeyContext::Home { narrow, .. } | KeyContext::Notes { narrow, .. } => narrow,
            _ => false,
        };
        let home = |pane, day| KeyContext::Home {
            pane,
            day,
            field: None,
            narrow,
        };
        let mut others = vec![
            ("TODAY", home(Pane::Day, Shown::Today)),
            ("BACKLOG", home(Pane::Backlog, Shown::Today)),
            ("PAST / FUTURE DAY", home(Pane::Day, Shown::Past)),
            ("DAYS", home(Pane::Backlog, Shown::Past)),
            (
                "ADD TASK",
                KeyContext::Home {
                    pane: Pane::Day,
                    day: Shown::Today,
                    field: Some(input::Field::Adding),
                    narrow,
                },
            ),
            (
                "RENAME TASK",
                KeyContext::Home {
                    pane: Pane::Day,
                    day: Shown::Today,
                    field: Some(input::Field::Renaming),
                    narrow,
                },
            ),
            (
                "REVIEW PILE",
                KeyContext::Review {
                    step: ReviewStep::Pile,
                    last: false,
                    asks: true,
                    text_field: false,
                },
            ),
            (
                "DUE & REMINDERS",
                KeyContext::Review {
                    step: ReviewStep::Surfaced,
                    last: true,
                    asks: true,
                    text_field: false,
                },
            ),
            (
                "STACK",
                KeyContext::Notes {
                    pane: NotesPane::List,
                    list: NotesList::Stack,
                    text_field: false,
                    narrow,
                },
            ),
            (
                "ARCHIVE",
                KeyContext::Notes {
                    pane: NotesPane::List,
                    list: NotesList::Archive,
                    text_field: false,
                    narrow,
                },
            ),
            (
                "NOTES FILTER",
                KeyContext::Notes {
                    pane: NotesPane::Filter,
                    list: NotesList::Stack,
                    text_field: true,
                    narrow,
                },
            ),
            (
                "SCRATCHPAD",
                KeyContext::Notes {
                    pane: NotesPane::Note,
                    list: NotesList::Stack,
                    text_field: true,
                    narrow,
                },
            ),
            ("SETTINGS", KeyContext::Settings { field: false }),
            ("SETTING INPUT", KeyContext::Settings { field: true }),
            ("GO (AFTER g)", KeyContext::Leader { over: None }),
        ];
        for kind in [
            PopupKind::Search,
            PopupKind::Palette,
            PopupKind::Move,
            PopupKind::Date,
            PopupKind::Repeat,
            PopupKind::CopyQuestion,
            PopupKind::DeleteQuestion,
            PopupKind::Spelling,
            PopupKind::Dictionary,
        ] {
            let ctx = KeyContext::Popup {
                kind,
                text_field: matches!(
                    kind,
                    PopupKind::Search | PopupKind::Palette | PopupKind::Date
                ),
            };
            others.push((input::name(ctx), ctx));
        }
        others.push((
            "CALENDAR",
            KeyContext::Popup {
                kind: PopupKind::Date,
                text_field: false,
            },
        ));
        others.push((
            "DICTIONARY INPUT",
            KeyContext::Popup {
                kind: PopupKind::Dictionary,
                text_field: true,
            },
        ));
        sections.extend(others.into_iter().filter(|(_, ctx)| *ctx != context));
    }
    sections
}

fn help(canvas: &mut Canvas, _app: &App, popup: &Popup, rows: &Rows, layout: &mut Layout) {
    let Card::Help { context, all } = popup.card else {
        return;
    };
    let width = canvas.width().saturating_sub(4).min(90);
    let key_width = 18.min(width.saturating_sub(12));
    let text_width = width.saturating_sub(key_width + 5).max(1);
    let mut lines = Vec::new();
    for (title, ctx) in help_sections(context, all) {
        lines.push(HelpLine {
            key: String::new(),
            text: title.to_owned(),
            heading: true,
        });
        for binding in input::bindings(ctx)
            .iter()
            .filter(|binding| !binding.keys.is_empty())
        {
            for (at, (part, _)) in super::wrapped(binding.label, text_width)
                .into_iter()
                .enumerate()
            {
                lines.push(HelpLine {
                    key: if at == 0 {
                        binding.shown.to_owned()
                    } else {
                        String::new()
                    },
                    text: part,
                    heading: false,
                });
            }
        }
        if ctx.text_field() {
            for (key, label) in [
                ("home/end", "start/end of displayed row"),
                ("backspace/delete", "delete before/after caret"),
                ("shift+arrows", "select text"),
                ("shift-home/end", "select to row edge"),
                ("ctrl-shift-←/→", "select by word"),
                ("ctrl-a", "select all"),
                ("ctrl-←/→", "move by word"),
                ("ctrl-backspace", "delete previous word"),
                ("ctrl-shift-c", "copy selection"),
                ("ctrl-insert", "copy selection"),
                (
                    "alt-y",
                    if matches!(ctx, KeyContext::Notes { .. }) {
                        "copy selection; whole note if none"
                    } else {
                        "copy selection"
                    },
                ),
                ("ctrl-shift-x", "cut selection"),
                ("ctrl-shift-v", "paste"),
                ("shift-insert", "paste"),
                ("ctrl-x", "cut selection"),
                ("ctrl-v", "paste"),
            ] {
                for (at, (part, _)) in super::wrapped(label, text_width).into_iter().enumerate() {
                    lines.push(HelpLine {
                        key: if at == 0 {
                            key.to_owned()
                        } else {
                            String::new()
                        },
                        text: part,
                        heading: false,
                    });
                }
            }
        }
        lines.push(HelpLine {
            key: String::new(),
            text: String::new(),
            heading: false,
        });
    }
    for (at, (part, _)) in super::wrapped(input::CTRL_C.label, text_width)
        .into_iter()
        .enumerate()
    {
        lines.push(HelpLine {
            key: if at == 0 {
                input::CTRL_C.shown.to_owned()
            } else {
                String::new()
            },
            text: part,
            heading: false,
        });
    }
    layout.help_lines = lines.len();
    let room = (rows.bottom - rows.top + 1).saturating_sub(4).max(1) as usize;
    let shown = lines.len().min(room);
    let selected = popup.selected.min(lines.len().saturating_sub(1));
    let first = super::scroll_to(lines.len(), Some(selected), shown);
    let height = shown as u16 + 4;
    let (x, y) = place(canvas, rows, width, height);
    card(
        canvas,
        x,
        y,
        width,
        height,
        "Keys",
        if all { "all modes" } else { "current mode" },
    );
    position(canvas, x, y, width, selected, lines.len());
    for (at, line) in lines.iter().skip(first).take(shown).enumerate() {
        let row = y + 1 + at as u16;
        if line.heading {
            canvas.put(x + 2, row, super::clip(&line.text, width - 4), accent());
        } else {
            canvas.key(x + 2, row, &line.key);
            canvas.put(x + key_width + 3, row, &line.text, plain());
        }
    }
    divide(canvas, x, y + height - 3, width);
    footer(
        canvas,
        x,
        y + height - 2,
        width,
        &[
            ("?/esc", "close"),
            ("↑/↓", "scroll"),
            ("tab", if all { "current keys" } else { "all keys" }),
        ],
    );
}

#[cfg(test)]
#[path = "popup_calendar_tests.rs"]
mod calendar_boundary_tests;
