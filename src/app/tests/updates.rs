use super::*;

use std::collections::VecDeque;

/// An updater a test can question: every look it was asked for, whether
/// each was asked for by a key, how many installs, and what it will say
/// on the next ticks.
#[derive(Clone, Default)]
struct Fake {
    looks: Rc<RefCell<Vec<bool>>>,
    installs: Rc<Cell<usize>>,
    says: Rc<RefCell<VecDeque<Heard>>>,
}

impl Fake {
    fn looks(&self) -> Vec<bool> {
        self.looks.borrow().clone()
    }

    fn say(&self, heard: Heard) {
        self.says.borrow_mut().push_back(heard);
    }
}

impl Updater for Fake {
    fn look(&mut self, asked: bool) {
        self.looks.borrow_mut().push(asked);
    }

    fn install(&mut self) {
        self.installs.set(self.installs.get() + 1);
    }

    fn heard(&mut self) -> Option<Heard> {
        self.says.borrow_mut().pop_front()
    }
}

fn watched(app: &mut App) -> Fake {
    let fake = Fake::default();
    app.watch_releases(Box::new(fake.clone()));
    fake
}

fn release(standing: Standing, installed: &str, available: &str) -> Release {
    Release {
        standing,
        installed: installed.to_owned(),
        available: available.to_owned(),
    }
}

fn newer() -> Heard {
    Heard::Looked(Ok(release(Standing::Newer, "1.4.0", "1.5.0")))
}

fn later(app: &mut App, seconds: i64) {
    app.clock = app
        .clock
        .checked_add(Span::new().seconds(seconds))
        .expect("a later instant");
}

fn turn_the_check_off(app: &mut App) {
    app.update(Action::SettingsPage);
    cursor_to(app, SettingRow::CheckForUpdates);
    app.update(Action::Pick);
    assert!(!app.settings().check_for_updates());
    app.update(Action::Cancel);
}

#[test]
fn without_an_updater_nothing_looks_and_u_says_how_this_copy_is_updated() {
    let mut app = started();
    app.update(Action::Tick);
    app.update(Action::Update);
    assert_eq!(
        hint(&app),
        "This copy was not installed from a release; update it the way it was installed."
    );
    assert_eq!(app.update_notice(), None);
}

#[test]
fn the_window_looks_on_its_first_tick_and_then_once_a_minute() {
    let mut app = started();
    let fake = watched(&mut app);
    assert_eq!(fake.looks(), Vec::<bool>::new(), "nothing at launch");

    app.update(Action::Tick);
    assert_eq!(fake.looks(), [false]);
    fake.say(Heard::Nothing);
    app.update(Action::Tick);
    later(&mut app, 59);
    app.update(Action::Tick);
    assert_eq!(fake.looks(), [false], "not again within the minute");

    later(&mut app, 1);
    app.update(Action::Tick);
    assert_eq!(fake.looks(), [false, false]);
    assert_eq!(hint(&app), "", "a look of the window's own says nothing");
}

#[test]
fn no_second_look_starts_while_one_is_on_its_way() {
    let mut app = started();
    let fake = watched(&mut app);
    app.update(Action::Tick);
    later(&mut app, 120);
    app.update(Action::Tick);
    assert_eq!(fake.looks(), [false]);
}

#[test]
fn with_the_setting_off_the_window_never_looks_but_u_still_does() {
    let mut app = started();
    let fake = watched(&mut app);
    turn_the_check_off(&mut app);

    app.update(Action::Tick);
    later(&mut app, 3600);
    app.update(Action::Tick);
    assert_eq!(fake.looks(), Vec::<bool>::new());

    app.update(Action::Update);
    assert_eq!(fake.looks(), [true]);
    assert_eq!(hint(&app), "Checking for updates…");

    fake.say(newer());
    app.update(Action::Tick);
    assert_eq!(
        hint(&app),
        "Jobsdone 1.5.0 is available. Press U to update."
    );
    assert_eq!(
        app.update_notice(),
        None,
        "no notice while the setting is off"
    );
}

#[test]
fn a_newer_release_puts_up_the_notice_without_a_word() {
    let mut app = started();
    let fake = watched(&mut app);
    app.update(Action::Tick);
    fake.say(newer());
    app.update(Action::Tick);
    assert_eq!(app.update_notice(), Some("1.5.0"));
    assert_eq!(hint(&app), "");

    // The next answer is the one that stands.
    later(&mut app, 60);
    app.update(Action::Tick);
    fake.say(Heard::Looked(Ok(release(Standing::Same, "1.4.0", "1.4.0"))));
    app.update(Action::Tick);
    assert_eq!(app.update_notice(), None);
}

#[test]
fn turning_the_setting_off_takes_the_notice_away() {
    let mut app = started();
    let fake = watched(&mut app);
    app.update(Action::Tick);
    fake.say(newer());
    app.update(Action::Tick);
    assert_eq!(app.update_notice(), Some("1.5.0"));

    turn_the_check_off(&mut app);
    assert_eq!(app.update_notice(), None);
}

#[test]
fn a_look_somebody_asked_for_says_what_it_found() {
    let cases = [
        (
            Heard::Looked(Ok(release(Standing::Same, "1.4.0", "1.4.0"))),
            "Jobsdone 1.4.0 is up to date.",
        ),
        (
            Heard::Looked(Ok(release(Standing::Older, "1.4.0", "1.3.0"))),
            "Jobsdone 1.4.0 is newer than the latest release, 1.3.0.",
        ),
        (
            Heard::Looked(Err(
                "Could not find a stable release. Check your connection and try again.".to_owned(),
            )),
            "Could not check for updates: Could not find a stable release. Check your \
             connection and try again.",
        ),
        (
            Heard::Looked(Err("no answer in 30 seconds".to_owned())),
            "Could not check for updates: no answer in 30 seconds.",
        ),
    ];
    for (heard, said) in cases {
        let mut app = started();
        let fake = watched(&mut app);
        app.update(Action::Update);
        fake.say(heard.clone());
        app.update(Action::Tick);
        assert_eq!(hint(&app), said);

        // The same answer to a look of the window's own is not said.
        let mut quiet = started();
        let fake = watched(&mut quiet);
        quiet.update(Action::Tick);
        fake.say(heard);
        quiet.update(Action::Tick);
        assert_eq!(hint(&quiet), "", "{said}");
    }
}

#[test]
fn u_during_a_look_of_the_windows_own_makes_it_one_that_answers() {
    let mut app = started();
    let fake = watched(&mut app);
    app.update(Action::Tick);
    app.update(Action::Update);
    assert_eq!(fake.looks(), [false], "the look on its way is the one");

    fake.say(Heard::Looked(Ok(release(Standing::Same, "1.4.0", "1.4.0"))));
    app.update(Action::Tick);
    assert_eq!(hint(&app), "Jobsdone 1.4.0 is up to date.");
}

/// An app that has heard of 1.5.0 on a look of its own.
fn told_of_a_release() -> (App, Fake) {
    let mut app = started();
    let fake = watched(&mut app);
    app.update(Action::Tick);
    fake.say(newer());
    app.update(Action::Tick);
    assert_eq!(app.update_notice(), Some("1.5.0"));
    (app, fake)
}

#[test]
fn u_with_a_newer_release_asks_and_escape_keeps_what_is_installed() {
    let (mut app, fake) = told_of_a_release();
    app.update(Action::Update);
    assert_eq!(
        app.key_context(),
        KeyContext::Popup {
            kind: PopupKind::UpdateQuestion,
            text_field: false,
        }
    );

    app.update(Action::Cancel);
    assert!(app.popup().is_none());
    assert_eq!(fake.installs.get(), 0);
    assert_eq!(app.update_notice(), Some("1.5.0"));
    assert_eq!(app.installing(), None);
}

#[test]
fn enter_saves_the_open_note_and_starts_the_install() {
    let (mut app, fake) = told_of_a_release();
    app.update(Action::NotesPage);
    let note = note_saying(&mut app, "half a thought");
    assert_eq!(
        app.model().note(note).unwrap().body,
        "",
        "nothing written yet"
    );

    app.update(Action::Update);
    app.update(Action::Confirm);
    assert_eq!(app.model().note(note).unwrap().body, "half a thought");
    assert_eq!(fake.installs.get(), 1);
    assert_eq!(app.installing(), Some(("1.5.0", 0)));
    assert_eq!(app.key_context(), KeyContext::Updating);
}

#[test]
fn while_it_installs_every_key_but_quit_is_put_down() {
    let (mut app, fake) = told_of_a_release();
    app.update(Action::Update);
    app.update(Action::Confirm);

    for action in [Action::Add, Action::Down, Action::Update, Action::NotesPage] {
        assert_eq!(app.update(action), Flow::Continue);
    }
    app.unbound("x");
    assert!(app.editor().is_none());
    assert_eq!(app.page(), Page::Home);
    assert_eq!(hint(&app), "");
    assert_eq!(fake.installs.get(), 1);

    app.update(Action::Tick);
    app.update(Action::Tick);
    assert_eq!(app.installing(), Some(("1.5.0", 2)), "the spinner turns");
    assert_eq!(fake.looks(), [false], "and nothing looks meanwhile");

    assert_eq!(app.update(Action::Quit), Flow::Quit);
}

#[test]
fn a_finished_install_restarts_on_the_page_the_window_was_on() {
    let (mut app, fake) = told_of_a_release();
    app.update(Action::Update);
    app.update(Action::Confirm);
    fake.say(Heard::Installed(Ok(())));
    assert_eq!(
        app.update(Action::Tick),
        Flow::Restart(Restart { notes: false })
    );

    let (mut app, fake) = told_of_a_release();
    app.update(Action::NotesPage);
    app.update(Action::Update);
    app.update(Action::Confirm);
    fake.say(Heard::Installed(Ok(())));
    assert_eq!(
        app.update(Action::Tick),
        Flow::Restart(Restart { notes: true })
    );
}

#[test]
fn a_failed_install_says_why_and_leaves_the_window_as_it_was() {
    let (mut app, fake) = told_of_a_release();
    let id = add(&mut app, "keep me");
    app.update(Action::Update);
    app.update(Action::Confirm);
    fake.say(Heard::Installed(Err(
        "Download failed. Your installation has not changed.".to_owned(),
    )));
    assert_eq!(app.update(Action::Tick), Flow::Continue);

    assert_eq!(
        hint(&app),
        "Could not update: Download failed. Your installation has not changed."
    );
    assert_eq!(app.installing(), None);
    assert_eq!(
        app.update_notice(),
        Some("1.5.0"),
        "the notice is still there"
    );
    assert_eq!(app.page(), Page::Home);
    assert_eq!(cursor(&app, List::Day), Some(id));
    assert_eq!(titles(&app, List::Day), ["keep me"]);

    // And the key tries again.
    app.update(Action::Update);
    app.update(Action::Confirm);
    assert_eq!(fake.installs.get(), 2);
}

#[test]
fn a_look_that_lands_during_an_install_does_not_end_it() {
    let (mut app, fake) = told_of_a_release();
    later(&mut app, 60);
    app.update(Action::Tick);
    assert_eq!(fake.looks(), [false, false]);
    app.update(Action::Update);
    app.update(Action::Confirm);
    fake.say(newer());
    app.update(Action::Tick);
    assert_eq!(app.installing(), Some(("1.5.0", 1)));
}
