use super::*;

#[test]
fn a_dirty_deleted_note_recovers_text_and_editor_state_once() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    app.update(Action::NotesPage);
    let original = note_saying(&mut app, "baseline");
    app.update(Action::Tick);
    app.update(Action::Left);
    app.update(Action::Right);
    type_in(&mut app, " local");
    app.update(Action::SelectLeft);
    let before = app.draft().unwrap().clone();
    let selection = app.selection();
    elsewhere(&store, Command::DeleteNote { note: original });

    app.update(Action::Tick);
    let recovered = app.draft().expect("dirty text remains in an editor");
    let recovery = recovered.note;
    assert_ne!(recovery, original);
    assert_eq!(recovered.text, "baseline local");
    assert_eq!(recovered.saved, recovered.text);
    assert_eq!(recovered.caret, before.caret);
    assert_eq!(recovered.affinity, before.affinity);
    assert_eq!(recovered.first, before.first);
    assert_eq!(app.selection(), selection);
    assert_eq!(note_cursor(&app), Some(recovery));
    assert!(hint(&app).contains("deleted that note"));
    assert!(!app.model().note(original).unwrap().is_live());
    assert_eq!(app.model().note(original).unwrap().body, "baseline");
    assert_eq!(note_bodies(&app), ["baseline local"]);

    app.update(Action::Tick);
    assert_eq!(app.model().notes.len(), 2, "recovery is not repeated");
    app.update(Action::UndoText);
    app.update(Action::Tick);
    assert_eq!(app.model().note(recovery).unwrap().body, "baseline");
    app.update(Action::RedoText);
    app.update(Action::Tick);
    assert_eq!(app.model().note(recovery).unwrap().body, "baseline local");

    app.update(Action::Cancel);
    app.update(Action::Undo);
    assert!(!app.model().note(recovery).unwrap().is_live());
    assert!(!app.model().note(original).unwrap().is_live());
}

#[test]
fn leaving_or_quitting_a_deleted_dirty_note_saves_the_recovery_first() {
    for action in [Action::Cancel, Action::NotesPage, Action::Quit] {
        let store = MemStore::new();
        let mut app = app_at(store.clone(), NOW);
        app.update(Action::NotesPage);
        let original = note_saying(&mut app, "baseline");
        app.update(Action::Tick);
        type_in(&mut app, " local");
        elsewhere(&store, Command::DeleteNote { note: original });
        let flow = app.update(action);
        assert_eq!(flow == Flow::Quit, action == Action::Quit);
        assert_eq!(note_bodies(&app), ["baseline local"]);
        assert!(!app.model().note(original).unwrap().is_live());
        assert_eq!(store.load().unwrap(), *app.model());
    }
}

struct RecoveryFailure {
    inner: MemStore,
    failing: Rc<Cell<bool>>,
    conflict: bool,
}

impl Store for RecoveryFailure {
    fn load(&self) -> Result<Model, StoreError> {
        self.inner.load()
    }
    fn version(&self) -> Result<u64, StoreError> {
        self.inner.version()
    }
    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        if self.failing.get() {
            return Err(if self.conflict {
                StoreError::Conflict
            } else {
                StoreError::Other("disk is full".to_owned())
            });
        }
        self.inner.commit(change)
    }
}

#[test]
fn failed_deleted_note_recovery_keeps_the_draft_and_retries_without_duplicates() {
    for conflict in [false, true] {
        let store = MemStore::new();
        let mut seed = app_at(store.clone(), NOW);
        seed.update(Action::NotesPage);
        let original = note_saying(&mut seed, "baseline");
        seed.update(Action::Tick);
        let failing = Rc::new(Cell::new(false));
        let mut app = App::new(
            Box::new(RecoveryFailure {
                inner: store.clone(),
                failing: failing.clone(),
                conflict,
            }),
            Box::new(Desk::absent()),
            Locale::default(),
            &at(NOW),
        )
        .unwrap();
        app.update(Action::NotesPage);
        app.update(Action::Confirm);
        type_in(&mut app, " local");
        elsewhere(&store, Command::DeleteNote { note: original });
        failing.set(true);
        app.update(Action::Tick);
        assert_eq!(app.draft().unwrap().text, "baseline local");
        assert_eq!(store.load().unwrap().notes.len(), 1);
        app.update(Action::Cancel);
        assert!(app.draft().is_some());
        assert_eq!(app.update(Action::Quit), Flow::Continue);
        assert!(hint(&app).contains("could not be saved"));
        failing.set(false);
        app.update(Action::Tick);
        assert_eq!(note_bodies(&app), ["baseline local"]);
        app.update(Action::Tick);
        assert_eq!(store.load().unwrap().notes.len(), 2);
        assert_ne!(app.draft().unwrap().note, original);
    }
}

#[test]
fn deletion_recovery_preserves_a_scrolled_soft_wrap_position() {
    let store = MemStore::new();
    let mut app = app_at(store.clone(), NOW);
    app.update(Action::NotesPage);
    let original = note_saying(&mut app, &"abcdefghij".repeat(8));
    app.update(Action::Tick);
    type_in(&mut app, "local");
    note_pane(&mut app, 10, 3);
    app.update(Action::Up);
    app.update(Action::LineEnd);
    let before = app.draft().unwrap().clone();
    assert!(before.first > 0);
    assert_eq!(before.affinity, Affinity::BeforeTheBreak);
    elsewhere(&store, Command::DeleteNote { note: original });
    app.update(Action::Tick);
    note_pane(&mut app, 10, 3);
    let after = app.draft().unwrap();
    assert_ne!(after.note, original);
    assert_eq!(after.first, before.first);
    assert_eq!(after.caret, before.caret);
    assert_eq!(after.affinity, before.affinity);
}
