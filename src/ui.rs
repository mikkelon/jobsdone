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
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Editor, Layout, List, ListArea, Page, Rect as Cells, RowArea, RowId};
use crate::domain::{
    self, DateOrder, DayListRow, MonthDay, NoteRow, Place, Rule, ScheduleRow, Stretch, Weekday,
    day_label, short_label, stamp_label,
};
use crate::input::{self, Field, NotesPane, Pane, Shown, Side};

mod popup;
mod review;
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
    ///
    /// The unit is the grapheme cluster, which is what a person calls a
    /// character and what a terminal draws in one place: an `e` and the
    /// combining accent after it go into one cell together, and a family
    /// emoji made of four people and three joiners goes into two. A
    /// cluster is as many cells wide as the terminal will give it, so a
    /// double-width one takes the cell beside it too.
    fn put(&mut self, x: u16, y: u16, text: &str, style: Style) -> u16 {
        let mut at = x;
        for symbol in text.graphemes(true) {
            let width = cells(symbol);
            if width == 0 {
                continue;
            }
            if y >= self.height() || at.saturating_add(width) > self.width() {
                break;
            }
            let position = Position::new(self.area.x + at, self.area.y + y);
            if let Some(cell) = self.buffer.cell_mut(position) {
                cell.reset();
                cell.set_symbol(symbol);
                cell.set_style(style);
            }
            // The cells a wide character covers are cleared and never
            // written to, which is what ratatui's diff reads as "the one
            // beside it owns this".
            for beside in at + 1..at + width {
                let position = Position::new(self.area.x + beside, self.area.y + y);
                if let Some(cell) = self.buffer.cell_mut(position) {
                    cell.reset();
                }
            }
            at = at.saturating_add(width);
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

/// A count and the noun it counts, in the number the count puts it in.
/// The plural is written out, because English does not always make one by
/// adding an s.
fn counted(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// How many cells a string takes, which is not how many characters it
/// has: a CJK character or an emoji takes two, a combining mark none.
fn count(text: &str) -> u16 {
    UnicodeWidthStr::width(text) as u16
}

/// The same for one grapheme cluster, which is how a string is walked a
/// cell at a time. A cluster whose only characters are combining marks
/// has no cell of its own; every other one has one or two.
fn cells(glyph: &str) -> u16 {
    UnicodeWidthStr::width(glyph) as u16
}

// ---- dates, rules and places, as words -------------------------------

/// A date beside today: the word where there is one, the date otherwise.
fn when(date: Date, today: Date, dates: DateOrder) -> String {
    if date == today {
        "today".to_owned()
    } else {
        short_label(date, dates)
    }
}

/// How far off a day is, which is what the status line says about the
/// day the pane has been stepped to.
fn ago(day: Date, today: Date) -> String {
    let days = today
        .until(day)
        .map_or(0, |span| i64::from(span.get_days()));
    match days {
        -1 => "yesterday".to_owned(),
        1 => "tomorrow".to_owned(),
        ..=0 => format!("{} days ago", -days),
        _ => format!("in {days} days"),
    }
}

/// Where a task is now, which is what a moved row points at.
fn place_label(place: Place, today: Date, dates: DateOrder) -> String {
    match place {
        Place::Backlog => "backlog".to_owned(),
        Place::Day(day) if day == today => "today".to_owned(),
        Place::Day(day) => day_label(day, dates),
    }
}

/// `Mo`, the two letters a calendar column has room for.
fn short_weekday(day: Weekday) -> &'static str {
    &weekday_name(day)[..2]
}

/// `1st`, or `last`.
fn month_day_label(day: MonthDay) -> String {
    match day {
        MonthDay::Day(day) => format!("{day}{}", ordinal(day)),
        MonthDay::Last => "last".to_owned(),
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
        Rule::EveryNWeeks { n, .. } => format!("every {}", counted(*n as usize, "week", "weeks")),
    }
}

/// A repeat rule with the room a list row has, which is enough for a
/// weekday's whole name when the rule names only one.
fn schedule_label(rule: &Rule) -> String {
    match rule {
        Rule::Workdays => "every work day".to_owned(),
        Rule::Weekly { weekdays } if weekdays.len() == 1 => {
            format!("every {}", weekday_word(weekdays[0]))
        }
        other => rule_label(other),
    }
}

fn weekday_word(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "Monday",
        Weekday::Tue => "Tuesday",
        Weekday::Wed => "Wednesday",
        Weekday::Thu => "Thursday",
        Weekday::Fri => "Friday",
        Weekday::Sat => "Saturday",
        Weekday::Sun => "Sunday",
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
    // The review is a mode over the page and takes the whole window
    // (DESIGN.md section 5).
    if app.review().is_some() {
        review::draw(&mut canvas, app, &rows, &mut layout);
    } else if narrow {
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
    if let Some(under_way) = app.review() {
        review::status(canvas, under_way, y, narrow);
        return;
    }
    let review = app.review_count();
    // Red only while there is something on the pile: a zero is a count,
    // not an alert.
    let pile = if review > 0 {
        Style::new().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        dim()
    };
    let browsing = app.shown() != Shown::Today;
    let day = if browsing {
        (day_label(app.showing(), app.dates()), bold())
    } else {
        (
            format!("Today · {}", day_label(app.today(), app.dates())),
            bold(),
        )
    };
    let count = app.notes().count;
    let notes = counted(count, "note", "notes");
    // A day that is not today says how far off it is and how to come
    // back, which leaves the right end no room for its own words.
    let short = vec![
        (format!("● {review} in review"), pile),
        quiet(&format!("{notes} n")),
        quiet("/"),
        quiet(":"),
        quiet("?"),
    ];

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
        (Page::Home, false) if browsing => (
            vec![
                quiet("‹"),
                day,
                quiet("›"),
                quiet(&ago(app.showing(), app.today())),
                quiet(". back to today"),
            ],
            short,
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

    let edge = canvas.width() - 1;
    if let Some(message) = app.message() {
        // The offer of `u` is not made while a field has the keyboard:
        // `u` types there, so the bar would be offering a key the line
        // would swallow (DESIGN.md section 8).
        let offer = message.undo && !context.text_field();
        let room = edge.saturating_sub(x + if offer { 8 } else { 0 });
        x = canvas.put(x, y, clip(&message.text, room), plain()) + 2;
        if offer {
            x = canvas.put(x, y, "u", accent());
            x = canvas.put(x + 2, y, "undo", dim()) + 2;
        }
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

    // After a message, the keys that still fit follow it; the ones that
    // do not are left out rather than cut in half.
    for (shown, name) in left {
        if x + count(shown) + 1 + count(name) > edge {
            break;
        }
        x = canvas.put(x, y, shown, accent());
        x = canvas.put(x + 1, y, name, dim()) + 2;
    }

    let mut edge = edge;
    for (shown, name) in right.into_iter().rev() {
        // A row that would reach what the left end has already drawn is
        // left out rather than written over it.
        if edge.saturating_sub(count(name) + count(shown) + 1) < x {
            break;
        }
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
    // Stepped to another day, the first two tabs are that day and the
    // list of days, because that is what the two panes hold.
    let browsing = app.shown() != Shown::Today;
    let tabs: [String; 3] = if browsing {
        [
            day_label(app.showing(), app.dates()).to_uppercase(),
            "DAYS".to_owned(),
            TABS[2].to_owned(),
        ]
    } else {
        TABS.map(str::to_owned)
    };
    let counts = if browsing {
        [
            app.day().counts.planned,
            app.days().days().count(),
            app.notes().count,
        ]
    } else {
        [app.day().counts.open, app.backlog().open, app.notes().count]
    };
    let mut x = 1;
    for (at, (name, count)) in tabs.iter().zip(counts).enumerate() {
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
            // The pane beside the day is the backlog only while the day
            // is today (DESIGN.md section 6).
            let beside = if app.browsing() {
                List::Days
            } else {
                List::Backlog
            };
            pane(canvas, app, beside, right, rows, !on_left, layout);
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
struct Look<'a> {
    kind: Kind,
    today: Date,
    /// Which way round the dates on the row are written.
    dates: DateOrder,
    narrow: bool,
    moving: bool,
    /// The words at the right of the row when the caller knows them and
    /// the drawing cannot work them out, which is the review saying what
    /// it did. They stand in for everything else the right holds.
    note: Option<&'a str>,
    /// Whether the row is to be read whole: a title longer than its line
    /// then goes on under it rather than ending in an ellipsis, which is
    /// how the cursor row is drawn (DESIGN.md section 2).
    whole: bool,
}

/// How a row is drawn, which is what its group says rather than anything
/// the drawing works out for itself.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Open,
    Done,
    Waiting,
    Moved,
    /// A row the review has answered: checked and dim, whatever the
    /// answer was, with what it was in the words beside it.
    Handled,
}

/// The rows of one group of a pane.
enum Content<'a> {
    Tasks(&'a [domain::Row], Kind),
    Schedules(&'a [ScheduleRow]),
    Days(&'a [DayListRow]),
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
    title: String,
    sub: String,
    right: String,
    sections: Vec<Section<'a>>,
    /// A last dim line under the whole pane, which only the day list has:
    /// what it does not show.
    foot: Option<&'static str>,
    /// What the list is for and the keys that fill it (DESIGN.md section
    /// 10).
    empty: [&'static str; 2],
}

/// Today, or whichever day the day pane has been stepped to. A day is
/// drawn the same way whichever it is (DESIGN.md section 6); only the
/// header says which, and only the words around the add line change.
fn day_pane<'a>(app: &'a App, adding: bool) -> PaneView<'a> {
    let view = app.day();
    let counts = view.counts;
    let shown = app.shown();
    let mut sections = Vec::new();

    if !view.focus.is_empty() {
        sections.push(Section {
            label: "Focus",
            count: None,
            content: Content::Tasks(&view.focus, Kind::Open),
            add: None,
        });
    }
    // The add line is the pane's rather than the group's, so it is drawn
    // whenever the day has anything on it at all, and the label goes when
    // there is nothing under it but the add line (DESIGN.md section 6).
    if counts.planned > 0 || adding {
        sections.push(Section {
            label: if view.plan.is_empty() { "" } else { "Plan" },
            count: None,
            content: Content::Tasks(&view.plan, Kind::Open),
            add: Some(match shown {
                Shown::Today => "add a task",
                _ => "add a task to this day",
            }),
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

    // Today counts what it still holds; a day that is over is a record,
    // so it counts everything that was planned on it (DOMAIN.md section
    // 6).
    let right = match (counts.planned, shown) {
        (0, Shown::Past) => "nothing was planned".to_owned(),
        (0, _) => "nothing planned".to_owned(),
        (_, Shown::Today) => {
            let mut parts = vec![format!("{} open", counts.open)];
            if counts.done > 0 {
                parts.push(format!("{} done", counts.done));
            }
            if counts.moved > 0 {
                parts.push(format!("{} moved", counts.moved));
            }
            parts.join(" · ")
        }
        // The whole record of the day, which is what a day that is not
        // today is. A count of nothing is left out, the way today's are,
        // so the header still has room for its date beside them.
        _ => [
            (counts.planned, "planned"),
            (counts.done, "done"),
            (counts.open, "open"),
            (counts.moved, "moved"),
        ]
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, name)| format!("{n} {name}"))
        .collect::<Vec<_>>()
        .join(" · "),
    };

    let (title, sub) = match shown {
        Shown::Today => ("Today".to_owned(), day_label(view.day, app.dates())),
        Shown::Past => (day_label(view.day, app.dates()), "past day".to_owned()),
        Shown::Future => (day_label(view.day, app.dates()), "future day".to_owned()),
    };

    PaneView {
        title,
        sub,
        right,
        sections,
        foot: None,
        empty: match shown {
            Shown::Today => [
                "Nothing planned.",
                "a add a task · l then t pull from the backlog",
            ],
            Shown::Past => [
                "Nothing was planned on this day.",
                "[ keeps stepping back · g pick a date",
            ],
            Shown::Future => [
                "Nothing planned for this day.",
                "] keeps stepping on · g pick a date",
            ],
        },
    }
}

/// The days that have something planned on them, which is what the pane
/// beside the day becomes as soon as the day is not today (DESIGN.md
/// section 6).
fn days_pane(app: &App) -> PaneView<'_> {
    let view = app.days();
    let sections: Vec<Section<'_>> = view
        .stretches
        .iter()
        .map(|stretch| Section {
            label: stretch_label(stretch.stretch),
            count: None,
            content: Content::Days(&stretch.days),
            add: None,
        })
        .collect();

    PaneView {
        title: "Days".to_owned(),
        sub: String::new(),
        right: "g go to date".to_owned(),
        sections,
        foot: Some("days with nothing planned are skipped"),
        empty: [
            "Nothing has been planned on any day yet.",
            ". back to today",
        ],
    }
}

/// What a stretch of the day list is called. Which days are in it is the
/// domain's (DOMAIN.md section 6); the name is the screen's.
fn stretch_label(stretch: Stretch) -> &'static str {
    match stretch {
        Stretch::Later => "Later",
        Stretch::ThisWeek => "This week",
        Stretch::LastWeek => "Last week",
        Stretch::Earlier => "Earlier",
    }
}

fn backlog_pane<'a>(app: &'a App, adding: bool) -> PaneView<'a> {
    let view = app.backlog();
    let mut sections = Vec::new();

    // The add line belongs to the first group, so it is drawn whenever
    // the pane has anything at all: a backlog holding only schedules is
    // not the empty state, but it still needs the key that fills it.
    if view.open > 0 || adding || !view.schedules.is_empty() {
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

    // The schedules under the two groups are a view of the rules, not of
    // tasks: somewhere to see and edit them without a fourth place for a
    // task to live (DOMAIN.md section 7).
    if !view.schedules.is_empty() {
        sections.push(Section {
            label: "Repeating",
            count: Some(view.schedules.len()),
            content: Content::Schedules(&view.schedules),
            add: None,
        });
    }

    let right = if view.waiting_count > 0 {
        format!("{} · {} waiting", view.open, view.waiting_count)
    } else {
        view.open.to_string()
    };

    PaneView {
        title: "Backlog".to_owned(),
        sub: String::new(),
        right,
        sections,
        foot: None,
        empty: ["Backlog is empty.", "a add · b on a day task sends it here"],
    }
}

/// The notes list, and under it the row that makes another one. They are
/// two groups rather than one so that a blank row separates them, which is
/// what keeps the list a list.
fn notes_pane(app: &App) -> PaneView<'_> {
    let view = app.notes();
    PaneView {
        title: "Notes".to_owned(),
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
        foot: None,
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
    Schedule(&'a ScheduleRow),
    Day(&'a DayListRow),
    Note(&'a NoteRow),
    Add(&'static str),
    /// The pane's last word about itself, under everything else.
    Foot(&'static str),
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
            Content::Schedules(rows) => lines.extend(rows.iter().map(Line::Schedule)),
            Content::Days(rows) => lines.extend(rows.iter().map(Line::Day)),
            Content::Notes(rows) => lines.extend(rows.iter().map(Line::Note)),
        }
        if let Some(add) = section.add {
            lines.push(Line::Add(add));
        }
    }
    if let Some(foot) = view.foot {
        lines.push(Line::Foot(foot));
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
        List::Days => days_pane(app),
        List::Notes => notes_pane(app),
        // The review draws its own rows and is never a pane beside
        // another one.
        List::Review => return,
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
        Line::Task(row, _) => !adding && on == Some(RowId::Task(row.task)),
        Line::Note(row) => !adding && on == Some(RowId::Note(row.note)),
        Line::Schedule(row) => !adding && on == Some(RowId::Schedule(row.schedule)),
        Line::Day(row) => !adding && on == Some(RowId::Day(row.day)),
        _ => false,
    });
    // The cursor row is read whole: a title longer than its line goes on
    // under it, and the rows below move down by as much (DESIGN.md
    // section 2). How much is known before anything scrolls.
    let tail_of_row = |row: &domain::Row, kind: Kind| -> Vec<String> {
        let look = Look {
            kind,
            today,
            dates: app.dates(),
            narrow,
            moving: app.moving() == Some(row.task),
            note: None,
            whole: true,
        };
        let (_, room) = right_side(column, row, look);
        tail_of(&row.title, room, width.saturating_sub(6))
    };
    let extra = match anchor.map(|at| &lines[at]) {
        Some(Line::Task(row, kind)) if writing.is_none() => tail_of_row(row, *kind).len(),
        _ => 0,
    };

    // The foot is the pane's last word about itself, so it follows the
    // last row on screen rather than being scrolled off under it. The
    // scroll counts the cursor row's extra lines as lines after it.
    let bottom = match (anchor, view.foot) {
        (Some(at), Some(_)) if at + 2 == lines.len() => Some(at + extra + 1),
        (anchor, _) => anchor.map(|at| at + extra),
    };
    let first = scroll_to(lines.len() + extra, bottom, height as usize);
    let first = anchor.map_or(first, |at| first.min(at));

    let mut y = column.top;
    for line in lines.iter().skip(first) {
        if y > column.bottom {
            break;
        }
        let mut rows = 1;
        let id = match line {
            Line::Blank => None,
            Line::Rule(label, count) => {
                group_rule(canvas, x, width, y, label, *count);
                None
            }
            Line::Add(label) => {
                match writing.filter(|editor| editor.field == Field::Adding) {
                    Some(editor) => add_field(canvas, x, width, y, editor),
                    None => {
                        canvas.put(x + 1, y, &format!(" +  {label}"), dim());
                        canvas.rput(x + width - 1, y, "a", accent());
                    }
                }
                None
            }
            Line::Foot(text) => {
                canvas.put(x + 5, y, "…", dim());
                canvas.rput(x + width - 1, y, text, dim());
                None
            }
            Line::Note(row) => {
                note_row(canvas, x, width, y, row, app);
                Some(RowId::Note(row.note))
            }
            Line::Day(row) => {
                day_row(canvas, x, width, y, row, today, app.dates());
                Some(RowId::Day(row.day))
            }
            Line::Schedule(row) => {
                schedule_row(canvas, x, width, y, row);
                Some(RowId::Schedule(row.schedule))
            }
            Line::Task(row, kind) => {
                let renaming = writing
                    .filter(|editor| editor.task == Some(row.task))
                    .filter(|editor| editor.field == Field::Renaming);
                match renaming {
                    Some(editor) => {
                        title_field(canvas, x, width, y, mark_of(row, *kind).0, editor);
                        // A row being typed into is not also a cursor row.
                        layout
                            .rows
                            .push(row_area(list, RowId::Task(row.task), column, y, 1));
                        None
                    }
                    None => {
                        let id = RowId::Task(row.task);
                        let look = Look {
                            kind: *kind,
                            today,
                            dates: app.dates(),
                            narrow,
                            moving: app.moving() == Some(row.task),
                            note: None,
                            whole: on == Some(id),
                        };
                        if let Some(rest) = task_row(canvas, column, y, row, look) {
                            let style = mark_of(row, *kind).2;
                            for (line, _) in wrapped(&rest, width.saturating_sub(6)) {
                                if y + rows > column.bottom {
                                    break;
                                }
                                canvas.put(x + 5, y + rows, line.trim_end(), style);
                                rows += 1;
                            }
                        }
                        Some(id)
                    }
                }
            }
        };
        if let Some(id) = id {
            if on == Some(id) {
                for below in 0..rows {
                    canvas.restyle(x, y + below, width, cursor());
                }
            }
            layout.rows.push(row_area(list, id, column, y, rows));
        }
        y += rows;
    }
}

fn row_area(list: List, id: RowId, column: Column, y: u16, height: u16) -> RowArea {
    RowArea {
        list,
        id,
        area: Cells {
            x: column.x,
            y,
            width: column.width,
            height,
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

/// The blank cells a header keeps between what it names on the left and
/// what it counts on the right, so the two are never read as one word.
const HEADER_GAP: u16 = 2;

/// `Today Fri 5 Sep                    6 open · 2 done · 1 moved`
fn header(canvas: &mut Canvas, x: u16, width: u16, y: u16, view: &PaneView, focused: bool) {
    let title = if focused { accent() } else { bold() };
    let mut left = canvas.put(x + 1, y, &view.title, title);
    if !view.sub.is_empty() {
        left = canvas.put(x + 2 + count(&view.title), y, &view.sub, dim());
    }
    if !view.right.is_empty() {
        // The right end takes what is left of the line after the words on
        // the left and the gap, so a long date and a long count run out
        // of room rather than into each other.
        let edge = x + width.saturating_sub(1);
        let room = edge.saturating_sub(left + HEADER_GAP);
        canvas.rput(edge, y, clip(&view.right, room), dim());
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
        Kind::Handled => ("[x]", dim(), dim()),
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
fn chips_of(row: &domain::Row, look: Look) -> Vec<Chip> {
    let Look {
        kind, today, dates, ..
    } = look;
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
            text: format!("due {}{}", when(due.on, today, dates), how_late(due, today)),
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
            text: format!("◷ {}", when(remind, today, dates)),
            short: "◷",
            style: Style::new().fg(Color::Cyan),
        });
    }
    // A task still open on a day that has passed. Red, because the cost
    // of leaving it there is the point (DESIGN.md section 3). Not in the
    // review, whose whole list is the pile: a chip on every row of it
    // would say nothing.
    if row.on_the_pile && look.note.is_none() {
        chips.push(Chip {
            text: "on the pile".to_owned(),
            short: "pile",
            style: Style::new().fg(Color::Red),
        });
    }
    // The same task on a day the pile no longer reaches. Dim, not red:
    // the horizon is the person's own decision to stop being asked.
    if row.still_open && look.note.is_none() {
        chips.push(Chip {
            text: "still open".to_owned(),
            short: "open",
            style: dim(),
        });
    }
    chips
}

/// How long a due date has been past, which is the cost of leaving it
/// there and the reason the chip is red.
fn how_late(due: domain::DueChip, today: Date) -> String {
    if !due.overdue {
        return String::new();
    }
    let days = due
        .on
        .until(today)
        .map_or(0, |span| i64::from(span.get_days()));
    format!(" · {} over", counted(days as usize, "day", "days"))
}

/// The right-hand words that are not a chip: that the row is being
/// carried up or down, where a moved task went, that a task came in from
/// the backlog, that a closed one was focus.
fn meta_of(row: &domain::Row, kind: Kind, today: Date, dates: DateOrder, moving: bool) -> String {
    if moving {
        return "moving ▲▼".to_owned();
    }
    if kind == Kind::Moved {
        return format!("to {}", place_label(row.place, today, dates));
    }
    if row.was_focus {
        return "was focus".to_owned();
    }
    if row.from_backlog {
        return "←backlog".to_owned();
    }
    String::new()
}

/// The most of its title a row keeps, however full its right-hand side.
/// A row whose title has gone cannot be told from any other row, so the
/// words on the right give way first (DESIGN.md section 2).
const TITLE_LEAST: u16 = 8;

/// One of the things at the right of a row: its text, the blank cells to
/// its left, and how readily it goes when the line is too full.
struct Piece {
    text: String,
    style: Style,
    gap: u16,
    /// Dropped in descending order, so the chips go before the row's own
    /// words and the time it was closed goes last.
    drop: u8,
}

/// The pieces at the right of a row, shrunk to fit, and the cells the
/// title has to the left of them.
///
/// The right-hand side is measured before anything is drawn, and shrinks
/// to fit: first every chip to the mark a narrow pane would give it, then
/// pieces dropped from the left until the title has `TITLE_LEAST` cells.
/// Nothing is ever placed left of the title, whatever the row carries.
fn right_side(column: Column, row: &domain::Row, look: Look) -> (Vec<Piece>, u16) {
    let Column { x, width, .. } = column;
    let Look {
        kind,
        today,
        dates,
        narrow,
        moving,
        note,
        ..
    } = look;
    let title_x = x + 5;
    let edge = x + width.saturating_sub(1);
    // A moved row says where the task is now and nothing else: what
    // became of it there belongs to the day it is on. Nor does a row the
    // caller has given its own words, which are the whole of its right.
    let closed = if kind == Kind::Moved || note.is_some() {
        String::new()
    } else {
        closed_label(row, today, dates)
    };
    let meta = match note {
        Some(note) => note.to_owned(),
        None => meta_of(row, kind, today, dates, moving),
    };

    let pieces_of = |short: bool| {
        let mut pieces = Vec::new();
        // A narrow pane drops the row's words, except where they are the
        // content of the row: where a moved task went, and what the review
        // did to a row it has answered.
        if !meta.is_empty() && (!narrow || kind == Kind::Moved || note.is_some()) {
            pieces.push(Piece {
                text: meta.clone(),
                style: dim(),
                gap: 1,
                drop: 1,
            });
        }
        for chip in chips_of(row, look) {
            let text = if short { chip.short } else { &chip.text };
            pieces.push(Piece {
                text: format!("[{text}]"),
                style: chip.style,
                gap: 2,
                drop: 2,
            });
        }
        if !closed.is_empty() && !narrow {
            pieces.push(Piece {
                text: closed.clone(),
                style: dim(),
                gap: 2,
                drop: 0,
            });
        }
        pieces
    };
    let span = |pieces: &[Piece]| -> u16 {
        pieces
            .iter()
            .map(|piece| count(&piece.text) + piece.gap)
            .sum()
    };

    let room = edge.saturating_sub(title_x);
    let budget = room.saturating_sub(TITLE_LEAST.min(room));
    let mut pieces = pieces_of(narrow);
    if span(&pieces) > budget {
        pieces = pieces_of(true);
    }
    while span(&pieces) > budget {
        // The leftmost of the pieces that go first, so a row loses its
        // chips in the order it gained them.
        let Some(at) = (0..pieces.len()).rev().max_by_key(|at| pieces[*at].drop) else {
            break;
        };
        pieces.remove(at);
    }

    let taken = span(&pieces);
    (pieces, room.saturating_sub(taken))
}

/// `[ ] Book dentist                          [◷ today]`
///
/// The title has whatever the right-hand side leaves it. A title that
/// does not fit ends in an ellipsis, unless the row is to be read whole:
/// then its first line is drawn and the rest handed back to go on the
/// lines below (DESIGN.md section 2).
fn task_row(
    canvas: &mut Canvas,
    column: Column,
    y: u16,
    row: &domain::Row,
    look: Look,
) -> Option<String> {
    let Column { x, width, .. } = column;
    let (mark, mark_style, title_style) = mark_of(row, look.kind);
    canvas.put(x + 1, y, mark, mark_style);

    let title_x = x + 5;
    let edge = x + width.saturating_sub(1);
    let (pieces, room) = right_side(column, row, look);
    let mut right = edge;
    for piece in pieces.iter().rev() {
        right = canvas
            .rput(right, y, &piece.text, piece.style)
            .saturating_sub(count(&piece.text) + piece.gap);
    }

    match split_title(&row.title, room) {
        None => {
            canvas.put(title_x, y, &row.title, title_style);
            None
        }
        Some((first, rest)) if look.whole => {
            canvas.put(title_x, y, &first, title_style);
            Some(rest.to_owned())
        }
        Some(_) => {
            let at = canvas.put(
                title_x,
                y,
                clip(&row.title, room.saturating_sub(1)),
                title_style,
            );
            if room > 0 {
                canvas.put(at, y, "…", dim());
            }
            None
        }
    }
}

/// A title that does not fit its cells, as the first line of it that
/// does, broken on a space where there is one, and the rest.
fn split_title(title: &str, room: u16) -> Option<(String, &str)> {
    if count(title) <= room {
        return None;
    }
    let lines = wrapped(title, room);
    let first = lines
        .first()
        .map(|(line, _)| line.trim_end().to_owned())
        .unwrap_or_default();
    let rest = lines
        .get(1)
        .map_or("", |(_, start)| &title[glyph_at(title, *start)..]);
    Some((first, rest))
}

/// The lines under a row that show the rest of a title its own line had
/// no room for, each at the full width of the pane.
fn tail_of(title: &str, room: u16, width: u16) -> Vec<String> {
    match split_title(title, room) {
        Some((_, rest)) => wrapped(rest, width)
            .into_iter()
            .map(|(line, _)| line.trim_end().to_owned())
            .collect(),
        None => Vec::new(),
    }
}

/// The time a task was closed, or the date when it was closed on a later
/// day than the one being shown (DOMAIN.md section 6).
fn closed_label(row: &domain::Row, today: Date, dates: DateOrder) -> String {
    let Some(at) = &row.closed_at else {
        return String::new();
    };
    if row.closed_on_this_day {
        at.strftime("%H:%M").to_string()
    } else {
        format!("closed {}", when(at.date(), today, dates))
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
    caret_line(canvas, x, y, width, &editor.text, editor.caret);
}

/// A line being typed, in `width` cells, scrolled so that the caret is
/// always on it: what is before the caret gives up its beginning until
/// the caret fits, and what is after it is cut off at the end of the line
/// (DESIGN.md section 8).
fn caret_line(canvas: &mut Canvas, x: u16, y: u16, width: u16, text: &str, caret: usize) {
    if width == 0 {
        return;
    }
    let split = glyph_at(text, caret);
    let before = &text[..split];
    let after = &text[split..];

    // The caret has a cell of its own, so the text before it has one
    // fewer than the line.
    let mut over = (count(before) + 1).saturating_sub(width);
    let mut from = 0;
    for glyph in before.graphemes(true) {
        if over == 0 {
            break;
        }
        over = over.saturating_sub(cells(glyph));
        from += glyph.len();
    }

    let at = canvas.put(x, y, &before[from..], plain());
    let at = canvas.put(at, y, CARET, bold());
    canvas.put(at, y, clip(after, (x + width).saturating_sub(at)), plain());
}

/// ` ↻  Write standup notes                     every work day`
fn schedule_row(canvas: &mut Canvas, x: u16, width: u16, y: u16, row: &ScheduleRow) {
    let rule = schedule_label(&row.rule);
    canvas.put(x + 1, y, " ↻ ", dim());
    canvas.put(
        x + 5,
        y,
        clip(&row.title, width.saturating_sub(8 + count(&rule))),
        plain(),
    );
    canvas.rput(x + width - 1, y, &rule, dim());
}

/// `     Thu 4 Sep                               4 / 5 · 1 open`
fn day_row(
    canvas: &mut Canvas,
    x: u16,
    width: u16,
    y: u16,
    row: &DayListRow,
    today: Date,
    dates: DateOrder,
) {
    let mut name = day_label(row.day, dates);
    if row.day == today {
        name.push_str(" · today");
    }
    canvas.put(x + 5, y, &name, plain());

    let mut counts = format!("{} / {}", row.done, row.kept);
    if row.open > 0 {
        counts.push_str(&format!(" · {} open", row.open));
    }
    canvas.rput(x + width - 1, y, &counts, dim());
}

/// ` ▪ Mention to Anna: CI runner b                        yesterday`
fn note_row(canvas: &mut Canvas, x: u16, width: u16, y: u16, row: &NoteRow, app: &App) {
    canvas.put(x + 1, y, " ▪ ", dim());
    canvas.put(
        x + 4,
        y,
        clip(&row.first_line, width.saturating_sub(16)),
        plain(),
    );
    let made = app.settings().working_day(&row.created_at);
    canvas.rput(
        x + width - 1,
        y,
        &age(made, app.today(), app.dates()),
        dim(),
    );
}

/// How long ago a note was made, counted in working days so that one
/// written at one in the morning is still yesterday's (DOMAIN.md section
/// 2). Past a couple of months the words stop being shorter than the date.
fn age(made: Date, today: Date, dates: DateOrder) -> String {
    let days = made
        .until(today)
        .map_or(0, |span| i64::from(span.get_days()));
    match days {
        ..=0 => "today".to_owned(),
        1 => "yesterday".to_owned(),
        2..=6 => format!("{days} days"),
        7..=13 => "last week".to_owned(),
        14..=55 => format!("{} weeks", days / 7),
        _ => short_label(made, dates),
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
    let open = app.cursor(List::Notes).and_then(RowId::note);
    let made = app
        .notes()
        .rows
        .iter()
        .find(|row| Some(row.note) == open)
        .map(|row| stamp_label(&row.created_at, app.dates()));

    let view = PaneView {
        title: "Note".to_owned(),
        sub: made.clone().unwrap_or_default(),
        // The way out, on the pane the way out is from.
        right: if focused && made.is_some() {
            "esc back".to_owned()
        } else {
            String::new()
        },
        sections: Vec::new(),
        foot: None,
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
        let y = column.top + at as u16;
        // The caret is a cell of its own between two characters, the way
        // it is in a field, so the character it is in front of is still
        // drawn and a wide one is not cut in half.
        match caret.filter(|(row, _)| *row == first + at) {
            Some((_, glyph)) => {
                let split = glyph_at(text, glyph);
                let at = canvas.put(x + 2, y, &text[..split], plain());
                let at = canvas.put(at, y, CARET, bold());
                canvas.put(at, y, &text[split..], plain());
            }
            None => {
                canvas.put(x + 2, y, text, plain());
            }
        }
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
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut at = 0;
    for line in body.split('\n') {
        let glyphs: Vec<&str> = line.graphemes(true).collect();
        let mut from = 0;
        loop {
            // How many clusters of the rest the line has room for. A
            // cluster wider than the whole pane still takes a line of
            // its own rather than none.
            let mut fits = 0;
            let mut taken = 0;
            while from + fits < glyphs.len() {
                let cells = cells(glyphs[from + fits]);
                if taken + cells > width {
                    break;
                }
                taken += cells;
                fits += 1;
            }
            let fits = fits.max(1);
            if from + fits >= glyphs.len() {
                lines.push((glyphs[from..].concat(), at + from));
                break;
            }
            // After the last space that fits, or through a word longer
            // than the pane.
            let take = glyphs[from..from + fits]
                .iter()
                .rposition(|glyph| *glyph == " ")
                .map_or(fits, |space| space + 1);
            lines.push((glyphs[from..from + take].concat(), at + from));
            from += take;
        }
        // The newline the split took off.
        at += glyphs.len() + 1;
    }
    lines
}

/// Which drawn line a caret is on, and how many clusters along it.
fn caret_at(lines: &[(String, usize)], caret: usize) -> (usize, usize) {
    let row = lines
        .iter()
        .rposition(|(_, start)| *start <= caret)
        .unwrap_or_default();
    (row, caret - lines.get(row).map_or(0, |(_, start)| *start))
}

/// As much of `text` as fits in `width` cells. A cluster that would
/// straddle the edge is left out whole, with the marks that belong to
/// it.
fn clip(text: &str, width: u16) -> &str {
    let mut taken = 0;
    for (at, glyph) in text.grapheme_indices(true) {
        taken += cells(glyph);
        if taken > width {
            return &text[..at];
        }
    }
    text
}

/// The byte offset a caret counted in clusters points at, so that a line
/// can be cut where the caret is without cutting a cluster in half.
fn glyph_at(text: &str, caret: usize) -> usize {
    text.grapheme_indices(true)
        .nth(caret)
        .map_or(text.len(), |(at, _)| at)
}
