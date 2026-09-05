//! Drawing: application state in, a ratatui frame out, plus the layout of
//! what was drawn.
//!
//! A view is a pure function of application state. This module orders,
//! filters and groups nothing; it formats and places what the domain's
//! views hand it.
//!
//! Every colour here is one of the eight ANSI colours or the terminal's
//! own default, and every weight is bold, dim or reverse, so the screen
//! follows an Omarchy theme change live with no code of its own
//! (DESIGN.md section 3).

use jiff::civil::Date;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};

use crate::app::{App, Editor, Layout, List, ListArea, Page, Rect as Cells, RowArea};
use crate::domain::{self, MonthDay, NoteRow, Place, Rule, Weekday};
use crate::input::{self, Field, NotesPane, Pane, Side};

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

/// The caret of a text field, which is a cell of its own rather than a
/// terminal cursor, so that it sits where the text does.
const CARET: &str = "▏";

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

// ---- dates, rules and places, as words -------------------------------

/// `Fri 5 Sep`, which is how every date in the program is written.
fn day_label(date: Date) -> String {
    date.strftime("%a %-d %b").to_string()
}

/// A date beside today: the word where there is one, the date otherwise.
fn when(date: Date, today: Date) -> String {
    if date == today {
        "today".to_owned()
    } else {
        date.strftime("%-d %b").to_string()
    }
}

/// Where a task is now, which is what a moved row points at.
fn place_label(place: Place, today: Date) -> String {
    match place {
        Place::Backlog => "backlog".to_owned(),
        Place::Day(day) if day == today => "today".to_owned(),
        Place::Day(day) => day_label(day),
    }
}

fn weekday_name(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "Mon",
        Weekday::Tue => "Tue",
        Weekday::Wed => "Wed",
        Weekday::Thu => "Thu",
        Weekday::Fri => "Fri",
        Weekday::Sat => "Sat",
        Weekday::Sun => "Sun",
    }
}

/// A repeat rule in the few words a chip has room for.
fn rule_label(rule: &Rule) -> String {
    match rule {
        Rule::Workdays => "work days".to_owned(),
        Rule::Daily => "every day".to_owned(),
        Rule::Weekly { weekdays } => {
            let days: Vec<&str> = weekdays.iter().map(|day| weekday_name(*day)).collect();
            format!("every {}", days.join(", "))
        }
        Rule::Monthly { day } => match day {
            MonthDay::Day(day) => format!("{day}{} of the month", ordinal(*day)),
            MonthDay::Last => "last of the month".to_owned(),
        },
        Rule::EveryNWeeks { n, .. } => format!("every {n} weeks"),
    }
}

fn ordinal(day: u8) -> &'static str {
    match (day % 10, day % 100) {
        (1, 1 | 21 | 31) => "st",
        (2, 2 | 22) => "nd",
        (3, 3 | 23) => "rd",
        _ => "th",
    }
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
    let review = app.review_count();
    // Red only while there is something on the pile: a zero is a count,
    // not an alert.
    let pile = if review > 0 {
        Style::new().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        dim()
    };
    let day = (format!("Today · {}", day_label(app.today())), bold());
    let count = app.notes().count;
    let notes = format!("{count} note{}", if count == 1 { "" } else { "s" });

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
///
/// What just happened takes the bar over until the next key, because an
/// action that can be undone has to say so somewhere (DESIGN.md section
/// 8).
fn hint_bar(canvas: &mut Canvas, app: &App, y: u16, narrow: bool) {
    let context = app.key_context();
    let mut x = canvas.put(1, y, input::name(context), bold()) + 2;

    if let Some(message) = app.message() {
        x = canvas.put(x, y, &message.text, plain()) + 2;
        if message.undo {
            x = canvas.put(x, y, "u", accent());
            canvas.put(x + 2, y, "undo", dim());
        }
        return;
    }

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
    let counts = [app.day().counts.open, app.backlog().open, app.notes().count];
    let mut x = 1;
    for (at, (name, count)) in TABS.iter().zip(counts).enumerate() {
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
        top: rows.top,
        bottom: rows.bottom,
    };
    let right = Column {
        x: divider + 1,
        width: width - divider - 1,
        top: rows.top,
        bottom: rows.bottom,
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
            open_note(canvas, app, right, Some(rows.headers), !on_left);
        }
    }
}

/// A narrow window shows one list and makes the others tabs. On the notes
/// page the list and the open note stack in the one tab, with the note's
/// header as the rule between them.
fn one_pane(canvas: &mut Canvas, app: &App, rows: &Rows, layout: &mut Layout) {
    let whole = Column {
        x: 0,
        width: canvas.width(),
        top: rows.top,
        bottom: rows.bottom,
    };
    // Too short to stack the note under the list: the list is the tab.
    if app.page() != Page::Notes || whole.height() < 6 {
        pane(canvas, app, app.focused(), whole, rows, true, layout);
        return;
    }

    // The list keeps to what it has to draw, and never more than half the
    // tab, so the note has the rest.
    let wanted = lines_of(&notes_pane(app)).len() as u16;
    let height = wanted.clamp(2, whole.height() / 2);
    let list = Column {
        bottom: whole.top + height - 1,
        ..whole
    };
    let note = Column {
        top: list.bottom + 2,
        ..whole
    };
    let on_list = app.notes_pane() == NotesPane::List;
    pane(canvas, app, List::Notes, list, rows, on_list, layout);
    open_note(canvas, app, note, None, !on_list);
}

/// The cells a pane occupies: its columns, and the first and last row of
/// its body. Two panes side by side have the same rows; the notes page in
/// one tab stacks them, so the rows are the pane's own.
#[derive(Clone, Copy)]
struct Column {
    x: u16,
    width: u16,
    top: u16,
    bottom: u16,
}

impl Column {
    fn height(self) -> u16 {
        (self.bottom + 1).saturating_sub(self.top)
    }
}

/// Everything about a row that is not in the row: which group it is in,
/// the day the pane is showing, whether the pane has room for words, and
/// whether this row is the one being carried up or down.
#[derive(Clone, Copy)]
struct Look {
    kind: Kind,
    today: Date,
    narrow: bool,
    moving: bool,
}

/// How a row is drawn, which is what its group says rather than anything
/// the drawing works out for itself.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Open,
    Done,
    Waiting,
    Moved,
}

/// The rows of one group of a pane.
enum Content<'a> {
    Tasks(&'a [domain::Row], Kind),
    Notes(&'a [NoteRow]),
}

/// A group under its rule, with the `+ add` line that may close it.
struct Section<'a> {
    label: &'static str,
    count: Option<usize>,
    content: Content<'a>,
    add: Option<&'static str>,
}

/// A pane: what its header says, its groups, and what it says instead
/// when it has nothing in it.
struct PaneView<'a> {
    title: &'static str,
    sub: String,
    right: String,
    sections: Vec<Section<'a>>,
    /// What the list is for and the keys that fill it (DESIGN.md section
    /// 10).
    empty: [&'static str; 2],
}

/// Today, or whichever day the day pane is showing.
fn day_pane<'a>(app: &'a App, adding: bool) -> PaneView<'a> {
    let view = app.day();
    let counts = view.counts;
    let mut sections = Vec::new();

    if !view.focus.is_empty() {
        sections.push(Section {
            label: "Focus",
            count: None,
            content: Content::Tasks(&view.focus, Kind::Open),
            add: None,
        });
    }
    // The add line belongs to the plan, so the group is drawn whenever
    // the day has anything on it at all.
    if counts.planned > 0 || adding {
        sections.push(Section {
            label: "Plan",
            count: None,
            content: Content::Tasks(&view.plan, Kind::Open),
            add: Some("add a task"),
        });
    }
    if !view.done.is_empty() {
        sections.push(Section {
            label: "Done",
            count: Some(view.done.len()),
            content: Content::Tasks(&view.done, Kind::Done),
            add: None,
        });
    }
    if !view.moved.is_empty() {
        sections.push(Section {
            label: "Moved",
            count: Some(view.moved.len()),
            content: Content::Tasks(&view.moved, Kind::Moved),
            add: None,
        });
    }

    let right = if counts.planned == 0 {
        "nothing planned".to_owned()
    } else {
        let mut parts = vec![format!("{} open", counts.open)];
        if counts.done > 0 {
            parts.push(format!("{} done", counts.done));
        }
        if counts.moved > 0 {
            parts.push(format!("{} moved", counts.moved));
        }
        parts.join(" · ")
    };

    PaneView {
        title: "Today",
        sub: day_label(view.day),
        right,
        sections,
        empty: [
            "Nothing planned.",
            "a add a task · l then t pull from the backlog",
        ],
    }
}

fn backlog_pane<'a>(app: &'a App, adding: bool) -> PaneView<'a> {
    let view = app.backlog();
    let mut sections = Vec::new();

    if view.open > 0 || adding {
        sections.push(Section {
            label: "",
            count: None,
            content: Content::Tasks(&view.ordinary, Kind::Open),
            add: Some("add to backlog"),
        });
    }
    if !view.waiting.is_empty() {
        sections.push(Section {
            label: "Waiting",
            count: Some(view.waiting_count),
            content: Content::Tasks(&view.waiting, Kind::Waiting),
            add: None,
        });
    }

    let right = if view.waiting_count > 0 {
        format!("{} · {} waiting", view.open, view.waiting_count)
    } else {
        view.open.to_string()
    };

    PaneView {
        title: "Backlog",
        sub: String::new(),
        right,
        sections,
        empty: ["Backlog is empty.", "a add · b on a day task sends it here"],
    }
}

/// The notes list, and under it the row that makes another one. They are
/// two groups rather than one so that a blank row separates them, which is
/// what keeps the list a list.
fn notes_pane(app: &App) -> PaneView<'_> {
    let view = app.notes();
    PaneView {
        title: "Notes",
        sub: String::new(),
        // The count is in the status line; the header names the key that
        // fills the list instead (DESIGN.md section 9).
        right: "a new".to_owned(),
        // The new-note row is the whole empty state (DESIGN.md section 10).
        sections: vec![
            Section {
                label: "",
                count: None,
                content: Content::Notes(&view.rows),
                add: None,
            },
            Section {
                label: "",
                count: None,
                content: Content::Notes(&[]),
                add: Some("new note"),
            },
        ],
        empty: ["", ""],
    }
}

/// A pane: its header, then its groups, then whatever is left blank.
/// One drawn line of a pane. A pane is laid out as lines first, so that
/// one longer than the window can be scrolled a line at a time rather
/// than a group at a time.
enum Line<'a> {
    Blank,
    Rule(&'static str, Option<usize>),
    Task(&'a domain::Row, Kind),
    Note(&'a NoteRow),
    Add(&'static str),
}

fn lines_of<'a>(view: &'a PaneView<'a>) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    for (at, section) in view.sections.iter().enumerate() {
        // A blank line between groups, and none before the first.
        if at > 0 {
            lines.push(Line::Blank);
        }
        if !section.label.is_empty() {
            lines.push(Line::Rule(section.label, section.count));
        }
        match section.content {
            Content::Tasks(rows, kind) => {
                lines.extend(rows.iter().map(|row| Line::Task(row, kind)));
            }
            Content::Notes(rows) => lines.extend(rows.iter().map(Line::Note)),
        }
        if let Some(add) = section.add {
            lines.push(Line::Add(add));
        }
    }
    lines
}

/// The first line drawn, which is as far down as it has to be for the
/// cursor to be on screen and no further. A pane has no scroll position
/// of its own: the cursor is what it follows (DESIGN.md section 4).
fn scroll_to(lines: usize, anchor: Option<usize>, height: usize) -> usize {
    let last = lines.saturating_sub(height);
    let anchor = anchor.unwrap_or(0);
    (anchor + 1).saturating_sub(height).min(last)
}

/// A pane: its header, then its lines from wherever the cursor has pushed
/// them, then whatever is left blank.
fn pane(
    canvas: &mut Canvas,
    app: &App,
    list: List,
    column: Column,
    rows: &Rows,
    focused: bool,
    layout: &mut Layout,
) {
    let Column { x, width, .. } = column;
    let writing = app.editor().filter(|editor| editor.list == list);
    let adding = writing.is_some_and(|editor| editor.field == Field::Adding);
    let view = match list {
        List::Day => day_pane(app, adding),
        List::Backlog => backlog_pane(app, adding),
        List::Notes => notes_pane(app),
    };

    // A narrow window puts the tab row where the pane headers would be.
    if !layout.narrow {
        header(canvas, x, width, rows.headers, &view, focused);
    }
    let height = column.height();
    layout.lists.push(ListArea {
        list,
        area: Cells {
            x,
            y: column.top,
            width,
            height,
        },
    });

    if view.sections.is_empty() {
        empty_state(canvas, column, view.empty);
        return;
    }

    let narrow = layout.narrow;
    let on = if focused { app.cursor(list) } else { None };
    let today = app.today();
    let lines = lines_of(&view);

    // What has to stay on screen: the field being typed into, or the
    // cursor row.
    let anchor = lines.iter().position(|line| match line {
        Line::Add(_) => adding,
        Line::Task(row, _) => !adding && on == Some(row.task),
        Line::Note(row) => !adding && on == Some(row.note),
        _ => false,
    });
    let first = scroll_to(lines.len(), anchor, height as usize);

    for (at, line) in lines.iter().skip(first).take(height as usize).enumerate() {
        let y = column.top + at as u16;
        let id = match line {
            Line::Blank => continue,
            Line::Rule(label, count) => {
                group_rule(canvas, x, width, y, label, *count);
                continue;
            }
            Line::Add(label) => {
                match writing.filter(|editor| editor.field == Field::Adding) {
                    Some(editor) => add_field(canvas, x, width, y, editor),
                    None => {
                        canvas.put(x + 1, y, &format!(" +  {label}"), dim());
                        canvas.rput(x + width - 1, y, "a", accent());
                    }
                }
                continue;
            }
            Line::Note(row) => {
                note_row(canvas, x, width, y, row, today);
                row.note
            }
            Line::Task(row, kind) => {
                let renaming = writing
                    .filter(|editor| editor.task == Some(row.task))
                    .filter(|editor| editor.field == Field::Renaming);
                match renaming {
                    Some(editor) => {
                        title_field(canvas, x, width, y, mark_of(row, *kind).0, editor);
                        // A row being typed into is not also a cursor row.
                        layout.rows.push(row_area(list, row.task, column, y));
                        continue;
                    }
                    None => task_row(
                        canvas,
                        column,
                        y,
                        row,
                        Look {
                            kind: *kind,
                            today,
                            narrow,
                            moving: app.moving() == Some(row.task),
                        },
                    ),
                }
                row.task
            }
        };
        if on == Some(id) {
            canvas.restyle(x, y, width, cursor());
        }
        layout.rows.push(row_area(list, id, column, y));
    }
}

fn row_area(list: List, id: i64, column: Column, y: u16) -> RowArea {
    RowArea {
        list,
        id,
        area: Cells {
            x: column.x,
            y,
            width: column.width,
            height: 1,
        },
    }
}

/// What an empty list is for, and the one or two keys that fill it
/// (DESIGN.md section 10). No illustration, no encouragement.
fn empty_state(canvas: &mut Canvas, column: Column, empty: [&str; 2]) {
    let middle = |text: &str| column.x + column.width.saturating_sub(count(text)) / 2;
    canvas.put(middle(empty[0]), column.top + 2, empty[0], dim());
    canvas.put(middle(empty[1]), column.top + 3, empty[1], dim());
}

/// `Today Fri 5 Sep                    6 open · 2 done · 1 moved`
fn header(canvas: &mut Canvas, x: u16, width: u16, y: u16, view: &PaneView, focused: bool) {
    let title = if focused { accent() } else { bold() };
    canvas.put(x + 1, y, view.title, title);
    if !view.sub.is_empty() {
        canvas.put(x + 2 + count(view.title), y, &view.sub, dim());
    }
    if !view.right.is_empty() {
        canvas.rput(x + width - 1, y, &view.right, dim());
    }
}

/// `FOCUS ──────────` and `DONE 2 ────────`.
fn group_rule(canvas: &mut Canvas, x: u16, width: u16, y: u16, label: &str, n: Option<usize>) {
    let text = match n {
        Some(n) => format!("{} {n}", label.to_uppercase()),
        None => label.to_uppercase(),
    };
    rule(canvas, x, width, y, &text);
}

/// A named line across a pane.
fn rule(canvas: &mut Canvas, x: u16, width: u16, y: u16, text: &str) {
    canvas.put(x + 1, y, text, dim());
    canvas.hline(
        x + 2 + count(text),
        y,
        width.saturating_sub(3 + count(text)),
        dim(),
    );
}

/// The box at the left of a row and the weight the row is drawn at.
fn mark_of(row: &domain::Row, kind: Kind) -> (&'static str, Style, Style) {
    match kind {
        Kind::Open if row.focus => ("[ ]", bold(), bold()),
        Kind::Open => ("[ ]", plain(), plain()),
        Kind::Done => ("[x]", Style::new().fg(Color::Green), dim()),
        Kind::Waiting => ("[ ]", dim(), dim()),
        Kind::Moved => ("[→]", dim(), plain()),
    }
}

/// A bracketed mark at the right of a row: its colour is its meaning and
/// the words beside it carry that meaning without the colour.
struct Chip {
    text: String,
    short: &'static str,
    style: Style,
}

/// The chips of a row, in the order they are drawn from the left.
fn chips_of(row: &domain::Row, kind: Kind, today: Date) -> Vec<Chip> {
    let mut chips = Vec::new();
    if kind == Kind::Waiting || row.waiting {
        chips.push(Chip {
            text: "waiting".to_owned(),
            short: "w",
            style: Style::new().fg(Color::Magenta),
        });
    }
    if let Some(rule) = &row.repeat {
        chips.push(Chip {
            text: format!("↻ {}", rule_label(rule)),
            short: "↻",
            style: dim(),
        });
    }
    if let Some(due) = row.due {
        chips.push(Chip {
            text: format!("due {}", when(due.on, today)),
            short: "due",
            style: Style::new().fg(if due.overdue {
                Color::Red
            } else {
                Color::Yellow
            }),
        });
    }
    if let Some(remind) = row.remind {
        chips.push(Chip {
            text: format!("◷ {}", when(remind, today)),
            short: "◷",
            style: Style::new().fg(Color::Cyan),
        });
    }
    chips
}

/// The right-hand words that are not a chip: that the row is being
/// carried up or down, where a moved task went, that a task came in from
/// the backlog, that a closed one was focus.
fn meta_of(row: &domain::Row, kind: Kind, today: Date, moving: bool) -> String {
    if moving {
        return "moving ▲▼".to_owned();
    }
    if kind == Kind::Moved {
        return format!("to {}", place_label(row.place, today));
    }
    if row.was_focus {
        return "was focus".to_owned();
    }
    if row.from_backlog {
        return "←backlog".to_owned();
    }
    String::new()
}

/// `[ ] Book dentist                          [◷ today]`
fn task_row(canvas: &mut Canvas, column: Column, y: u16, row: &domain::Row, look: Look) {
    let Column { x, width, .. } = column;
    let Look {
        kind,
        today,
        narrow,
        moving,
    } = look;
    let (mark, mark_style, title_style) = mark_of(row, kind);
    canvas.put(x + 1, y, mark, mark_style);
    canvas.put(
        x + 5,
        y,
        clip(&row.title, width.saturating_sub(6)),
        title_style,
    );

    // The right of the row, filled from its edge inwards: the time it was
    // closed, then the chips, then whatever text is left.
    let mut edge = x + width - 1;
    let closed = closed_label(row, today);
    if !closed.is_empty() && !narrow {
        edge = canvas.rput(edge, y, &closed, dim()) - count(&closed) - 2;
    }
    for chip in chips_of(row, kind, today).iter().rev() {
        let text = if narrow { chip.short } else { &chip.text };
        edge = canvas.rput(edge, y, &format!("[{text}]"), chip.style) - count(text) - 4;
    }
    // A narrow pane drops the row's words but keeps a moved row's pointer,
    // which is the whole content of the row.
    let meta = meta_of(row, kind, today, moving);
    if !meta.is_empty() && (!narrow || kind == Kind::Moved) {
        canvas.rput(edge, y, &meta, dim());
    }
}

/// The time a task was closed, or the date when it was closed on a later
/// day than the one being shown (DOMAIN.md section 6).
fn closed_label(row: &domain::Row, today: Date) -> String {
    let Some(at) = &row.closed_at else {
        return String::new();
    };
    if row.closed_on_this_day {
        at.strftime("%H:%M").to_string()
    } else {
        format!("closed {}", when(at.date(), today))
    }
}

/// A title being typed in place of the row it belongs to.
fn title_field(canvas: &mut Canvas, x: u16, width: u16, y: u16, mark: &str, editor: &Editor) {
    canvas.put(x + 1, y, mark, dim());
    field_text(canvas, x + 5, y, width.saturating_sub(6), editor);
}

/// The `+ add` line while it is being typed into.
fn add_field(canvas: &mut Canvas, x: u16, width: u16, y: u16, editor: &Editor) {
    canvas.put(x + 1, y, " +  ", accent());
    field_text(canvas, x + 5, y, width.saturating_sub(6), editor);
}

/// The text of a field with its caret where the next character goes.
fn field_text(canvas: &mut Canvas, x: u16, y: u16, width: u16, editor: &Editor) {
    let typed: String = editor.text.chars().take(editor.caret).collect();
    let rest: String = editor.text.chars().skip(editor.caret).collect();
    let at = canvas.put(x, y, clip(&typed, width), plain());
    let at = canvas.put(at, y, CARET, bold());
    canvas.put(at, y, clip(&rest, width.saturating_sub(at - x)), plain());
}

/// ` ▪ Mention to Anna: CI runner b                        yesterday`
fn note_row(canvas: &mut Canvas, x: u16, width: u16, y: u16, row: &NoteRow, today: Date) {
    canvas.put(x + 1, y, " ▪ ", dim());
    canvas.put(
        x + 4,
        y,
        clip(&row.first_line, width.saturating_sub(16)),
        plain(),
    );
    canvas.rput(x + width - 1, y, &age(row.created_at.date(), today), dim());
}

/// How long ago a note was made, in the few words the list has room for.
/// Past a couple of months the words stop being shorter than the date.
fn age(made: Date, today: Date) -> String {
    let days = made
        .until(today)
        .map_or(0, |span| i64::from(span.get_days()));
    match days {
        ..=0 => "today".to_owned(),
        1 => "yesterday".to_owned(),
        2..=6 => format!("{days} days"),
        7..=13 => "last week".to_owned(),
        14..=55 => format!("{} weeks", days / 7),
        _ => made.strftime("%-d %b").to_string(),
    }
}

/// The open note: the day it was made, and the body as a plain text area
/// with a caret in it. Nothing else is on it (DESIGN.md section 9).
///
/// `header` is the row the pane's header goes on; a narrow window has none
/// and puts the same words on the rule above the note instead.
fn open_note(
    canvas: &mut Canvas,
    app: &App,
    column: Column,
    header_row: Option<u16>,
    focused: bool,
) {
    let Column { x, width, .. } = column;
    let open = app.cursor(List::Notes);
    let made = app
        .notes()
        .rows
        .iter()
        .find(|row| Some(row.note) == open)
        .map(|row| row.created_at.strftime("%a %-d %b %H:%M").to_string());

    let view = PaneView {
        title: "Note",
        sub: made.clone().unwrap_or_default(),
        // The way out, on the pane the way out is from.
        right: if focused && made.is_some() {
            "esc back".to_owned()
        } else {
            String::new()
        },
        sections: Vec::new(),
        empty: ["No notes yet.", "a writes one"],
    };
    match header_row {
        Some(y) => header(canvas, x, width, y, &view, focused),
        None => stacked_rule(canvas, column, &view),
    }

    let Some(open) = open else {
        empty_state(canvas, column, view.empty);
        return;
    };
    // What is being typed, or the note as it was last saved.
    let draft = app.draft().filter(|draft| draft.note == open);
    let body = match draft {
        Some(draft) => draft.text.clone(),
        None => app
            .model()
            .note(open)
            .map_or_else(String::new, |note| note.body.clone()),
    };

    // The body has the pane from its second column to its last.
    let lines = wrapped(&body, width.saturating_sub(2));
    let caret = draft.map(|draft| caret_at(&lines, draft.caret));
    let height = column.height() as usize;
    let first = scroll_to(lines.len(), caret.map(|(row, _)| row), height);

    for (at, (text, _)) in lines.iter().skip(first).take(height).enumerate() {
        canvas.put(x + 2, column.top + at as u16, text, plain());
    }
    if let Some((row, glyph)) = caret
        && row >= first
        && row < first + height
    {
        canvas.put(
            x + 2 + glyph as u16,
            column.top + (row - first) as u16,
            CARET,
            bold(),
        );
    }
}

/// In one tab the note has no header of its own, so its name and day
/// become the rule between it and the list.
fn stacked_rule(canvas: &mut Canvas, column: Column, view: &PaneView) {
    let text = format!("{} {}", view.title, view.sub);
    rule(canvas, column.x, column.width, column.top - 1, text.trim());
}

/// A note body as the lines it is drawn on: every line of the body broken
/// at the width of the pane, on a space where there is one, each with the
/// character of the body it starts at so that the caret can be found on
/// it again.
fn wrapped(body: &str, width: u16) -> Vec<(String, usize)> {
    let width = width.max(1) as usize;
    let mut lines = Vec::new();
    let mut at = 0;
    for line in body.split('\n') {
        let glyphs: Vec<char> = line.chars().collect();
        let mut from = 0;
        loop {
            if glyphs.len() - from <= width {
                lines.push((glyphs[from..].iter().collect(), at + from));
                break;
            }
            // After the last space that fits, or through a word longer
            // than the pane.
            let take = glyphs[from..from + width]
                .iter()
                .rposition(|glyph| *glyph == ' ')
                .map_or(width, |space| space + 1);
            lines.push((glyphs[from..from + take].iter().collect(), at + from));
            from += take;
        }
        // The newline the split took off.
        at += glyphs.len() + 1;
    }
    lines
}

/// Which drawn line a caret is on, and how far along it.
fn caret_at(lines: &[(String, usize)], caret: usize) -> (usize, usize) {
    let row = lines
        .iter()
        .rposition(|(_, start)| *start <= caret)
        .unwrap_or_default();
    (row, caret - lines.get(row).map_or(0, |(_, start)| *start))
}

fn clip(text: &str, width: u16) -> &str {
    match text.char_indices().nth(width as usize) {
        Some((at, _)) => &text[..at],
        None => text,
    }
}
