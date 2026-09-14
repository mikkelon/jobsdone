//! Persistence and service interoperability through the public app boundary.
use jiff::Zoned;
use jobsdone::{
    app::{App, Desktop, Locale, WindowSize},
    domain::{DateOrder, Store},
    input::Action,
    service,
    storage::Sqlite,
};
const NOW: &str = "2025-09-05T09:00:00+02:00[Europe/Copenhagen]";
fn at(text: &str) -> Zoned {
    text.parse().unwrap()
}
struct NoDesktop;
impl Desktop for NoDesktop {
    fn available(&self) -> bool {
        false
    }
    fn apply_window(&self, _: bool, _: WindowSize) -> Result<(), String> {
        Ok(())
    }
    fn preview(&self, _: WindowSize) -> Result<bool, String> {
        Ok(false)
    }
}

#[test]
fn a_service_delete_on_another_sqlite_connection_recovers_the_dirty_editor() {
    use serde_json::json;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.db");
    let mut other = Sqlite::open(&path).unwrap();
    let created = service::execute(
        &mut other,
        json!({"op":"note.create", "body":"baseline"}),
        &at(NOW),
        DateOrder::DayFirst,
    )
    .unwrap();
    let original = created["data"]["note"]["id"].as_i64().unwrap();
    let mut app = App::new(
        Box::new(Sqlite::open(&path).unwrap()),
        Box::new(NoDesktop),
        Locale::default(),
        &at(NOW),
    )
    .unwrap();
    app.update(Action::NotesPage);
    app.update(Action::Confirm);
    app.paste(Ok(" local".to_owned()));
    service::execute(
        &mut other,
        json!({"op":"note.delete", "id":original}),
        &at(NOW),
        DateOrder::DayFirst,
    )
    .unwrap();
    app.update(Action::Tick);
    let stored = other.load().unwrap();
    assert_eq!(stored, *app.model());
    assert!(!stored.note(original).unwrap().is_live());
    assert_eq!(stored.note(original).unwrap().body, "baseline");
    let recovery = app.draft().unwrap().note;
    assert_ne!(recovery, original);
    assert_eq!(stored.note(recovery).unwrap().body, "baseline local");
}

#[test]
fn a_cli_crlf_note_survives_editor_copy_and_database_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("import.txt");
    let body = "abc\r\ndef\r\nghi\t\u{1b}";
    std::fs::write(&file, body).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_jobsdone"))
        .arg("--data-dir")
        .arg(dir.path())
        .args(["--json", "note", "create", "--file"])
        .arg(&file)
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    let created: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(created["data"]["note"]["body"], body);
    let path = dir.path().join("jobsdone.db");
    let mut app = App::new(
        Box::new(Sqlite::open(&path).unwrap()),
        Box::new(NoDesktop),
        Locale::default(),
        &Zoned::now(),
    )
    .unwrap();
    app.update(Action::NotesPage);
    app.update(Action::Confirm);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 36)).unwrap();
    let mut layout = None;
    terminal
        .draw(|frame| layout = Some(jobsdone::ui::draw(&app, frame)))
        .unwrap();
    app.set_layout(layout.unwrap());
    app.update(Action::LineStart);
    app.update(Action::Insert('X'));
    let expected = "abc\r\ndef\r\nXghi\t\u{1b}";
    assert_eq!(
        app.update(Action::CopyNote),
        jobsdone::app::Flow::CopyNote(expected.into())
    );
    assert_eq!(app.update(Action::Quit), jobsdone::app::Flow::Quit);
    drop(app);
    let stored = Sqlite::open(&path).unwrap().load().unwrap();
    assert_eq!(stored.note(1).unwrap().body, expected);
}
