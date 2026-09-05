//! Application state, the launch sequence, reloading, turning actions
//! into commands, and the screen layout.

use jiff::Zoned;
use jiff::civil::Date;
use tracing::warn;

use crate::domain::{
    self, BacklogView, Change, Command, DayView, Id, Model, NotesView, Place, Rule, SearchResults,
    Store, StoreError, Weekday, Write,
};
use crate::input::{self, Action, Binding, Field, KeyContext, NotesPane, Pane, PopupKind};

#[cfg(test)]
mod tests;

/// The length the undo stack is held to. The domain does not choose the
/// number (DOMAIN.md section 11); a hundred is more than a day's work and
/// small enough to load with everything else.
const UNDO_CAP: usize = 100;

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

/// Which group of a pane a row is in.
///
/// The domain decides what is in each group; the application needs the
/// name of the group because a key on a row can mean something different
/// in each: `J` swaps neighbours within one group, and a moved row is a
/// pointer rather than a task to act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Focus,
    Plan,
    Done,
    Moved,
    Ordinary,
    Waiting,
    Notes,
}

impl Group {
    /// Whether `J` and `K` mean anything in it. Done is ordered by the
    /// time each task was closed and Moved by when the task left, so
    /// neither has an order to change.
    fn is_ordered_by_hand(self) -> bool {
        matches!(
            self,
            Group::Focus | Group::Plan | Group::Ordinary | Group::Waiting
        )
    }
}

/// The hint bar's last word: what just happened, and whether `u` takes it
/// back. It stands until the next key (DESIGN.md section 8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub text: String,
    pub undo: bool,
}

/// A title being typed on a row. Uncommitted text lives here and nowhere
/// else (ARCHITECTURE.md rule 8); Enter turns it into one command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Editor {
    pub field: Field,
    /// The list the field is in, which is also the place a new task goes.
    pub list: List,
    /// The task being renamed; none while one is being added.
    pub task: Option<Id>,
    pub text: String,
    pub caret: usize,
}

/// A popup over the page. What is typed into it is application state for
/// the same reason a title being edited is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Popup {
    pub kind: PopupKind,
    /// What has been typed into it, and where the caret is, in characters.
    pub text: String,
    pub caret: usize,
    /// Which row of its list is selected. A popup lists commands, results
    /// and days, not model rows, so an index is what it means.
    pub selected: usize,
    /// The task the popup is about, held by id so that a reload cannot
    /// turn it into another one.
    pub target: Option<Id>,
}

/// Where the move card sends a task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveTarget {
    Day(Date),
    Backlog,
    /// The date card, which phase 8 draws.
    Pick,
}

/// One row of the move card: its key and its name from the key table, and
/// the day it means, which only the application can work out.
#[derive(Clone, Copy, Debug)]
pub struct MoveChoice {
    pub key: &'static str,
    pub label: &'static str,
    pub target: MoveTarget,
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

/// The views the screen is drawn from, recomputed whenever the model or
/// the working day changes and at no other time.
#[derive(Default)]
struct Views {
    day: DayView,
    backlog: BacklogView,
    notes: NotesView,
    /// The size of the pile, which the status line counts in red.
    pile: usize,
}

pub struct App {
    store: Box<dyn Store>,
    model: Model,
    /// The database version the model was loaded at, so a change made by
    /// another window can be noticed.
    version: u64,
    today: Date,
    views: Views,
    page: Page,
    pane: Pane,
    notes_pane: NotesPane,
    popup: Option<Popup>,
    editor: Option<Editor>,
    message: Option<Message>,
    cursors: Cursors,
    /// The row a reorder is happening to, marked "moving" until the next
    /// key. Application state, like the title being typed (DOMAIN.md
    /// section 18).
    moving: Option<Id>,
    /// The row the mouse took hold of, while it holds it.
    dragging: Option<(List, Id)>,
    layout: Layout,
    /// Where the clock comes from. The application is the only module
    /// that reads it (ARCHITECTURE.md section 3), which is also what
    /// makes it something a test can hold still: a test's app keeps the
    /// instant it was built with, and the program itself has no field
    /// here at all.
    #[cfg(test)]
    clock: Zoned,
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
            views: Views::default(),
            page: Page::Home,
            pane: Pane::Day,
            notes_pane: NotesPane::List,
            popup: None,
            editor: None,
            message: None,
            cursors: Cursors::default(),
            moving: None,
            dragging: None,
            layout: Layout::default(),
            #[cfg(test)]
            clock: now.clone(),
        };
        app.refresh();
        app.rest_the_cursors();
        Ok(app)
    }

    /// The single entry point for every event, ticks included.
    pub fn update(&mut self, action: Action) -> Flow {
        // What the hint bar last said, and the mark on a row being
        // carried, stand until the next key.
        if !matches!(action, Action::Tick | Action::Resize | Action::FocusGained) {
            self.message = None;
            self.moving = None;
        }

        match action {
            Action::Quit => return Flow::Quit,
            Action::Tick | Action::FocusGained => {
                // The clock is read here and nowhere else, so the date
                // rolling over while the window is open is just a tick.
                let today = domain::working_day(&self.now());
                let rolled = today != self.today;
                self.today = today;
                if self.reload_if_stale() || rolled {
                    self.refresh();
                }
            }
            Action::Resize => {}

            Action::Down => self.step(true),
            Action::Up => self.step(false),
            Action::PaneLeft => self.shift_pane(false, false),
            Action::PaneRight => self.shift_pane(true, false),
            Action::NextPane => self.shift_pane(true, true),
            Action::NotesPage => self.turn_the_page(),

            Action::Commands => self.open(PopupKind::Palette, None),
            Action::Search => self.open(PopupKind::Search, None),
            Action::Help => self.open(PopupKind::Help, None),
            Action::Cancel => self.back_out(),
            Action::Confirm => return self.confirm(),

            Action::Add => self.start_adding(),
            Action::Edit => self.start_renaming(),
            Action::Close => self.close_or_reopen(),
            Action::Focus => self.turn_focus_over(),
            Action::Delete => self.delete(),
            Action::MoveDown => self.reorder(true),
            Action::MoveUp => self.reorder(false),
            Action::ToToday => self.move_it(MoveTarget::Day(self.today)),
            Action::ToBacklog => self.move_it(MoveTarget::Backlog),
            Action::Tomorrow => self.move_it(self.target_for(Action::Tomorrow)),
            Action::NextWorkDay => self.move_it(self.target_for(Action::NextWorkDay)),
            Action::NextMonday => self.move_it(self.target_for(Action::NextMonday)),
            Action::MoveToDay => self.open_the_move_card(),
            Action::GoToDate => self.not_yet("The date card is not built yet."),
            Action::ThisCopy => self.answer_the_question(false),
            Action::ThisAndFuture => self.answer_the_question(true),
            Action::Undo => self.undo(),

            Action::PrevDay | Action::NextDay => {
                self.not_yet("Stepping through days is not built yet.");
            }
            Action::Today => {}
            Action::DueBy | Action::RemindOn => {
                self.not_yet("Due dates and reminders are not built yet.");
            }
            Action::Waiting => self.wait_on_someone(),
            Action::Repeat => self.not_yet("The repeat card is not built yet."),
            Action::Keep => {}

            Action::Insert(typed) => self.type_in(typed),
            Action::Backspace => self.rub_out(),
            Action::DeleteForward => self.rub_forward(),
            Action::Left => self.move_caret(false),
            Action::Right => self.move_caret(true),
            Action::LineStart => self.set_caret(0),
            Action::LineEnd => self.set_caret(usize::MAX),

            Action::MouseDown { column, row } => self.point_at(column, row),
            Action::MouseDrag { column, row } => self.drag_to(column, row),
            Action::MouseUp { .. } => self.dragging = None,
            Action::Scroll { down, .. } => self.step(down),
        }
        Flow::Continue
    }

    /// The instant an action happens at, read once per action.
    fn now(&self) -> Zoned {
        #[cfg(test)]
        return self.clock.clone();
        #[cfg(not(test))]
        Zoned::now()
    }

    // ---- the model ---------------------------------------------------

    /// Recomputes every view. Views are pure functions of the model and
    /// the working day, so this is the only thing that has to happen when
    /// either changes.
    fn refresh(&mut self) {
        self.views = Views {
            day: domain::day_view(&self.model, self.today, self.today),
            backlog: domain::backlog_view(&self.model, self.today),
            notes: domain::notes(&self.model),
            pile: domain::pile(&self.model, self.today).total,
        };
    }

    /// Picks up what another window has done, and says whether it did.
    /// `version` moves only for a write made on another connection, so
    /// this is free when nothing has happened.
    fn reload_if_stale(&mut self) -> bool {
        let version = match self.store.version() {
            Ok(version) => version,
            Err(error) => {
                warn!(%error, "the database version could not be read");
                return false;
            }
        };
        if version == self.version {
            return false;
        }
        match self.store.load() {
            Ok(model) => {
                self.model = model;
                self.version = version;
                true
            }
            Err(error) => {
                warn!(%error, "the model could not be reloaded");
                false
            }
        }
    }

    /// One command, all the way through: reload if another window has
    /// written, ask the domain what the change would be, have storage
    /// commit it, and apply it here. The change comes back so that the
    /// caller can find the row it created.
    fn run(&mut self, command: Command) -> Option<Change> {
        self.reload_if_stale();
        let now = self.now();
        let change = match domain::apply(&self.model, command, &now, UNDO_CAP) {
            Ok(change) => change,
            Err(rejected) => {
                self.say(rejected.to_string(), false);
                return None;
            }
        };
        let label = label_of(&change);
        self.commit(&change)?;
        if let Some(label) = label {
            self.say(label, true);
        }
        Some(change)
    }

    /// Commits a change and applies it to the model, or leaves the model
    /// as it was and says what went wrong (ARCHITECTURE.md rule 10).
    fn commit(&mut self, change: &Change) -> Option<()> {
        let committed = self.store.commit(change);
        if let Err(error) = committed {
            match error {
                StoreError::Conflict => {
                    self.say(
                        "Another window changed that first. Nothing was saved.",
                        false,
                    );
                }
                StoreError::Other(ref why) => {
                    warn!(%why, "the change could not be saved");
                    self.say("The change could not be saved.", false);
                }
            }
            return None;
        }
        self.model.apply(change);
        if let Ok(version) = self.store.version() {
            self.version = version;
        }
        self.refresh();
        Some(())
    }

    /// `u`: the top of the undo stack, applied as its inverse. An entry
    /// whose inverse no longer holds is dropped, and the hint bar says so
    /// (DOMAIN.md section 11).
    fn undo(&mut self) {
        self.reload_if_stale();
        let now = self.now();
        let undone = match domain::undo(&self.model, &now) {
            Ok(undone) => undone,
            Err(rejected) => {
                self.say(rejected.to_string(), false);
                return;
            }
        };
        if self.commit(&undone.change).is_none() {
            return;
        }
        match undone.dropped {
            Some(why) => self.say(
                format!("{} could not be undone: {why}", undone.label),
                false,
            ),
            None => self.say(format!("Undone: {}", undone.label), false),
        }
    }

    fn say(&mut self, text: impl Into<String>, undo: bool) {
        self.message = Some(Message {
            text: text.into(),
            undo,
        });
    }

    /// A key the shell already declares and a later phase fills in.
    fn not_yet(&mut self, what: &str) {
        self.say(what, false);
    }

    // ---- the rows ----------------------------------------------------

    /// Every row of a list in the order they are drawn, which is the
    /// order the cursor moves in, each with the group it is in. The
    /// groups and their order are the domain's (DOMAIN.md sections 6
    /// and 7).
    fn rows_of(&self, list: List) -> Vec<(Id, Group)> {
        let tasks = |rows: &[domain::Row], group| {
            rows.iter().map(|row| (row.task, group)).collect::<Vec<_>>()
        };
        match list {
            List::Day => {
                let day = &self.views.day;
                [
                    tasks(&day.focus, Group::Focus),
                    tasks(&day.plan, Group::Plan),
                    tasks(&day.done, Group::Done),
                    tasks(&day.moved, Group::Moved),
                ]
                .concat()
            }
            List::Backlog => {
                let backlog = &self.views.backlog;
                [
                    tasks(&backlog.ordinary, Group::Ordinary),
                    tasks(&backlog.waiting, Group::Waiting),
                ]
                .concat()
            }
            List::Notes => self
                .views
                .notes
                .rows
                .iter()
                .map(|row| (row.note, Group::Notes))
                .collect(),
        }
    }

    fn group_of(&self, list: List, id: Id) -> Option<Group> {
        self.rows_of(list)
            .into_iter()
            .find(|(row, _)| *row == id)
            .map(|(_, group)| group)
    }

    /// The task a key on the cursor row acts on, or nothing and a reason.
    fn task_at_cursor(&mut self) -> Option<Id> {
        if self.page == Page::Notes {
            self.not_yet("The notes page is not built yet.");
            return None;
        }
        let list = self.focused();
        let Some(id) = self.cursor(list) else {
            self.say("There is no task here yet.", false);
            return None;
        };
        if self.group_of(list, id) == Some(Group::Moved) {
            self.say("That row only points at the task; it has moved.", false);
            return None;
        }
        Some(id)
    }

    /// The row the cursor lands on when the one it is on leaves the list:
    /// the next of its group, then the one before it, then whatever is
    /// nearest.
    fn neighbour_of(&self, list: List, id: Id) -> Option<Id> {
        let rows = self.rows_of(list);
        let at = rows.iter().position(|(row, _)| *row == id)?;
        let group = rows[at].1;
        let same = |(row, other): &&(Id, Group)| *other == group && *row != id;

        rows[at + 1..]
            .iter()
            .find(same)
            .or_else(|| rows[..at].iter().rev().find(same))
            .or_else(|| rows[at + 1..].first())
            .or_else(|| rows[..at].last())
            .map(|(row, _)| *row)
    }

    // ---- the keys on a row -------------------------------------------

    /// `space`: close an open task, reopen a closed one. Closing a task
    /// in the backlog puts it on today first, because that is where it
    /// was done.
    fn close_or_reopen(&mut self) {
        let Some(id) = self.task_at_cursor() else {
            return;
        };
        let closed = self.model.live_task(id).is_some_and(|task| !task.is_open());
        let list = self.focused();
        // A reopened task stays in the list, so the cursor stays on it.
        let next = if closed {
            None
        } else {
            self.neighbour_of(list, id)
        };

        let command = if closed {
            Command::Reopen { task: id }
        } else {
            Command::Close { task: id }
        };
        if self.run(command).is_some()
            && let Some(next) = next
        {
            self.set_cursor(list, next);
        }
    }

    /// `f`: one of the day's must-dos, or not.
    fn turn_focus_over(&mut self) {
        let Some(id) = self.task_at_cursor() else {
            return;
        };
        let Some(task) = self.model.live_task(id) else {
            return;
        };
        let focus = !task.focus;
        self.run(Command::SetFocus { task: id, focus });
    }

    /// `w`: blocked on someone or something else. Waiting is a backlog
    /// state, so on a day task the domain moves the task to the backlog
    /// with the flag, and the cursor steps on as it does for any move
    /// (DOMAIN.md section 9).
    fn wait_on_someone(&mut self) {
        let Some(id) = self.task_at_cursor() else {
            return;
        };
        let Some(task) = self.model.live_task(id) else {
            return;
        };
        let waiting = !task.waiting;
        let leaves = waiting && task.day.is_some();
        let list = self.focused();
        let next = leaves.then(|| self.neighbour_of(list, id)).flatten();

        if self
            .run(Command::SetWaiting { task: id, waiting })
            .is_some()
            && let Some(next) = next
        {
            self.set_cursor(list, next);
        }
    }

    /// `x`: no confirm, and `u` in the hint bar until the next key.
    fn delete(&mut self) {
        let Some(id) = self.task_at_cursor() else {
            return;
        };
        let list = self.focused();
        let next = self.neighbour_of(list, id);
        if self.run(Command::DeleteTask { task: id }).is_some()
            && let Some(next) = next
        {
            self.set_cursor(list, next);
        }
    }

    /// `J` and `K`: swap with the neighbour in the same group, because
    /// what the screen shows as two groups is one sequence of positions
    /// underneath (DOMAIN.md section 4).
    fn reorder(&mut self, down: bool) {
        let Some(id) = self.task_at_cursor() else {
            return;
        };
        let list = self.focused();
        let Some(group) = self.group_of(list, id) else {
            return;
        };
        if !group.is_ordered_by_hand() {
            self.say("Those rows keep the order they are in.", false);
            return;
        }

        let siblings: Vec<Id> = self
            .rows_of(list)
            .into_iter()
            .filter(|(_, other)| *other == group)
            .map(|(row, _)| row)
            .collect();
        let Some(at) = siblings.iter().position(|row| *row == id) else {
            return;
        };
        let swap = if down {
            Some(at + 1)
        } else {
            at.checked_sub(1)
        };
        let Some(other) = swap.and_then(|swap| siblings.get(swap)) else {
            return;
        };
        let Some(position) = self.model.live_task(*other).map(|task| task.position) else {
            return;
        };
        self.reorder_to(id, position);
    }

    /// One step of a reorder, wherever it came from. The row is marked
    /// "moving" until the next key, which is the only sign the screen has
    /// that a row is being carried rather than just selected.
    fn reorder_to(&mut self, task: Id, position: usize) {
        if self.run(Command::Reorder { task, position }).is_some() {
            self.moving = Some(task);
        }
    }

    /// `t`, `b`, and the days of the move card. With the card open the
    /// task is the one the card was opened on; otherwise it is the row
    /// the cursor is on.
    fn move_it(&mut self, target: MoveTarget) {
        if target == MoveTarget::Pick {
            self.not_yet("The date card is not built yet.");
            return;
        }
        let carded = self.take_the_card();
        let Some(id) = carded.or_else(|| self.task_at_cursor()) else {
            return;
        };
        let list = self.focused();
        let next = self.neighbour_of(list, id);

        let place = match target {
            MoveTarget::Day(day) => Place::Day(day),
            MoveTarget::Backlog => Place::Backlog,
            MoveTarget::Pick => return,
        };
        if self.run(Command::Move { task: id, place }).is_some()
            && let Some(next) = next
        {
            self.set_cursor(list, next);
        }
    }

    /// Closes the move card and answers the task it was opened on.
    fn take_the_card(&mut self) -> Option<Id> {
        let open = self.popup.as_ref()?;
        if open.kind != PopupKind::Move {
            return None;
        }
        let target = open.target;
        self.popup = None;
        target
    }

    fn open_the_move_card(&mut self) {
        let Some(id) = self.task_at_cursor() else {
            return;
        };
        self.open(PopupKind::Move, Some(id));
    }

    /// The day each row of the move card means. "Next work day" is the
    /// work-days rule's own definition of one (DOMAIN.md section 10).
    fn target_for(&self, action: Action) -> MoveTarget {
        // A day the calendar cannot reach, which is only ever the last
        // day it has, is offered as the date card rather than as some
        // other day the row does not name.
        let next = |rule: Rule| {
            domain::next_dates(&rule, self.today, 1)
                .first()
                .copied()
                .map_or(MoveTarget::Pick, MoveTarget::Day)
        };
        match action {
            Action::ToToday => MoveTarget::Day(self.today),
            Action::Tomorrow => self
                .today
                .tomorrow()
                .map_or(MoveTarget::Pick, MoveTarget::Day),
            Action::NextWorkDay => next(Rule::Workdays),
            Action::NextMonday => next(Rule::Weekly {
                weekdays: vec![Weekday::Mon],
            }),
            Action::ToBacklog => MoveTarget::Backlog,
            _ => MoveTarget::Pick,
        }
    }

    /// The rows of the move card: the keys and names from the key table,
    /// and the day each one means.
    pub fn move_choices(&self) -> Vec<MoveChoice> {
        input::bindings(KeyContext::Popup {
            kind: PopupKind::Move,
            text_field: false,
        })
        .iter()
        .filter_map(|binding| {
            let (key, action) = *binding.keys.first()?;
            let target = match action {
                Action::ToToday
                | Action::Tomorrow
                | Action::NextWorkDay
                | Action::NextMonday
                | Action::ToBacklog => self.target_for(action),
                Action::GoToDate => MoveTarget::Pick,
                _ => return None,
            };
            Some(MoveChoice {
                key,
                label: binding.label,
                target,
            })
        })
        .collect()
    }

    // ---- the title being typed ---------------------------------------

    /// `a`: a field at the end of the list, which Enter empties and keeps
    /// open, so a list of tasks is typed in one go.
    fn start_adding(&mut self) {
        if self.page == Page::Notes {
            self.not_yet("The notes page is not built yet.");
            return;
        }
        self.editor = Some(Editor {
            field: Field::Adding,
            list: self.focused(),
            task: None,
            text: String::new(),
            caret: 0,
        });
    }

    /// `e`: the row itself becomes the field. Editing is always in place
    /// (DESIGN.md section 8).
    fn start_renaming(&mut self) {
        let Some(id) = self.task_at_cursor() else {
            return;
        };
        let Some(task) = self.model.live_task(id) else {
            return;
        };
        let text = task.title.clone();
        self.editor = Some(Editor {
            field: Field::Renaming,
            list: self.focused(),
            task: Some(id),
            caret: text.chars().count(),
            text,
        });
    }

    /// Enter in the field: one command, and then either the field again
    /// or the question a recurring copy asks.
    fn commit_the_title(&mut self) {
        let Some(editor) = &self.editor else {
            return;
        };
        let (field, list, task, title) = (
            editor.field,
            editor.list,
            editor.task,
            editor.text.trim().to_owned(),
        );

        match (field, task) {
            (Field::Adding, _) => {
                // An empty field is how quick add is stopped.
                if title.is_empty() {
                    self.editor = None;
                    return;
                }
                let place = self.place_of(list);
                if let Some(change) = self.run(Command::AddTask { title, place })
                    && let Some(added) = added_task(&change)
                {
                    self.set_cursor(list, added);
                }
                if let Some(editor) = &mut self.editor {
                    editor.text.clear();
                    editor.caret = 0;
                }
            }
            (Field::Renaming, Some(id)) => {
                let copy = self
                    .model
                    .live_task(id)
                    .is_some_and(|task| task.schedule_id.is_some());
                if copy && !title.is_empty() {
                    // The one deliberate question (DESIGN.md section 8).
                    self.editor = None;
                    self.open(PopupKind::CopyQuestion, Some(id));
                    if let Some(popup) = &mut self.popup {
                        popup.text = title;
                    }
                    return;
                }
                if self.run(Command::EditTitle { task: id, title }).is_some() {
                    self.editor = None;
                }
            }
            (Field::Renaming, None) => self.editor = None,
        }
    }

    /// "This copy" renames the task; "this and future copies" renames the
    /// schedule with it (DOMAIN.md section 10).
    fn answer_the_question(&mut self, future: bool) {
        let Some(popup) = self.popup.take() else {
            return;
        };
        let Some(task) = popup.target else {
            return;
        };
        let title = popup.text;
        let command = if future {
            Command::EditTitleAndFuture {
                task,
                schedule_title: title.clone(),
                title,
            }
        } else {
            Command::EditTitle { task, title }
        };
        self.run(command);
    }

    fn place_of(&self, list: List) -> Place {
        match list {
            List::Backlog => Place::Backlog,
            List::Day | List::Notes => Place::Day(self.today),
        }
    }

    // ---- what the keyboard is on ------------------------------------

    pub fn key_context(&self) -> KeyContext {
        match &self.popup {
            Some(popup) => KeyContext::Popup {
                kind: popup.kind,
                // The palette and search are typed into; the others are
                // read and answered with a key.
                text_field: matches!(popup.kind, PopupKind::Palette | PopupKind::Search),
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
                field: self.editor.as_ref().map(|editor| editor.field),
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

    pub fn day(&self) -> &DayView {
        &self.views.day
    }

    pub fn backlog(&self) -> &BacklogView {
        &self.views.backlog
    }

    pub fn notes(&self) -> &NotesView {
        &self.views.notes
    }

    /// The size of the pile: the "n in review" the status line counts,
    /// whatever was or was not decided (DOMAIN.md section 13).
    pub fn review_count(&self) -> usize {
        self.views.pile
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

    pub fn editor(&self) -> Option<&Editor> {
        self.editor.as_ref()
    }

    pub fn message(&self) -> Option<&Message> {
        self.message.as_ref()
    }

    /// The row being carried up or down its group, if one is.
    pub fn moving(&self) -> Option<Id> {
        self.moving
    }

    /// The cursor of a list, re-resolved against what the list holds now:
    /// a row that has gone clamps to the first one (ARCHITECTURE.md rule
    /// 6).
    pub fn cursor(&self, list: List) -> Option<Id> {
        let rows = self.rows_of(list);
        let wanted = match list {
            List::Day => self.cursors.day,
            List::Backlog => self.cursors.backlog,
            List::Notes => self.cursors.notes,
        };
        match wanted {
            Some(id) if rows.iter().any(|(row, _)| *row == id) => Some(id),
            // The row has gone since, so the cursor clamps to the first.
            _ => rows.first().map(|(row, _)| *row),
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
    pub fn search_results(&self) -> SearchResults {
        match &self.popup {
            Some(popup) if popup.kind == PopupKind::Search => {
                domain::search(&self.model, popup.text.trim(), self.today)
            }
            _ => SearchResults::default(),
        }
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
    }

    // ---- moving about ------------------------------------------------

    fn rest_the_cursors(&mut self) {
        self.cursors = Cursors {
            day: self.rows_of(List::Day).first().map(|(id, _)| *id),
            backlog: self.rows_of(List::Backlog).first().map(|(id, _)| *id),
            notes: self.rows_of(List::Notes).first().map(|(id, _)| *id),
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
        let ids: Vec<Id> = self.rows_of(list).into_iter().map(|(id, _)| id).collect();
        let Some(at) = self
            .cursor(list)
            .and_then(|id| ids.iter().position(|other| *other == id))
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
            Some(PopupKind::Search) => self.search_results().total,
            Some(PopupKind::Move) => self.move_choices().len(),
            _ => 0,
        }
    }

    /// The next pane, or the next tab when the window has collapsed to
    /// one. `wrap` is `tab`, which goes round; `h` and `l` stop.
    fn shift_pane(&mut self, forward: bool, wrap: bool) {
        if self.popup.is_some() || self.editor.is_some() {
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
        if self.popup.is_some() || self.editor.is_some() {
            return;
        }
        let Some(list) = self.layout.list_at(column, row) else {
            return;
        };
        self.focus_on(list);
        if let Some(clicked) = self.layout.row_at(column, row) {
            self.set_cursor(clicked.list, clicked.id);
            self.dragging = Some((clicked.list, clicked.id));
        }
    }

    /// Dragging carries the row under the pointer, one reorder per row it
    /// passes, so that it follows the mouse instead of jumping when the
    /// button comes up. Nothing is only reachable by mouse: this is the
    /// same command `J` and `K` send.
    fn drag_to(&mut self, column: u16, row: u16) {
        let Some((list, task)) = self.dragging else {
            return;
        };
        let Some(over) = self.layout.row_at(column, row) else {
            return;
        };
        if over.list != list || over.id == task {
            return;
        }
        let group = self.group_of(list, task);
        if group != self.group_of(list, over.id) || !group.is_some_and(Group::is_ordered_by_hand) {
            return;
        }
        let Some(position) = self.model.live_task(over.id).map(|task| task.position) else {
            return;
        };
        self.set_cursor(list, task);
        self.reorder_to(task, position);
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

    fn open(&mut self, kind: PopupKind, target: Option<Id>) {
        self.editor = None;
        self.popup = Some(Popup {
            kind,
            text: String::new(),
            caret: 0,
            selected: 0,
            target,
        });
    }

    /// Escape backs out one level: the popup first, then the field.
    fn back_out(&mut self) {
        if self.popup.take().is_some() {
            return;
        }
        self.editor = None;
    }

    /// Enter: the popup if one is open, and the field under it otherwise.
    fn confirm(&mut self) -> Flow {
        let Some(kind) = self.popup.as_ref().map(|popup| popup.kind) else {
            self.commit_the_title();
            return Flow::Continue;
        };
        match kind {
            PopupKind::Palette => return self.run_the_selected_command(),
            PopupKind::Search => self.take_the_search(),
            PopupKind::Move => self.take_the_chosen_day(),
            // The question has no answer safe enough to be Enter's.
            PopupKind::Help | PopupKind::CopyQuestion => {}
        }
        Flow::Continue
    }

    /// Enter in the palette runs the row it is on, as if its key had been
    /// pressed on the page beneath.
    fn run_the_selected_command(&mut self) -> Flow {
        let Some(popup) = &self.popup else {
            return Flow::Continue;
        };
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

    /// Enter on the move card moves the task to the day the cursor is on.
    fn take_the_chosen_day(&mut self) {
        let Some(popup) = &self.popup else {
            return;
        };
        let Some(choice) = self.move_choices().get(popup.selected).copied() else {
            return;
        };
        self.move_it(choice.target);
    }

    /// Enter in search. A search that found nothing offers to add what was
    /// typed as a task on today (DOMAIN.md section 14); going to a result
    /// is phase 10's.
    fn take_the_search(&mut self) {
        let Some(popup) = &self.popup else {
            return;
        };
        let title = popup.text.trim().to_owned();
        if self.search_results().total > 0 {
            self.not_yet("Going to a task from search is not built yet.");
            return;
        }
        if title.is_empty() {
            return;
        }
        self.popup = None;
        let place = Place::Day(self.today);
        if let Some(change) = self.run(Command::AddTask { title, place })
            && let Some(added) = added_task(&change)
        {
            self.focus_on(List::Day);
            self.set_cursor(List::Day, added);
        }
    }

    /// The text field with the keyboard: the field on a row, or the one
    /// in a popup that is typed into.
    fn field(&mut self) -> Option<(&mut String, &mut usize)> {
        if let Some(popup) = &mut self.popup {
            if !matches!(popup.kind, PopupKind::Palette | PopupKind::Search) {
                return None;
            }
            return Some((&mut popup.text, &mut popup.caret));
        }
        let editor = self.editor.as_mut()?;
        Some((&mut editor.text, &mut editor.caret))
    }

    /// Typing puts the selection of a filtered list back at the top.
    fn select_the_first(&mut self) {
        if let Some(popup) = &mut self.popup {
            popup.selected = 0;
        }
    }

    fn type_in(&mut self, typed: char) {
        if let Some((text, caret)) = self.field() {
            let at = byte_at(text, *caret);
            text.insert(at, typed);
            *caret += 1;
        }
        self.select_the_first();
    }

    fn rub_out(&mut self) {
        if let Some((text, caret)) = self.field() {
            if *caret == 0 {
                return;
            }
            let at = byte_at(text, *caret - 1);
            text.remove(at);
            *caret -= 1;
        }
        self.select_the_first();
    }

    fn rub_forward(&mut self) {
        if let Some((text, caret)) = self.field() {
            let at = byte_at(text, *caret);
            if at < text.len() {
                text.remove(at);
            }
        }
        self.select_the_first();
    }

    fn move_caret(&mut self, forward: bool) {
        if let Some((text, caret)) = self.field() {
            let last = text.chars().count();
            *caret = if forward {
                (*caret + 1).min(last)
            } else {
                caret.saturating_sub(1)
            };
        }
    }

    fn set_caret(&mut self, at: usize) {
        if let Some((text, caret)) = self.field() {
            *caret = at.min(text.chars().count());
        }
    }
}

/// The byte offset of a character offset, so that a caret counted in
/// characters can index a `String`.
fn byte_at(text: &str, caret: usize) -> usize {
    text.char_indices()
        .nth(caret)
        .map_or(text.len(), |(at, _)| at)
}

/// What the hint bar calls a change, which is the label the domain put on
/// the undo entry. A change with no entry is one nobody can undo.
fn label_of(change: &Change) -> Option<String> {
    change.writes.iter().find_map(|write| match write {
        Write::PushUndo(entry) => Some(entry.label.clone()),
        _ => None,
    })
}

/// The task an AddTask wrote, so the cursor can land on it.
fn added_task(change: &Change) -> Option<Id> {
    change.writes.iter().find_map(|write| match write {
        Write::PutTask(task) => Some(task.id),
        _ => None,
    })
}
