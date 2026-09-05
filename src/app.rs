//! Application state, the launch sequence, reloading, turning actions
//! into commands, and the screen layout.

use jiff::Zoned;
use jiff::civil::Date;
use tracing::warn;

use crate::domain::{self, Model, Store, StoreError};
use crate::input::{self, Action, Binding, KeyContext, NotesPane, Pane, PopupKind};

pub mod demo;
#[cfg(test)]
mod tests;

use demo::{Fixture, Id, PaneView, Results};

/// Whether the event loop goes round again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
}

/// Which of the two pages the window is showing (DESIGN.md section 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Home,
    Notes,
}

/// A list of rows, on either page. The cursor is one per list, so
/// switching pane and coming back lands where it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum List {
    Day,
    Backlog,
    Notes,
}

/// A popup over the page. Uncommitted text lives here and nowhere else
/// (ARCHITECTURE.md rule 8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Popup {
    pub kind: PopupKind,
    /// What has been typed into it, and where the caret is, in characters.
    pub text: String,
    pub caret: usize,
    /// Which row of its list is selected. A popup lists commands and
    /// results, not model rows, so an index is what it means.
    pub selected: usize,
}

/// A rectangle of cells, in the terminal's own coordinates. `app` may not
/// name ratatui, and a mouse click is resolved here rather than there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    pub fn holds(self, column: u16, row: u16) -> bool {
        column >= self.x
            && column < self.x.saturating_add(self.width)
            && row >= self.y
            && row < self.y.saturating_add(self.height)
    }
}

/// Where a list was drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListArea {
    pub list: List,
    pub area: Rect,
}

/// Where one row was drawn, and which row it is. By id, never by index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowArea {
    pub list: List,
    pub id: Id,
    pub area: Rect,
}

/// Which pane and which row, with its task or note id, occupies which cell
/// rectangle. `ui::draw` returns one and the mouse is resolved against it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    /// Whether the window was too narrow for two panes and collapsed to
    /// tabs.
    pub narrow: bool,
    pub lists: Vec<ListArea>,
    pub rows: Vec<RowArea>,
}

impl Layout {
    fn row_at(&self, column: u16, row: u16) -> Option<RowArea> {
        self.rows
            .iter()
            .find(|drawn| drawn.area.holds(column, row))
            .copied()
    }

    fn list_at(&self, column: u16, row: u16) -> Option<List> {
        self.lists
            .iter()
            .find(|drawn| drawn.area.holds(column, row))
            .map(|drawn| drawn.list)
    }
}

/// The cursor of each list, by id.
#[derive(Clone, Copy, Debug, Default)]
struct Cursors {
    day: Option<Id>,
    backlog: Option<Id>,
    notes: Option<Id>,
}

pub struct App {
    store: Box<dyn Store>,
    model: Model,
    /// The database version the model was loaded at, so a change made by
    /// another window can be noticed.
    version: u64,
    today: Date,
    page: Page,
    pane: Pane,
    notes_pane: NotesPane,
    popup: Option<Popup>,
    cursors: Cursors,
    layout: Layout,
    /// Phase 6 draws fake data. Phase 7 deletes this field with the
    /// `demo` module.
    fixture: Fixture,
}

impl App {
    /// Loads the model and runs the launch sequence. Phase 8 adds the
    /// generation of recurring copies to it and phase 9 the review gate.
    pub fn new(store: Box<dyn Store>, now: &Zoned) -> Result<App, StoreError> {
        let model = store.load()?;
        let version = store.version()?;

        let mut app = App {
            store,
            model,
            version,
            today: domain::working_day(now),
            page: Page::Home,
            pane: Pane::Day,
            notes_pane: NotesPane::List,
            popup: None,
            cursors: Cursors::default(),
            layout: Layout::default(),
            fixture: Fixture::Filled,
        };
        app.rest_the_cursors();
        Ok(app)
    }

    /// The single entry point for every event, ticks included.
    pub fn update(&mut self, action: Action) -> Flow {
        match action {
            Action::Quit => return Flow::Quit,
            Action::Tick | Action::FocusGained => {
                // The clock is read here and nowhere else, so the date
                // rolling over while the window is open is just a tick.
                self.today = domain::working_day(&Zoned::now());
                self.reload_if_stale();
            }
            Action::Resize => {}

            Action::Down => self.step(true),
            Action::Up => self.step(false),
            Action::PaneLeft => self.shift_pane(false, false),
            Action::PaneRight => self.shift_pane(true, false),
            Action::NextPane => self.shift_pane(true, true),
            Action::NotesPage => self.turn_the_page(),

            Action::Commands => self.open(PopupKind::Palette),
            Action::Search => self.open(PopupKind::Search),
            Action::Help => self.open(PopupKind::Help),
            Action::Cancel => self.popup = None,
            Action::Confirm => return self.run_the_selected_command(),

            Action::Insert(typed) => self.type_in(typed),
            Action::Backspace => self.rub_out(),
            Action::DeleteForward => self.rub_forward(),
            Action::Left => self.move_caret(false),
            Action::Right => self.move_caret(true),
            Action::LineStart => self.set_caret(0),
            Action::LineEnd => self.set_caret(usize::MAX),

            Action::MouseDown { column, row } => self.point_at(column, row),
            Action::Scroll { down, .. } => self.step(down),

            // Phase 7 onwards turns the rest into commands. Until the
            // domain is connected they name what the shell would do.
            _ => {}
        }
        Flow::Continue
    }

    /// Picks up what another window has done. `version` moves only for a
    /// write made on another connection, so this is free when nothing has
    /// happened.
    fn reload_if_stale(&mut self) {
        let version = match self.store.version() {
            Ok(version) => version,
            Err(error) => {
                warn!(%error, "the database version could not be read");
                return;
            }
        };
        if version == self.version {
            return;
        }
        match self.store.load() {
            Ok(model) => {
                self.model = model;
                self.version = version;
            }
            Err(error) => warn!(%error, "the model could not be reloaded"),
        }
    }

    // ---- what the keyboard is on ------------------------------------

    pub fn key_context(&self) -> KeyContext {
        match &self.popup {
            Some(popup) => KeyContext::Popup {
                kind: popup.kind,
                // Help is the one popup that is read rather than typed in.
                text_field: popup.kind != PopupKind::Help,
            },
            None => self.page_context(),
        }
    }

    /// The context of the page itself, which is what the palette lists the
    /// commands of even while it is over it.
    pub fn page_context(&self) -> KeyContext {
        match self.page {
            Page::Home => KeyContext::Home {
                pane: self.pane,
                text_field: false,
            },
            Page::Notes => KeyContext::Notes {
                pane: self.notes_pane,
                text_field: self.notes_pane == NotesPane::Note,
            },
        }
    }

    /// The list the cursor is in.
    pub fn focused(&self) -> List {
        match (self.page, self.pane) {
            (Page::Home, Pane::Day) => List::Day,
            (Page::Home, Pane::Backlog) => List::Backlog,
            (Page::Notes, _) => List::Notes,
        }
    }

    // ---- reading the state, for `ui` --------------------------------

    /// The working day, which is what the status line calls today.
    pub fn today(&self) -> Date {
        self.today
    }

    pub fn model(&self) -> &Model {
        &self.model
    }

    pub fn page(&self) -> Page {
        self.page
    }

    pub fn pane(&self) -> Pane {
        self.pane
    }

    pub fn notes_pane(&self) -> NotesPane {
        self.notes_pane
    }

    pub fn popup(&self) -> Option<&Popup> {
        self.popup.as_ref()
    }

    pub fn fixture(&self) -> Fixture {
        self.fixture
    }

    pub fn view(&self, list: List) -> &'static PaneView {
        match list {
            List::Day => self.fixture.day(),
            List::Backlog => self.fixture.backlog(),
            List::Notes => self.fixture.notes(),
        }
    }

    /// The cursor of a list, re-resolved against what the list holds now:
    /// a row that has gone clamps to the first one (ARCHITECTURE.md rule
    /// 6).
    pub fn cursor(&self, list: List) -> Option<Id> {
        let view = self.view(list);
        let wanted = match list {
            List::Day => self.cursors.day,
            List::Backlog => self.cursors.backlog,
            List::Notes => self.cursors.notes,
        };
        match wanted {
            Some(id) if view.rows().any(|row| row.id == id) => Some(id),
            // The row has gone since, so the cursor clamps to the first.
            _ => view.rows().next().map(|row| row.id),
        }
    }

    /// The commands the palette offers: the rows of the key table for the
    /// page beneath it, narrowed by what has been typed.
    pub fn palette_rows(&self) -> Vec<&'static Binding> {
        let typed = self.popup.as_ref().map(|popup| popup.text.to_lowercase());
        let typed = typed.unwrap_or_default();
        let wanted = typed.trim();
        input::bindings(self.page_context())
            .iter()
            .filter(|binding| !binding.keys.is_empty())
            .filter(|binding| {
                binding.label.to_lowercase().contains(wanted) || binding.shown == wanted
            })
            .collect()
    }

    /// What the search box has found.
    pub fn search_results(&self) -> Results {
        match &self.popup {
            Some(popup) if popup.kind == PopupKind::Search => self.fixture.search(&popup.text),
            _ => Results::default(),
        }
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
    }

    /// Shows the empty fixture, so that the empty states can be looked at.
    /// Phase 7 deletes this with the rest of `demo`.
    #[cfg(test)]
    pub(crate) fn show_empty(&mut self) {
        self.fixture = Fixture::Empty;
        self.rest_the_cursors();
    }

    // ---- moving about ------------------------------------------------

    fn rest_the_cursors(&mut self) {
        self.cursors = Cursors {
            day: self.view(List::Day).rows().next().map(|row| row.id),
            backlog: self.view(List::Backlog).rows().next().map(|row| row.id),
            notes: self.view(List::Notes).rows().next().map(|row| row.id),
        };
    }

    fn set_cursor(&mut self, list: List, id: Id) {
        let slot = match list {
            List::Day => &mut self.cursors.day,
            List::Backlog => &mut self.cursors.backlog,
            List::Notes => &mut self.cursors.notes,
        };
        *slot = Some(id);
    }

    /// One row down or up, in the popup if one is open and in the focused
    /// list otherwise. Both ends stop rather than wrap.
    fn step(&mut self, forward: bool) {
        if self.popup.is_some() {
            let last = self.popup_rows().saturating_sub(1);
            if let Some(popup) = &mut self.popup {
                popup.selected = if forward {
                    (popup.selected + 1).min(last)
                } else {
                    popup.selected.saturating_sub(1)
                };
            }
            return;
        }

        let list = self.focused();
        let ids: Vec<Id> = self.view(list).rows().map(|row| row.id).collect();
        let Some(at) = self
            .cursor(list)
            .and_then(|id| ids.iter().position(|&other| other == id))
        else {
            return;
        };
        let next = if forward {
            (at + 1).min(ids.len().saturating_sub(1))
        } else {
            at.saturating_sub(1)
        };
        self.set_cursor(list, ids[next]);
    }

    /// How many rows the open popup offers.
    fn popup_rows(&self) -> usize {
        match self.popup.as_ref().map(|popup| popup.kind) {
            Some(PopupKind::Palette) => self.palette_rows().len(),
            Some(PopupKind::Search) => self.search_results().count(),
            _ => 0,
        }
    }

    /// The next pane, or the next tab when the window has collapsed to
    /// one. `wrap` is `tab`, which goes round; `h` and `l` stop.
    fn shift_pane(&mut self, forward: bool, wrap: bool) {
        if self.popup.is_some() {
            return;
        }
        let narrow = self.layout.narrow;
        match (self.page, self.pane, self.notes_pane, forward) {
            (Page::Home, Pane::Day, _, true) => self.pane = Pane::Backlog,
            (Page::Home, Pane::Backlog, _, false) => self.pane = Pane::Day,
            // Notes is the third tab when the panes have collapsed.
            (Page::Home, Pane::Backlog, _, true) if narrow => self.page = Page::Notes,
            (Page::Home, Pane::Backlog, _, true) if wrap => self.pane = Pane::Day,
            (Page::Home, Pane::Day, _, false) if wrap => self.pane = Pane::Backlog,

            (Page::Notes, _, NotesPane::List, true) => self.notes_pane = NotesPane::Note,
            (Page::Notes, _, NotesPane::Note, false) => self.notes_pane = NotesPane::List,
            (Page::Notes, _, NotesPane::List, false) if narrow => {
                self.page = Page::Home;
                self.pane = Pane::Backlog;
            }
            (Page::Notes, _, NotesPane::Note, true) if wrap && narrow => {
                self.page = Page::Home;
                self.pane = Pane::Day;
            }
            (Page::Notes, _, NotesPane::Note, true) if wrap => self.notes_pane = NotesPane::List,
            _ => {}
        }
    }

    fn turn_the_page(&mut self) {
        self.page = match self.page {
            Page::Home => Page::Notes,
            Page::Notes => Page::Home,
        };
    }

    /// A click puts the cursor on the row it landed on and moves the
    /// keyboard to that pane. A click on a pane's empty space moves the
    /// keyboard and leaves the cursor where it was.
    fn point_at(&mut self, column: u16, row: u16) {
        if self.popup.is_some() {
            return;
        }
        let Some(list) = self.layout.list_at(column, row) else {
            return;
        };
        self.focus_on(list);
        if let Some(clicked) = self.layout.row_at(column, row) {
            self.set_cursor(clicked.list, clicked.id);
        }
    }

    fn focus_on(&mut self, list: List) {
        match list {
            List::Day => {
                self.page = Page::Home;
                self.pane = Pane::Day;
            }
            List::Backlog => {
                self.page = Page::Home;
                self.pane = Pane::Backlog;
            }
            List::Notes => {
                self.page = Page::Notes;
                self.notes_pane = NotesPane::List;
            }
        }
    }

    // ---- popups and their text fields --------------------------------

    fn open(&mut self, kind: PopupKind) {
        self.popup = Some(Popup {
            kind,
            text: String::new(),
            caret: 0,
            selected: 0,
        });
    }

    /// Enter in the palette runs the row it is on, as if its key had been
    /// pressed on the page beneath.
    fn run_the_selected_command(&mut self) -> Flow {
        let Some(popup) = &self.popup else {
            return Flow::Continue;
        };
        if popup.kind != PopupKind::Palette {
            self.popup = None;
            return Flow::Continue;
        }
        let chosen = self
            .palette_rows()
            .get(popup.selected)
            .and_then(|binding| binding.keys.first())
            .map(|(_, action)| *action);
        self.popup = None;
        match chosen {
            Some(action) => self.update(action),
            None => Flow::Continue,
        }
    }

    fn type_in(&mut self, typed: char) {
        let Some(popup) = &mut self.popup else {
            return;
        };
        let at = byte_at(&popup.text, popup.caret);
        popup.text.insert(at, typed);
        popup.caret += 1;
        popup.selected = 0;
    }

    fn rub_out(&mut self) {
        let Some(popup) = &mut self.popup else {
            return;
        };
        if popup.caret == 0 {
            return;
        }
        let at = byte_at(&popup.text, popup.caret - 1);
        popup.text.remove(at);
        popup.caret -= 1;
        popup.selected = 0;
    }

    fn rub_forward(&mut self) {
        let Some(popup) = &mut self.popup else {
            return;
        };
        let at = byte_at(&popup.text, popup.caret);
        if at < popup.text.len() {
            popup.text.remove(at);
            popup.selected = 0;
        }
    }

    fn move_caret(&mut self, forward: bool) {
        let Some(popup) = &mut self.popup else {
            return;
        };
        let last = popup.text.chars().count();
        popup.caret = if forward {
            (popup.caret + 1).min(last)
        } else {
            popup.caret.saturating_sub(1)
        };
    }

    fn set_caret(&mut self, at: usize) {
        let Some(popup) = &mut self.popup else {
            return;
        };
        popup.caret = at.min(popup.text.chars().count());
    }
}

/// The byte offset of a character offset, so that a caret counted in
/// characters can index a `String`.
fn byte_at(text: &str, caret: usize) -> usize {
    text.char_indices()
        .nth(caret)
        .map_or(text.len(), |(at, _)| at)
}
