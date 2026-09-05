//! Drawing: application state in, a ratatui frame out, plus the layout of
//! what was drawn.
//!
//! A view is a pure function of application state. This module orders,
//! filters and groups nothing; it formats and places what it is handed.
//!
//! Every colour here is one of the eight ANSI colours or the terminal's
//! own default, and every weight is bold, dim or reverse, so the screen
//! follows an Omarchy theme change live with no code of its own
//! (DESIGN.md section 3).

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};

use crate::app::demo::{self, Chip, ChipKind, PaneView, Row, State};
use crate::app::{App, Layout, List, ListArea, Page, Rect as Cells, RowArea};
use crate::input::{self, NotesPane, Pane, Side};

mod popup;
#[cfg(test)]
mod tests;

/// Under this many columns the two panes collapse to tabs (DESIGN.md
/// section 1).
const NARROW: u16 = 100;

/// The tabs a narrow window has, in the order `h` and `l` walk them.
const TABS: [&str; 3] = ["TODAY", "BACKLOG", "NOTES"];

/// The notes page gives the open note the width; the list is a fixed
/// column beside it rather than half the window.
const NOTES_DIVIDER: u16 = 44;

// ---- the meanings, as terminal colours -------------------------------

/// Text and structure at their ordinary weight.
fn plain() -> Style {
    Style::new()
}

/// Focus items and pane titles.
fn bold() -> Style {
    Style::new().add_modifier(Modifier::BOLD)
}

/// Secondary text and structure: metadata, hints, group labels, the box
/// drawing, and done and waiting rows.
fn dim() -> Style {
    Style::new().add_modifier(Modifier::DIM)
}

/// Where the keyboard is and the one thing to press. Blue stands in for
/// Omarchy's accent token, and it never marks task state.
fn accent() -> Style {
    Style::new().fg(Color::Blue).add_modifier(Modifier::BOLD)
}

/// The cursor row. A terminal does not tell an application what its
/// selection colour is, so reverse video stands in for it: the row takes
/// the foreground as its background and a chip on it keeps its colour as a
/// coloured block.
fn cursor() -> Style {
    Style::new().add_modifier(Modifier::REVERSED)
}

/// A chip's colour is its meaning (DESIGN.md section 3), and the bracketed
/// text beside it is what carries that meaning without colour.
fn chip_style(kind: ChipKind) -> Style {
    match kind {
        ChipKind::Due => Style::new().fg(Color::Yellow),
        ChipKind::Overdue | ChipKind::Pile => Style::new().fg(Color::Red),
        ChipKind::Remind => Style::new().fg(Color::Cyan),
        ChipKind::Waiting => Style::new().fg(Color::Magenta),
        ChipKind::Repeat => dim(),
    }
}

// ---- the canvas ------------------------------------------------------

/// A frame's cells, addressed from the top left of the drawing area.
///
/// Writing a cell resets it first, so a popup drawn over a pane takes none
/// of the pane's weight with it.
struct Canvas<'a> {
    buffer: &'a mut Buffer,
    area: Rect,
}

impl Canvas<'_> {
    fn width(&self) -> u16 {
        self.area.width
    }

    fn height(&self) -> u16 {
        self.area.height
    }

    /// Writes `text` at `x`, and answers where the text ended, whether or
    /// not the edge of the area cut it short.
    fn put(&mut self, x: u16, y: u16, text: &str, style: Style) -> u16 {
        let mut at = x;
        for symbol in text.chars() {
            if at >= self.width() || y >= self.height() {
                break;
            }
            let position = Position::new(self.area.x + at, self.area.y + y);
            if let Some(cell) = self.buffer.cell_mut(position) {
                cell.reset();
                cell.set_char(symbol);
                cell.set_style(style);
            }
            at = at.saturating_add(1);
        }
        x.saturating_add(count(text))
    }

    /// Writes `text` so that it ends just before `right`, and answers
    /// `right`, which is where the next thing to its left ends.
    fn rput(&mut self, right: u16, y: u16, text: &str, style: Style) -> u16 {
        self.put(right.saturating_sub(count(text)), y, text, style);
        right
    }

    /// A run of segments from `x`, separated by `gap` spaces.
    fn segments(&mut self, x: u16, y: u16, parts: &[(String, Style)], gap: u16) -> u16 {
        let mut at = x;
        for (text, style) in parts {
            at = self.put(at, y, text, *style).saturating_add(gap);
        }
        at
    }

    /// The same, ending just before `right`.
    fn rsegments(&mut self, right: u16, y: u16, parts: &[(String, Style)], gap: u16) {
        let text: u16 = parts.iter().map(|(text, _)| count(text)).sum();
        let gaps = gap * (parts.len().saturating_sub(1)) as u16;
        self.segments(right.saturating_sub(text + gaps), y, parts, gap);
    }

    fn hline(&mut self, x: u16, y: u16, width: u16, style: Style) {
        self.put(x, y, &"─".repeat(width as usize), style);
    }

    fn vline(&mut self, x: u16, y: u16, height: u16, style: Style) {
        for row in y..y.saturating_add(height) {
            self.put(x, row, "│", style);
        }
    }

    /// Adds a weight to cells that are already drawn, which is how the
    /// cursor row keeps the colours under it.
    fn restyle(&mut self, x: u16, y: u16, width: u16, style: Style) {
        for at in x..x.saturating_add(width).min(self.width()) {
            let position = Position::new(self.area.x + at, self.area.y + y);
            if let Some(cell) = self.buffer.cell_mut(position) {
                cell.set_style(style);
            }
        }
    }
}

/// How many cells a string takes. Every glyph the app draws is one cell
/// wide.
fn count(text: &str) -> u16 {
    text.chars().count() as u16
}

// ---- the frame -------------------------------------------------------

/// The rows the frame is made of, once the one-cell top and bottom margins
/// are taken off.
struct Rows {
    /// The status line.
    status: u16,
    /// The pane headers, or the tab row when the window is narrow.
    headers: u16,
    /// The first and last row of the body.
    top: u16,
    bottom: u16,
    /// The hint bar.
    hints: u16,
}

pub fn draw(app: &App, frame: &mut Frame) -> Layout {
    let area = frame.area();
    let mut canvas = Canvas {
        buffer: frame.buffer_mut(),
        area,
    };

    // Below this there is no room for a margin, two rules and a row.
    if area.width < 24 || area.height < 9 {
        return Layout::default();
    }

    let rows = Rows {
        status: 1,
        headers: 3,
        top: 5,
        bottom: area.height - 4,
        hints: area.height - 2,
    };
    let narrow = area.width < NARROW;
    let width = area.width;

    canvas.hline(0, rows.status + 1, width, dim());
    canvas.hline(0, rows.headers + 1, width, dim());
    canvas.hline(0, rows.hints - 1, width, dim());

    status_line(&mut canvas, app, rows.status, narrow);
    hint_bar(&mut canvas, app, rows.hints, narrow);

    let mut layout = Layout {
        narrow,
        ..Layout::default()
    };
    if narrow {
        tab_row(&mut canvas, app, rows.headers);
        one_pane(&mut canvas, app, &rows, &mut layout);
    } else {
        two_panes(&mut canvas, app, &rows, &mut layout);
    }

    popup::draw(&mut canvas, app, &rows);
    layout
}

/// A status-line segment at the weight most of them have.
fn quiet(text: &str) -> (String, Style) {
    (text.to_owned(), dim())
}

/// Which day, how to move between days, and the only global indicators.
///
/// A narrow window keeps the indicators and drops the words around them.
fn status_line(canvas: &mut Canvas, app: &App, y: u16, narrow: bool) {
    let fixture = app.fixture();
    let review = fixture.review_count();
    // Red only while there is something on the pile: a zero is a count,
    // not an alert.
    let pile = if review > 0 {
        Style::new().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        dim()
    };
    let day = (format!("Today · {}", demo::TODAY_LABEL), bold());
    let notes = format!("{} notes", fixture.note_count());

    let (left, right) = match (app.page(), narrow) {
        (Page::Home, true) => (
            vec![quiet("‹"), day, quiet("›")],
            vec![
                (format!("● {review}"), pile),
                quiet("/"),
                quiet(":"),
                quiet("?"),
            ],
        ),
        (Page::Home, false) => (
            vec![
                quiet("‹"),
                day,
                quiet("›"),
                quiet("[ ] day"),
                quiet("g go to date"),
            ],
            vec![
                (format!("● {review} in review"), pile),
                quiet(&format!("{notes} n")),
                quiet("/ search"),
                quiet(": commands"),
                quiet("?"),
            ],
        ),
        (Page::Notes, _) => (
            vec![("Notes".to_owned(), bold()), quiet(&notes)],
            vec![
                quiet("n or esc back to today"),
                quiet("/"),
                quiet(":"),
                quiet("?"),
            ],
        ),
    };

    canvas.segments(1, y, &left, 1);
    canvas.rsegments(canvas.width() - 1, y, &right, 3);
}

/// The keys of the focused context, drawn from the key table and from
/// nothing else, so the hint bar cannot disagree with the dispatcher.
fn hint_bar(canvas: &mut Canvas, app: &App, y: u16, narrow: bool) {
    let context = app.key_context();
    let mut left = Vec::new();
    let mut right = Vec::new();
    for binding in input::bindings(context) {
        let bar = if narrow { binding.narrow } else { binding.bar };
        match bar.slot(binding.label) {
            Some((Side::Left, name)) => left.push((binding.shown, name)),
            Some((Side::Right, name)) => right.push((binding.shown, name)),
            None => {}
        }
    }

    let mut x = canvas.put(1, y, input::name(context), bold()) + 2;
    for (shown, name) in left {
        x = canvas.put(x, y, shown, accent());
        x = canvas.put(x + 1, y, name, dim()) + 2;
    }

    let mut edge = canvas.width() - 1;
    for (shown, name) in right.into_iter().rev() {
        edge = canvas.rput(edge, y, name, dim()) - count(name) - 1;
        edge = canvas.rput(edge, y, shown, accent()) - count(shown) - 2;
    }
}

/// The three tabs that stand in for the panes when the window is narrow.
fn tab_row(canvas: &mut Canvas, app: &App, y: u16) {
    let here = match (app.page(), app.pane()) {
        (Page::Notes, _) => 2,
        (Page::Home, Pane::Day) => 0,
        (Page::Home, Pane::Backlog) => 1,
    };
    let mut x = 1;
    for (at, (name, count)) in TABS.iter().zip(app.fixture().tabs()).enumerate() {
        let style = if at == here {
            accent().add_modifier(Modifier::REVERSED)
        } else {
            dim()
        };
        x = canvas.put(x, y, &format!(" {name} {count} "), style) + 1;
    }
}

// ---- the panes -------------------------------------------------------

fn two_panes(canvas: &mut Canvas, app: &App, rows: &Rows, layout: &mut Layout) {
    let width = canvas.width();
    let divider = match app.page() {
        Page::Home => width / 2 - 1,
        Page::Notes => NOTES_DIVIDER.min(width / 2),
    };
    let left = Column {
        x: 0,
        width: divider,
    };
    let right = Column {
        x: divider + 1,
        width: width - divider - 1,
    };

    canvas.put(divider, rows.headers + 1, "\u{252c}", dim());
    canvas.vline(divider, rows.top, rows.bottom - rows.top + 1, dim());

    let on_left = match app.page() {
        Page::Home => app.pane() == Pane::Day,
        Page::Notes => app.notes_pane() == NotesPane::List,
    };
    match app.page() {
        Page::Home => {
            pane(canvas, app, List::Day, left, rows, on_left, layout);
            pane(canvas, app, List::Backlog, right, rows, !on_left, layout);
        }
        Page::Notes => {
            pane(canvas, app, List::Notes, left, rows, on_left, layout);
            open_note(canvas, app, right, rows, !on_left);
        }
    }
}

/// A narrow window shows one list and makes the others tabs. On the notes
/// page the list and the open note stack in the one tab.
fn one_pane(canvas: &mut Canvas, app: &App, rows: &Rows, layout: &mut Layout) {
    let whole = Column {
        x: 0,
        width: canvas.width(),
    };
    pane(canvas, app, app.focused(), whole, rows, true, layout);
}

/// The columns a pane occupies.
#[derive(Clone, Copy)]
struct Column {
    x: u16,
    width: u16,
}

/// A pane: its header, then its groups, then whatever is left blank.
fn pane(
    canvas: &mut Canvas,
    app: &App,
    list: List,
    column: Column,
    rows: &Rows,
    focused: bool,
    layout: &mut Layout,
) {
    let Column { x, width } = column;
    let view = app.view(list);
    // A narrow window puts the tab row where the pane headers would be.
    if !layout.narrow {
        header(canvas, x, width, rows.headers, view, focused);
    }
    layout.lists.push(ListArea {
        list,
        area: Cells {
            x,
            y: rows.top,
            width,
            height: rows.bottom - rows.top + 1,
        },
    });

    if view.is_empty() && !view.empty.what.is_empty() {
        let middle = |text: &str| x + width.saturating_sub(count(text)) / 2;
        canvas.put(
            middle(view.empty.what),
            rows.top + 2,
            view.empty.what,
            dim(),
        );
        canvas.put(
            middle(view.empty.keys),
            rows.top + 3,
            view.empty.keys,
            dim(),
        );
        return;
    }

    let narrow = layout.narrow;
    let notes = list == List::Notes;
    let on = if focused { app.cursor(list) } else { None };
    let mut y = rows.top;

    for (at, group) in view.groups.iter().enumerate() {
        if at > 0 {
            y += 1;
        }
        if !group.label.is_empty() {
            if y > rows.bottom {
                return;
            }
            group_rule(canvas, x, width, y, group.label, group.count);
            y += 1;
        }
        for row in group.rows {
            if y > rows.bottom {
                return;
            }
            if notes {
                note_row(canvas, x, width, y, row);
            } else {
                task_row(canvas, x, width, y, row, narrow);
            }
            if on == Some(row.id) {
                canvas.restyle(x, y, width, cursor());
            }
            layout.rows.push(RowArea {
                list,
                id: row.id,
                area: Cells {
                    x,
                    y,
                    width,
                    height: 1,
                },
            });
            y += 1;
        }
        if let Some(add) = group.add {
            if y > rows.bottom {
                return;
            }
            canvas.put(x + 1, y, &format!(" +  {}", add.label), dim());
            canvas.rput(x + width - 1, y, add.key, accent());
            y += 1;
        }
    }
}

/// `Today Fri 5 Sep                    6 open · 2 done · 1 moved`
fn header(canvas: &mut Canvas, x: u16, width: u16, y: u16, view: &PaneView, focused: bool) {
    let title = if focused { accent() } else { bold() };
    canvas.put(x + 1, y, view.title, title);
    if !view.sub.is_empty() {
        canvas.put(x + 2 + count(view.title), y, view.sub, dim());
    }
    if !view.right.is_empty() {
        canvas.rput(x + width - 1, y, view.right, dim());
    }
}

/// `FOCUS ──────────` and `DONE 2 ────────`.
fn group_rule(canvas: &mut Canvas, x: u16, width: u16, y: u16, label: &str, n: Option<usize>) {
    let text = match n {
        Some(n) => format!("{} {n}", label.to_uppercase()),
        None => label.to_uppercase(),
    };
    canvas.put(x + 1, y, &text, dim());
    canvas.hline(
        x + 2 + count(&text),
        y,
        width.saturating_sub(3 + count(&text)),
        dim(),
    );
}

/// `[ ] Book dentist                          [◷ today]`
fn task_row(canvas: &mut Canvas, x: u16, width: u16, y: u16, row: &Row, narrow: bool) {
    let (mark, mark_style, title_style) = match row.state {
        State::Open if row.focus => ("[ ]", bold(), bold()),
        State::Open => ("[ ]", plain(), plain()),
        State::Done => ("[x]", Style::new().fg(Color::Green), dim()),
        State::Waiting => ("[ ]", dim(), dim()),
        State::Moved => ("[→]", dim(), plain()),
    };
    canvas.put(x + 1, y, mark, mark_style);
    canvas.put(
        x + 5,
        y,
        clip(row.title, width.saturating_sub(6)),
        title_style,
    );

    // The right of the row, filled from its edge inwards: the time it was
    // closed, then the chips, then whatever text is left.
    let mut edge = x + width - 1;
    if !row.closed_at.is_empty() && !narrow {
        edge = canvas.rput(edge, y, row.closed_at, dim()) - count(row.closed_at) - 2;
    }
    for chip in row.chips.iter().rev() {
        let text = chip_text(chip, narrow);
        edge = canvas.rput(edge, y, &format!("[{text}]"), chip_style(chip.kind)) - count(text) - 4;
    }
    // A narrow pane drops the row's words but keeps a moved row's pointer,
    // which is the whole content of the row.
    let keep = !narrow || row.state == State::Moved;
    if !row.meta.is_empty() && keep {
        canvas.rput(edge, y, row.meta, dim());
    }
}

fn chip_text(chip: &Chip, narrow: bool) -> &str {
    if narrow { chip.short } else { chip.text }
}

/// ` ▪ Mention to Anna: CI runner b  yesterday`
fn note_row(canvas: &mut Canvas, x: u16, width: u16, y: u16, row: &Row) {
    canvas.put(x + 1, y, " ▪ ", dim());
    canvas.put(x + 4, y, clip(row.title, width.saturating_sub(16)), plain());
    if !row.meta.is_empty() {
        canvas.rput(x + width - 1, y, row.meta, dim());
    }
}

/// The open note: a plain multi-line text area and nothing else on it.
fn open_note(canvas: &mut Canvas, app: &App, column: Column, rows: &Rows, focused: bool) {
    let Column { x, width } = column;
    let view = app.fixture().open_note();
    header(canvas, x, width, rows.headers, view, focused);

    let body = app.fixture().note();
    for (at, line) in body.iter().enumerate() {
        let y = rows.top + at as u16;
        if y > rows.bottom {
            return;
        }
        canvas.put(x + 2, y, clip(line, width.saturating_sub(3)), plain());
    }
    if let Some(last) = body.len().checked_sub(1) {
        let y = rows.top + last as u16;
        if y <= rows.bottom {
            canvas.put(x + 2 + count(body[last]), y, "▏", bold());
        }
    }
}

fn clip(text: &str, width: u16) -> &str {
    match text.char_indices().nth(width as usize) {
        Some((at, _)) => &text[..at],
        None => text,
    }
}
