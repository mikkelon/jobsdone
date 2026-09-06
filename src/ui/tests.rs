use super::*;

use std::fs;

use jiff::Zoned;
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::style::Color;
use ratatui::{Terminal, TerminalOptions, Viewport};

use crate::app::{App, Desktop, Locale, WindowSize};
use crate::domain::tests::MemStore;
use crate::domain::{DueChip, FromPlace, Model, Note, Placement, Schedule, Task, Weekday};
use crate::input::Action;

/// The wireframes are the source of truth for the layout, so the test is a
/// character-by-character comparison against them at the three sizes
/// DESIGN.md names.
const WIREFRAMES: &str = "wireframes";

/// The day the wireframes are drawn on, which is a real Friday.
const NOW: &str = "2025-09-05T09:00:00+02:00[Europe/Copenhagen]";

fn at(text: &str) -> Zoned {
    text.parse().expect("a zoned timestamp")
}

fn on(text: &str) -> Date {
    text.parse().expect("a civil date")
}

/// A window manager that takes whatever it is told. Nothing on screen
/// asks it anything.
struct Desk;

impl Desktop for Desk {
    fn available(&self) -> bool {
        true
    }

    fn apply_window(&self, _floating: bool, _size: WindowSize) -> Result<(), String> {
        Ok(())
    }
}

/// The app a drawing test draws, on the store it is given.
fn app_on(store: MemStore) -> App {
    app_when(store, &at(NOW))
}

fn app_when(store: MemStore, now: &Zoned) -> App {
    App::new(Box::new(store), Box::new(Desk), Locale::default(), now).expect("an app")
}

/// An app on an empty database, which is what a first launch looks like.
fn empty() -> App {
    app_on(MemStore::new())
}

/// A task with everything a wireframe row does not say about it left off.
fn task(id: i64, title: &str, day: Option<Date>, position: usize) -> Task {
    Task {
        id,
        title: title.to_owned(),
        day,
        position,
        focus: false,
        waiting: false,
        closed_at: None,
        due_on: None,
        remind_on: None,
        schedule_id: None,
        scheduled_on: None,
        created_at: at(NOW),
        deleted_at: None,
    }
}

/// The model wireframe 03 is a picture of: today's plan, the backlog, two
/// tasks left on past days and four notes.
fn wireframe_model() -> Model {
    let today = on("2025-09-05");
    let monday = on("2025-09-08");
    let mut model = Model::empty();

    let mut put = |task: Task| {
        model.tasks.insert(task.id, task);
    };

    put(Task {
        focus: true,
        schedule_id: Some(1),
        scheduled_on: Some(today),
        ..task(1, "Ship invoice export", Some(today), 0)
    });
    put(Task {
        focus: true,
        ..task(2, "Reply to the tender questions", Some(today), 1)
    });
    put(task(3, "Fix the flaky migration test", Some(today), 2));
    put(Task {
        remind_on: Some(today),
        ..task(4, "Book dentist", Some(today), 3)
    });
    put(Task {
        schedule_id: Some(2),
        scheduled_on: Some(today),
        ..task(5, "Write standup notes", Some(today), 4)
    });
    put(task(6, "Review Anna's PR", Some(today), 5));
    put(Task {
        closed_at: Some(at("2025-09-05T08:12:00+02:00[Europe/Copenhagen]")),
        ..task(7, "Morning review", Some(today), 6)
    });
    put(Task {
        closed_at: Some(at("2025-09-05T08:30:00+02:00[Europe/Copenhagen]")),
        ..task(8, "Pay electricity bill", Some(today), 7)
    });
    // Planned for today, and now on Monday: the Moved group.
    put(task(9, "Chase the hosting invoice", Some(monday), 0));

    let backlog = [
        "Migrate CI to the new runners",
        "Write the Q4 planning doc",
        "Clean out the garage",
        "Renew passport",
        "Try the new keyboard layout",
        "Read the Hyprland plugin docs",
        "Cancel unused subscriptions",
        "Sort photo backups",
        "Update the household budget",
    ];
    for (at, title) in backlog.iter().enumerate() {
        put(task(101 + at as i64, title, None, at));
    }
    for (at, title) in [
        "Quote from the electrician",
        "Feedback on the proposal",
        "Parcel from the supplier",
    ]
    .iter()
    .enumerate()
    {
        put(Task {
            waiting: true,
            ..task(110 + at as i64, title, None, 9 + at)
        });
    }
    put(Task {
        due_on: Some(on("2025-09-12")),
        ..task(101, "Migrate CI to the new runners", None, 0)
    });
    put(Task {
        due_on: Some(on("2025-09-30")),
        ..task(102, "Write the Q4 planning doc", None, 1)
    });
    put(Task {
        remind_on: Some(on("2025-10-01")),
        ..task(104, "Renew passport", None, 3)
    });
    put(Task {
        waiting: true,
        remind_on: Some(on("2025-09-15")),
        ..task(111, "Feedback on the proposal", None, 10)
    });

    // Two tasks left behind on past days: the review count, and nothing
    // else on this screen. The review has run today, so the home page is
    // what the window opens on (DOMAIN.md section 13).
    model
        .meta
        .insert("review_on".to_owned(), "2025-09-05".to_owned());
    put(task(201, "Ring the accountant", Some(on("2025-09-03")), 0));
    put(task(
        202,
        "Send the meter reading",
        Some(on("2025-09-01")),
        0,
    ));

    for (task_id, day, from) in [
        (3, today, FromPlace::Backlog),
        (9, today, FromPlace::New),
        (9, monday, FromPlace::Day(today)),
    ] {
        model.placements.insert(
            (task_id, day),
            Placement {
                task_id,
                day,
                placed_at: at("2025-09-05T08:00:00+02:00[Europe/Copenhagen]"),
                from_place: from,
            },
        );
    }

    for (id, title, rule) in [
        (
            1,
            "Ship invoice export",
            Rule::Weekly {
                weekdays: vec![Weekday::Fri],
            },
        ),
        (2, "Write standup notes", Rule::Workdays),
    ] {
        model.schedules.insert(
            id,
            Schedule {
                id,
                title: title.to_owned(),
                rule,
                generated_through: today,
                stopped_on: None,
                created_at: at(NOW),
            },
        );
    }

    for (id, made, body) in [
        (1, "2025-09-04T16:40:00", NOTE),
        (
            2,
            "2025-09-03T11:20:00",
            "Draft reply to tender Q3: \"We can",
        ),
        (3, "2025-09-03T09:05:00", "nordic ltd PO 4471, due 30 days"),
        (
            4,
            "2025-08-29T14:00:00",
            "rsync -av --delete ~/work nas:/bk",
        ),
    ] {
        let made = at(&format!("{made}+02:00[Europe/Copenhagen]"));
        model.notes.insert(
            id,
            Note {
                id,
                body: body.to_owned(),
                created_at: made.clone(),
                updated_at: made,
                deleted_at: None,
            },
        );
    }
    model
}

/// The note wireframe 10 has open, which is also the first row of its
/// list: several lines, a blank one among them.
const NOTE: &str = "Mention to Anna:
- CI runner budget
- Friday demo slot
- ask about the retro format

Also: the tender deadline moved to the 12th, check with legal first.

Draft:
Hi Anna, two things before Friday. The CI runner budget needs a decision
this week, and I would like the demo slot after lunch rather than before.";

/// The model wireframe 08 is a picture of: a Monday with two tasks still
/// open on it, three closed and three moved away, and the other days the
/// list counts.
fn history_model() -> Model {
    let monday = on("2025-09-01");
    let mut model = Model::empty();
    let mut put = |task: Task| {
        model.tasks.insert(task.id, task);
    };

    put(Task {
        schedule_id: Some(1),
        scheduled_on: Some(monday),
        ..task(2, "Write standup notes", Some(monday), 1)
    });
    put(task(1, "Call the accountant about VAT", Some(monday), 0));
    for (id, title, at_time, focus) in [
        (3, "Weekly planning", "09:05", false),
        (4, "Send the contract draft", "11:20", true),
        (5, "Reply to Anna", "15:48", false),
    ] {
        put(Task {
            focus,
            closed_at: Some(at(&format!(
                "2025-09-01T{at_time}:00+02:00[Europe/Copenhagen]"
            ))),
            ..task(id, title, Some(monday), id as usize)
        });
    }
    // The three that left the day, still pointed at from it.
    put(task(
        6,
        "Prepare slides for Monday",
        Some(on("2025-09-05")),
        0,
    ));
    // Moved to the Wednesday and closed two days later, so the day it is
    // on and the day it was closed are not the same.
    put(Task {
        closed_at: Some(at("2025-09-05T14:00:00+02:00[Europe/Copenhagen]")),
        ..task(7, "Book the venue", Some(on("2025-09-03")), 0)
    });
    put(task(8, "Order new office chair", None, 0));

    for (task_id, day, minute) in [
        (1, monday, 0),
        (2, monday, 0),
        (3, monday, 0),
        (4, monday, 0),
        (5, monday, 0),
        (6, monday, 0),
        (7, monday, 1),
        (8, monday, 2),
        (6, on("2025-09-05"), 0),
        (7, on("2025-09-03"), 0),
    ] {
        model.placements.insert(
            (task_id, day),
            Placement {
                task_id,
                day,
                placed_at: at(&format!("{day}T08:0{minute}:00+02:00[Europe/Copenhagen]")),
                from_place: FromPlace::New,
            },
        );
    }

    model.schedules.insert(
        1,
        Schedule {
            id: 1,
            title: "Write standup notes".to_owned(),
            rule: Rule::Workdays,
            generated_through: on("2025-09-05"),
            stopped_on: None,
            created_at: at(NOW),
        },
    );

    // Every other day of the list, as many rows as its counts say. Only
    // the counts are drawn, so the rows need nothing but a day.
    let mut id = 100;
    for (day, done, open) in [
        ("2025-09-05", 2, 5),
        ("2025-09-04", 4, 1),
        ("2025-09-03", 5, 0),
        ("2025-09-02", 3, 0),
        ("2025-08-29", 5, 0),
        ("2025-08-28", 4, 0),
        ("2025-08-27", 2, 0),
        ("2025-08-26", 7, 0),
        ("2025-08-25", 3, 1),
        ("2025-08-22", 1, 1),
    ] {
        let day = on(day);
        for at_position in 0..done + open {
            id += 1;
            let closed = at_position < done;
            model.tasks.insert(
                id,
                Task {
                    closed_at: closed
                        .then(|| at(&format!("{day}T12:00:00+02:00[Europe/Copenhagen]"))),
                    ..task(id, &format!("Something on {day}"), Some(day), at_position)
                },
            );
            model.placements.insert(
                (id, day),
                Placement {
                    task_id: id,
                    day,
                    placed_at: at(&format!("{day}T08:00:00+02:00[Europe/Copenhagen]")),
                    from_place: FromPlace::New,
                },
            );
        }
    }

    for id in 1..=4 {
        let made = at("2025-09-04T16:40:00+02:00[Europe/Copenhagen]");
        model.notes.insert(
            id,
            Note {
                id,
                body: "A note".to_owned(),
                created_at: made.clone(),
                updated_at: made,
                deleted_at: None,
            },
        );
    }
    model
}

/// The same window, stepped back to the Monday.
fn history() -> App {
    // The review has run this morning, so the window opens on the page
    // that history is browsed from (DOMAIN.md section 13).
    let mut model = history_model();
    model
        .meta
        .insert("review_on".to_owned(), "2025-09-05".to_owned());
    let mut app = app_on(MemStore::holding(model));
    for _ in 0..4 {
        app.update(Action::PrevDay);
    }
    assert_eq!(app.showing(), on("2025-09-01"));
    app
}

fn app() -> App {
    app_on(MemStore::holding(wireframe_model()))
}

/// A window holding the rows that carry the most: a task that left today
/// for the backlog while waiting, due, reminded and repeating, so its
/// pointer in the Moved group and its backlog row both run out of line,
/// and a closed task that was focus, so the Done group carries a word and
/// a time as well.
fn crowded_model() -> Model {
    let today = on("2025-09-05");
    let mut model = Model::empty();
    model.schedules.insert(
        1,
        Schedule {
            id: 1,
            title: "Task".to_owned(),
            rule: Rule::Workdays,
            generated_through: today,
            stopped_on: None,
            created_at: at(NOW),
        },
    );
    model.tasks.insert(
        1,
        Task {
            waiting: true,
            due_on: Some(on("2025-09-03")),
            remind_on: Some(on("2025-09-06")),
            schedule_id: Some(1),
            scheduled_on: Some(today),
            ..task(1, "Task", None, 0)
        },
    );
    model.tasks.insert(
        2,
        Task {
            focus: true,
            closed_at: Some(at("2025-09-05T09:05:00+02:00[Europe/Copenhagen]")),
            ..task(2, "Send the contract draft", Some(today), 0)
        },
    );
    model.placements.insert(
        (1, today),
        Placement {
            task_id: 1,
            day: today,
            placed_at: at(NOW),
            from_place: FromPlace::New,
        },
    );
    model.placements.insert(
        (2, today),
        Placement {
            task_id: 2,
            day: today,
            placed_at: at(NOW),
            from_place: FromPlace::Backlog,
        },
    );
    model
}

fn crowded() -> App {
    let mut app = app_on(MemStore::holding(crowded_model()));
    // The repeat starting today opens the review; the rows this window is
    // for are the ones behind it.
    app.update(Action::Cancel);
    app
}

/// A row carrying every chip at once, more than any one task can really
/// be: waiting and on a past day are exclusive, but the drawing may not
/// depend on that.
/// A day holding one task whose title is longer than any box that draws
/// it, which is what search had no width for (F15). Search with nothing
/// typed matches it, so the sweep draws it in the box at every size.
fn long_titled() -> App {
    let mut model = Model::empty();
    model.tasks.insert(
        1,
        task(1, &"0123456789".repeat(8), Some(on("2025-09-05")), 0),
    );
    app_on(MemStore::holding(model))
}

fn every_chip() -> domain::Row {
    domain::Row {
        task: 1,
        title: "Write the Q4 planning doc".to_owned(),
        place: Place::Backlog,
        closed_at: Some(at("2025-09-05T09:05:00+02:00[Europe/Copenhagen]")),
        focus: true,
        waiting: true,
        due: Some(DueChip {
            on: on("2025-08-30"),
            overdue: true,
        }),
        remind: Some(on("2025-09-12")),
        repeat: Some(Rule::Weekly {
            weekdays: vec![Weekday::Mon, Weekday::Thu],
        }),
        was_focus: true,
        on_the_pile: true,
        from_backlog: true,
        closed_on_this_day: true,
    }
}

/// Draws one row on a canvas of its own, which is how a width the panes
/// never hand a row is still put to it.
fn one_row(width: u16, row: &domain::Row, look: Look) -> String {
    let area = Rect::new(0, 0, width, 1);
    let mut buffer = Buffer::empty(area);
    let mut canvas = Canvas {
        buffer: &mut buffer,
        area,
    };
    task_row(
        &mut canvas,
        Column {
            x: 0,
            width,
            top: 0,
            bottom: 0,
        },
        0,
        row,
        look,
    );
    lines(&buffer).remove(0)
}

/// Renders and answers the screen as lines, trailing blanks trimmed the
/// way the wireframe files trim them, and the layout `draw` reported.
fn screen(app: &App, width: u16, height: u16) -> (Vec<String>, Layout) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    let mut layout = Layout::default();
    terminal
        .draw(|frame| layout = draw(app, frame))
        .expect("a frame");
    (lines(terminal.backend().buffer()), layout)
}

/// The same, for a test that only looks at what was drawn.
fn look(app: &App, width: u16, height: u16) -> Vec<String> {
    screen(app, width, height).0
}

/// The screen with the cell a double-width character owns beside it
/// folded back into the character, so a row reads as the text it is. A
/// character drawn a cell narrower than it is loses its neighbour here.
fn glyphs(app: &App, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    terminal
        .draw(|frame| _ = draw(app, frame))
        .expect("a frame");
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            let mut row = String::new();
            let mut x = 0;
            while x < width {
                let symbol = buffer[(x, y)].symbol();
                row.push_str(symbol);
                x += count(symbol).max(1);
            }
            row.trim_end().to_owned()
        })
        .collect()
}

fn lines(buffer: &Buffer) -> Vec<String> {
    let area = buffer.area();
    (0..area.height)
        .map(|y| {
            let row: String = (0..area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<Vec<_>>()
                .join("");
            row.trim_end().to_owned()
        })
        .collect()
}

/// One screen out of a wireframe file. The blocks are `== label`, a blank
/// line, then exactly `height` rows.
fn wireframe(name: &str, block: usize, height: usize) -> Vec<String> {
    let path = format!("{WIREFRAMES}/{name}.txt");
    let text = fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
    let all: Vec<&str> = text.lines().collect();
    let heads: Vec<usize> = all
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("== "))
        .map(|(at, _)| at)
        .collect();
    let start = heads[block] + 2;
    all[start..start + height]
        .iter()
        .map(|line| line.trim_end().to_owned())
        .collect()
}

fn same(drawn: &[String], wanted: &[String], what: &str) {
    let mut wrong = Vec::new();
    for (at, (drawn, wanted)) in drawn.iter().zip(wanted).enumerate() {
        if drawn != wanted {
            wrong.push(format!(
                "row {at}\n  drawn:  {drawn:?}\n  wanted: {wanted:?}"
            ));
        }
    }
    assert!(
        wrong.is_empty() && drawn.len() == wanted.len(),
        "{what} does not match its wireframe:\n{}",
        wrong.join("\n")
    );
}

/// The model wireframes 01 and 02 are a picture of: seven unfinished
/// tasks on four past days, and the four dated backlog tasks and two
/// copies that surface on the morning of Friday 5 September.
fn review_model() -> Model {
    let today = on("2025-09-05");
    let mut model = Model::empty();
    let mut put = |task: Task| {
        model.tasks.insert(task.id, task);
    };

    // The pile, newest day first, in position order within each day.
    put(task(
        1,
        "Send the invoice to Nordic Ltd",
        Some(on("2025-09-04")),
        0,
    ));
    put(task(
        2,
        "Prepare slides for Monday",
        Some(on("2025-09-04")),
        1,
    ));
    put(Task {
        focus: true,
        ..task(3, "Fix the flaky migration test", Some(on("2025-09-04")), 2)
    });
    put(task(
        4,
        "Call the accountant about VAT",
        Some(on("2025-09-01")),
        0,
    ));
    put(Task {
        schedule_id: Some(2),
        scheduled_on: Some(on("2025-09-01")),
        ..task(5, "Write standup notes", Some(on("2025-09-01")), 1)
    });
    put(task(6, "Order new office chair", Some(on("2025-08-22")), 0));
    put(task(7, "Book the team dinner", Some(on("2025-08-12")), 0));

    // What surfaces today: two due, two reminders, and the two copies
    // the schedules made this morning.
    put(Task {
        due_on: Some(on("2025-09-03")),
        ..task(11, "Migrate CI to the new runners", None, 0)
    });
    put(Task {
        due_on: Some(today),
        ..task(12, "Submit the expense report", None, 1)
    });
    put(Task {
        remind_on: Some(today),
        ..task(13, "Book dentist", None, 2)
    });
    put(Task {
        waiting: true,
        remind_on: Some(today),
        ..task(14, "Feedback on the proposal", None, 3)
    });
    put(Task {
        schedule_id: Some(1),
        scheduled_on: Some(today),
        ..task(15, "Ship invoice export", Some(today), 0)
    });
    put(Task {
        schedule_id: Some(2),
        scheduled_on: Some(today),
        ..task(16, "Write standup notes", Some(today), 1)
    });

    for (id, title, rule) in [
        (
            1,
            "Ship invoice export",
            Rule::Weekly {
                weekdays: vec![Weekday::Fri],
            },
        ),
        (2, "Write standup notes", Rule::Workdays),
    ] {
        model.schedules.insert(
            id,
            Schedule {
                id,
                title: title.to_owned(),
                // Generated through today, so the launch owes no copies.
                rule,
                generated_through: today,
                stopped_on: None,
                created_at: at(NOW),
            },
        );
    }
    model
}

/// The review as wireframe 01 draws it: opened by the launch, with the
/// first row closed and the second moved onto today.
fn reviewing() -> App {
    let mut app = app_on(MemStore::holding(review_model()));
    app.update(Action::Close);
    app.update(Action::ToToday);
    // What just happened stands in the hint bar until the next key, and
    // the wireframe is drawn a key later.
    app.update(Action::Up);
    app.update(Action::Down);
    app
}

#[test]
fn the_pile_matches_the_wireframe() {
    let app = reviewing();
    same(
        &look(&app, 120, 36),
        &wireframe("01-review", 0, 36),
        "the review's first step at 120x36",
    );
}

#[test]
fn the_surfaced_step_matches_the_wireframe() {
    let mut app = reviewing();
    app.update(Action::Confirm);
    same(
        &look(&app, 120, 36),
        &wireframe("02-surfaced", 0, 36),
        "the review's second step at 120x36",
    );
}

#[test]
fn the_floating_window_matches_the_wireframe() {
    let drawn = look(&app(), 120, 36);
    same(
        &drawn,
        &wireframe("03-today", 0, 36),
        "the home page at 120x36",
    );
}

#[test]
fn the_full_width_tile_matches_the_wireframe() {
    let drawn = look(&app(), 160, 48);
    same(
        &drawn,
        &wireframe("03-today", 1, 48),
        "the home page at 160x48",
    );
}

#[test]
fn the_half_width_tile_collapses_to_tabs() {
    let drawn = look(&app(), 80, 44);
    same(
        &drawn,
        &wireframe("04-narrow", 0, 44),
        "the home page at 80x44",
    );
}

#[test]
fn the_past_day_matches_the_wireframe() {
    let drawn = look(&history(), 120, 36);
    same(
        &drawn,
        &wireframe("08-history", 0, 36),
        "a past day at 120x36",
    );
}

#[test]
fn a_past_day_with_nothing_on_it_names_the_keys_that_leave_it() {
    let mut app = history();
    // A Sunday nobody planned anything for (wireframe 12 panel D).
    for _ in 0..1 {
        app.update(Action::PrevDay);
    }
    let text = look(&app, 120, 36).join("\n");

    assert!(text.contains("Sun 31 Aug past day"));
    assert!(
        text.contains("nothing was planned"),
        "and the header says so"
    );
    assert!(text.contains("Nothing was planned on this day."));
    assert!(text.contains("[ keeps stepping back · g pick a date"));
    assert!(
        text.contains("Sun 31 Aug"),
        "the day list still skips it, but the pane is there"
    );
}

#[test]
fn the_narrow_window_makes_the_day_and_the_day_list_its_tabs() {
    let drawn = look(&history(), 80, 44);
    let tabs = drawn[3].clone();

    assert!(tabs.contains("MON 1 SEP"), "the tab is the day, not TODAY");
    assert!(tabs.contains("DAYS 11"), "and the backlog gives way to it");
    assert_eq!(
        drawn[drawn.len() - 2],
        " PAST DAY  [ ] day  . back  space close  t today  x del                  ? more",
        "and the bar keeps the way home in it"
    );
}

#[test]
fn a_hint_bar_too_full_for_its_right_end_leaves_it_out() {
    let mut app = history();
    // A day ahead: "FUTURE DAY" is the longest name a context has, and
    // its bar is the longest too.
    for _ in 0..8 {
        app.update(Action::NextDay);
    }
    let drawn = look(&app, 120, 36);
    let bar = drawn[34].clone();

    assert!(bar.starts_with(" FUTURE DAY  [ ] day  . today"));
    assert!(
        !bar.contains("pane"),
        "the row that does not fit is left out, not written over"
    );
    assert!(bar.len() <= 120, "and nothing runs past the window");
}

#[test]
fn the_day_list_keeps_its_last_word_under_its_last_day() {
    let mut app = history();
    app.update(Action::PaneRight);
    for _ in 0..11 {
        app.update(Action::Down);
    }
    // A window too short for the whole list, so it has scrolled.
    let drawn = look(&app, 120, 20);
    let body: Vec<&String> = drawn[5..17].iter().collect();

    assert!(
        body.last()
            .is_some_and(|row| row.contains("days with nothing planned are skipped")),
        "drawn:\n{}",
        drawn.join("\n")
    );
}

#[test]
fn the_frame_keeps_its_one_cell_margin() {
    let drawn = look(&app(), 120, 36);

    assert_eq!(drawn[0], "", "a blank row above the status line");
    assert_eq!(drawn[35], "", "a blank row below the hint bar");
    for (at, row) in drawn.iter().enumerate() {
        let rule = row.starts_with('─');
        assert!(
            rule || row.is_empty() || row.starts_with(' '),
            "row {at} touches the left border: {row:?}"
        );
    }
}

#[test]
fn draw_answers_where_every_row_was_put() {
    let (_, layout) = screen(&app(), 120, 36);

    assert!(!layout.narrow);
    assert_eq!(layout.lists.len(), 2, "both panes were drawn");

    // The day pane, then the backlog, in the order they were drawn.
    let day: Vec<_> = layout
        .rows
        .iter()
        .filter(|row| row.list == List::Day)
        .collect();
    assert_eq!(day.len(), 9, "every row of the day pane");
    assert_eq!(
        day[0].area.y, 6,
        "under the header rule and its group label"
    );
    assert_eq!(day[0].area.x, 0);
    assert_eq!(day[0].area.width, 59, "the left pane stops at the divider");

    let backlog: Vec<_> = layout
        .rows
        .iter()
        .filter(|row| row.list == List::Backlog)
        .collect();
    assert_eq!(backlog.len(), 14, "twelve tasks and the two schedules");
    assert_eq!(backlog[0].area.x, 60, "the right pane starts after it");
}

#[test]
fn a_click_moves_the_cursor_to_the_row_it_landed_on() {
    let mut app = app();
    let (_, layout) = screen(&app, 120, 36);
    app.set_layout(layout);

    let wanted = app.layout().rows[3];
    app.update(Action::MouseDown {
        column: wanted.area.x + 4,
        row: wanted.area.y,
    });

    assert_eq!(app.focused(), wanted.list);
    assert_eq!(app.cursor(wanted.list), Some(wanted.id));
}

#[test]
fn the_narrow_window_gives_the_one_pane_the_width() {
    let (_, layout) = screen(&app(), 80, 44);

    assert!(layout.narrow);
    assert_eq!(layout.lists.len(), 1, "one pane, and tabs for the others");
    assert_eq!(layout.lists[0].area.width, 80);
}

#[test]
fn every_colour_is_one_the_terminal_themes() {
    // DESIGN.md section 3: the default foreground and background and the
    // eight ANSI colours, and bright black where the terminal has no dim.
    // Nothing else, so an Omarchy theme change is followed with no code.
    const ALLOWED: &[Color] = &[
        Color::Reset,
        Color::Black,
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
        Color::Gray,
        Color::DarkGray,
    ];

    let mut review = reviewing();
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).expect("a test terminal");
    for action in [Action::Tick, Action::Confirm, Action::Cancel] {
        review.update(action);
        terminal
            .draw(|frame| {
                draw(&review, frame);
            })
            .expect("a frame");
        for cell in terminal.backend().buffer().content() {
            assert!(
                ALLOWED.contains(&cell.fg),
                "{:?} is not a theme colour",
                cell.fg
            );
            assert!(
                ALLOWED.contains(&cell.bg),
                "{:?} is not a theme colour",
                cell.bg
            );
        }
    }

    let mut app = app();
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).expect("a test terminal");
    for action in [
        Action::Tick,
        Action::Commands,
        Action::Help,
        Action::Search,
        Action::MoveToDay,
        Action::DueBy,
        Action::Repeat,
        Action::Add,
    ] {
        app.update(action);
        terminal
            .draw(|frame| {
                draw(&app, frame);
            })
            .expect("a frame");
        for cell in terminal.backend().buffer().content() {
            assert!(
                ALLOWED.contains(&cell.fg) && ALLOWED.contains(&cell.bg),
                "{:?} on {:?} is not a colour the terminal themes",
                cell.fg,
                cell.bg
            );
        }
        app.update(Action::Cancel);
    }
}

#[test]
fn the_cursor_row_is_reversed_and_keeps_its_colours() {
    let mut app = app();
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).expect("a test terminal");
    terminal
        .draw(|frame| {
            draw(&app, frame);
        })
        .expect("a frame");

    // Row 5 is the FOCUS label; the first task is under it.
    let buffer = terminal.backend().buffer();
    let on = buffer[(1, 6)].modifier.contains(Modifier::REVERSED);
    assert!(on, "the cursor starts on the first row of the day pane");
    let off = buffer[(1, 7)].modifier.contains(Modifier::REVERSED);
    assert!(!off, "and on no other");

    app.update(Action::Down);
    terminal
        .draw(|frame| {
            draw(&app, frame);
        })
        .expect("a frame");
    let buffer = terminal.backend().buffer();
    assert!(buffer[(1, 7)].modifier.contains(Modifier::REVERSED));
}

#[test]
fn a_pile_left_behind_is_counted_in_red() {
    // Escape leaves the review, and the home screen counts what is left
    // of the pile until it is dealt with (DESIGN.md section 1).
    let mut app = reviewing();
    app.update(Action::Cancel);
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).expect("a test terminal");
    terminal
        .draw(|frame| {
            draw(&app, frame);
        })
        .expect("a frame");

    let text = look(&app, 120, 36).join("\n");
    assert!(
        text.contains("● 5 in review"),
        "two of the seven were dealt with"
    );
    let buffer = terminal.backend().buffer();
    let at = look(&app, 120, 36)[1].find('●').expect("the count") as u16;
    assert_eq!(buffer[(at, 1)].fg, Color::Red);
}

#[test]
fn the_palette_lists_the_commands_of_the_page_beneath_it() {
    let mut app = app();
    app.update(Action::Commands);
    let drawn = look(&app, 120, 36);
    let text = drawn.join("\n");

    assert!(
        text.contains("COMMANDS"),
        "the group label names the context"
    );
    assert!(text.contains("Move to day…"), "a command, capitalised");
    assert!(text.contains("⏎ run"), "and the footer");
    assert!(drawn[35].is_empty(), "the popup stays inside the margin");
}

#[test]
fn the_help_overlay_is_the_whole_key_map() {
    let mut app = app();
    app.update(Action::Help);
    let text = look(&app, 120, 36).join("\n");

    assert!(text.contains(" Keys "));
    assert!(text.contains("? or esc close"));
    for column in ["EVERYWHERE", "DAY", "BACKLOG", "REVIEW", "NOTES"] {
        assert!(text.contains(column), "the {column} column");
    }
    assert!(text.contains("notes page"), "a key only the help shows");
    assert!(text.contains("ctrl-c"), "the key that is a row of no table");
}

#[test]
fn the_palette_says_what_undo_would_take_back() {
    let mut app = app();
    app.update(Action::Delete);
    app.update(Action::Commands);
    let text = look(&app, 120, 36).join("\n");

    assert!(
        text.contains("Undo: Deleted \"Ship invoice export\""),
        "{text}"
    );
}

#[test]
fn the_date_card_names_its_shapes_and_says_when_the_line_is_not_one() {
    let mut app = app();
    app.update(Action::DueBy);
    let text = look(&app, 120, 36).join("\n");
    assert!(
        text.contains("e.g. 12 sep, +3, -3, mon, tomorrow"),
        "{text}"
    );

    for typed in "banana".chars() {
        app.update(Action::Insert(typed));
    }
    let text = look(&app, 120, 36).join("\n");
    assert!(text.contains("banana"), "{text}");
    assert!(text.contains("not a date"), "{text}");
    assert!(
        !text.contains("e.g. 12 sep"),
        "the shapes go once something is typed: {text}"
    );
}

#[test]
fn the_review_opens_the_help_overlay_too() {
    let mut app = reviewing();
    app.update(Action::Help);
    let text = look(&app, 120, 36).join("\n");

    assert!(text.contains(" Keys "));
    app.update(Action::Cancel);
    assert!(app.review().is_some(), "closing the help keeps the review");
}

#[test]
fn search_says_when_nothing_matches_and_offers_to_add_it() {
    let mut app = app();
    app.update(Action::Search);
    for typed in "tax return".chars() {
        app.update(Action::Insert(typed));
    }
    let text = look(&app, 120, 36).join("\n");

    assert!(text.contains("0 matches"));
    assert!(text.contains("Nothing matches, open or closed."));
    assert!(text.contains("add \"tax return\" to today"));
}

#[test]
fn a_closed_result_names_the_day_enter_would_go_to() {
    let mut app = history();
    app.update(Action::Search);
    for typed in "venue".chars() {
        app.update(Action::Insert(typed));
    }
    let drawn = look(&app, 120, 36);
    let found = drawn
        .iter()
        .find(|row| row.contains("[x] Book the venue"))
        .expect("the result");

    assert!(
        found.contains("Wed 3 Sep") && !found.contains("Fri 5 Sep"),
        "the day it is on, not the Friday it was closed on: {found:?}"
    );
}

#[test]
fn search_groups_what_it_finds_and_says_where_each_one_is() {
    let mut app = app();
    app.update(Action::Search);
    for typed in "invoice".chars() {
        app.update(Action::Insert(typed));
    }
    let text = look(&app, 120, 36).join("\n");

    assert!(text.contains("2 matches"));
    assert!(text.contains("OPEN"));
    assert!(text.contains("Ship invoice export"));
    assert!(text.contains("today · focus · ↻"));
    assert!(text.contains("Chase the hosting invoice"));
    assert!(text.contains("Mon 8 Sep"));
}

#[test]
fn an_empty_list_names_the_keys_that_fill_it() {
    let text = look(&empty(), 120, 36).join("\n");

    assert!(text.contains("Nothing planned."));
    assert!(text.contains("a add a task · l then t pull from the backlog"));
    assert!(text.contains("Backlog is empty."));
    assert!(text.contains("a add · b on a day task sends it here"));
    assert!(text.contains("nothing planned"), "and the header says so");
    assert!(
        text.contains("● 0 in review"),
        "a zero is a count, not an alert"
    );
}

#[test]
fn the_add_line_becomes_the_field_that_is_typed_into() {
    let mut app = empty();
    app.update(Action::Add);
    for typed in "Call the landlord about the leak".chars() {
        app.update(Action::Insert(typed));
    }
    let drawn = look(&app, 120, 36);

    // Nothing is planned yet, so no group label stands over the field:
    // the add line is the pane's own (DESIGN.md section 6).
    assert!(
        drawn[5].contains("+  Call the landlord about the leak▏"),
        "the field is where the add line was: {:?}",
        drawn[5]
    );
    assert!(
        drawn[34].contains("⏎ add & keep typing"),
        "and the hint bar says what Enter does: {:?}",
        drawn[34]
    );
}

#[test]
fn a_title_is_edited_in_the_row_it_belongs_to() {
    let mut app = app();
    app.update(Action::Edit);
    let drawn = look(&app, 120, 36);

    assert!(drawn[6].contains("[ ] Ship invoice export▏"));
    assert!(drawn[34].contains("⏎ save"));
    assert!(drawn[34].contains("esc cancel"));
}

#[test]
fn the_hint_bar_says_what_just_happened_and_offers_to_undo_it() {
    let mut app = app();
    app.update(Action::Delete);
    let drawn = look(&app, 120, 36);

    let bar = drawn[34].trim_end();
    assert!(
        bar.starts_with(" TODAY  Deleted \"Ship invoice export\"  u  undo  "),
        "{bar:?}"
    );
    assert!(
        bar.contains("space done"),
        "the keys that fit follow: {bar:?}"
    );
    assert!(
        !bar.contains("tab h/l pane"),
        "the right end gives way first: {bar:?}"
    );
}

#[test]
fn a_field_keeps_its_keys_beside_what_just_happened() {
    let mut app = app();
    app.update(Action::Add);
    for typed in "Task".chars() {
        app.update(Action::Insert(typed));
    }
    app.update(Action::Confirm);
    let bar = look(&app, 120, 36)[34].trim_end().to_owned();

    assert!(bar.contains("Added \"Task\""), "{bar:?}");
    assert!(bar.contains("add & keep typing"), "{bar:?}");
    assert!(!bar.contains(" u  undo"), "u types in the field: {bar:?}");
}

#[test]
fn the_move_card_names_the_task_and_the_days() {
    let mut app = app();
    app.update(Action::MoveToDay);
    let text = look(&app, 120, 36).join("\n");

    assert!(text.contains("Move Ship invoice export"));
    assert!(text.contains("Today"));
    assert!(text.contains("Fri 5 Sep"));
    assert!(text.contains("Next work day"));
    assert!(text.contains("Mon 8 Sep"));
    assert!(text.contains("Backlog"));
    assert!(text.contains("no day"));
}

#[test]
fn a_row_with_more_chips_than_room_loses_the_end_of_its_title() {
    let now = at(NOW);
    let mut model = Model::empty();
    model.tasks.insert(
        1,
        Task {
            due_on: Some(on("2025-09-30")),
            remind_on: Some(on("2025-09-12")),
            schedule_id: Some(1),
            scheduled_on: Some(on("2025-09-05")),
            ..task(1, "Write the Q4 planning doc and the one after it", None, 0)
        },
    );
    model.schedules.insert(
        1,
        Schedule {
            id: 1,
            title: "Write the Q4 planning doc".to_owned(),
            rule: Rule::Workdays,
            generated_through: on("2025-09-05"),
            stopped_on: None,
            created_at: now.clone(),
        },
    );
    let app = app_when(MemStore::holding(model), &now);

    let row = look(&app, 120, 36)
        .into_iter()
        .find(|row| row.contains("[due 30 Sep]"))
        .expect("the backlog row");

    assert!(
        row.contains("[↻ work days]  [due 30 Sep]  [◷ 12 Sep]"),
        "every chip is drawn: {row:?}"
    );
    assert!(
        !row.contains("doc[↻"),
        "and the title stops before them: {row:?}"
    );
}

#[test]
fn the_backlog_lists_the_schedules_under_its_groups() {
    let drawn = look(&app(), 120, 36);
    let text = drawn.join("\n");

    assert!(text.contains("REPEATING 2"), "a group of its own");
    assert!(
        drawn
            .iter()
            .any(|row| row.contains(" ↻  Ship invoice export") && row.ends_with("every Friday")),
        "each schedule by title and rule"
    );
    assert!(
        drawn
            .iter()
            .any(|row| row.contains(" ↻  Write standup notes") && row.ends_with("every work day"))
    );
}

#[test]
fn the_date_card_types_picks_and_walks_the_month() {
    let mut app = app();
    app.update(Action::PaneRight);
    app.update(Action::Down);
    app.update(Action::DueBy);
    for typed in "30 sep".chars() {
        app.update(Action::Insert(typed));
    }
    let drawn = look(&app, 120, 36);
    let text = drawn.join("\n");

    assert!(text.contains("Due by Write the Q4 planning doc"));
    assert!(text.contains("alt-r remind on"), "the card's other mode");
    assert!(text.contains("Tue 30 Sep"), "what the typed date reads as");
    assert!(text.contains("alt-3"));
    assert!(text.contains("In a week"));
    assert!(text.contains("no due date"), "what clearing answers");
    assert!(text.contains("September 2025"));
    assert!(text.contains(" Mo Tu We Th Fr Sa Su"));
    assert!(text.contains("⏎ set"));
    assert!(text.contains("tab calendar"));
}

#[test]
fn the_date_cards_calendar_takes_the_keyboard_on_tab() {
    let mut app = app();
    app.update(Action::PaneRight);
    app.update(Action::DueBy);
    app.update(Action::NextPane);
    app.update(Action::Right);
    let drawn = look(&app, 120, 36);
    let text = drawn.join("\n");

    assert!(text.contains("h/l/j/k day"), "the calendar's own keys");
    assert!(text.contains("</> month"));
    assert!(text.contains("tab type it"), "and the way back");
    assert!(
        drawn
            .iter()
            .any(|row| row.contains('▏') && row.contains("Sat 13 Sep")),
        "the card opened on the due date the task has and l walked a day on"
    );
}

#[test]
fn the_repeat_card_shows_the_shapes_and_the_dates_they_fall_on() {
    let mut app = app();
    app.update(Action::Repeat);
    app.update(Action::EveryWeek);
    let text = look(&app, 120, 36).join("\n");

    assert!(text.contains("Repeat Ship invoice export"));
    assert!(text.contains("Every work day"));
    assert!(text.contains("Mon–Fri"));
    assert!(text.contains("Every week on"));
    assert!(
        text.contains(" Mo  Tu  We  Th [Fr] Sa  Su "),
        "the weekdays, the ones in the set bracketed"
    );
    assert!(text.contains("Stop repeating"));
    assert!(text.contains("copies stay"));
    assert!(text.contains("Next: Fri 12 Sep · Fri 19 Sep · Fri 26 Sep"));
    assert!(text.contains("⏎ save"));
}

#[test]
fn the_copy_question_spells_both_answers_out() {
    let mut app = app();
    app.update(Action::Edit);
    app.update(Action::Insert('!'));
    app.update(Action::Confirm);
    let text = look(&app, 120, 36).join("\n");

    assert!(text.contains("Rename Ship invoice export"));
    assert!(text.contains("This task repeats. Rename:"));
    assert!(text.contains("This copy"));
    assert!(text.contains("This and future copies"));
}

#[test]
fn the_notes_list_is_newest_first_with_the_age_of_each_note() {
    let mut app = app();
    app.update(Action::NotesPage);
    let drawn = look(&app, 120, 36);
    let wanted = wireframe("10-scratchpad", 0, 36);

    assert!(drawn[1].contains("Notes 4 notes"));
    assert!(drawn[1].contains("n or esc back to today"));
    let divider = drawn[4].chars().position(|glyph| glyph == '┬');
    assert_eq!(
        divider,
        Some(44),
        "the list is a column, not half the window"
    );

    // The list, its ages and the row that makes another note, against the
    // wireframe's own column. Its first row is the one note whose body the
    // wireframe draws in full beside it, so only its text differs.
    assert_eq!(
        left(&drawn[5]),
        "  ▪ Mention to Anna:              yesterday"
    );
    for row in [6, 7, 8, 9, 10] {
        assert_eq!(left(&drawn[row]), left(&wanted[row]), "row {row}");
    }
}

#[test]
fn the_open_note_is_a_text_area_beside_the_list() {
    let mut app = app();
    app.update(Action::NotesPage);
    app.update(Action::Confirm);
    let drawn = look(&app, 120, 36);
    let wanted = wireframe("10-scratchpad", 0, 36);

    // The header of the note is the day and the time it was made.
    assert_eq!(drawn[3], wanted[3], "the two headers");
    // The body, beside the list, wrapped where the writer wrapped it.
    for row in 5..=14 {
        assert_eq!(right(&drawn[row]), right(&wanted[row]), "row {row}");
    }
    assert_eq!(drawn[34], wanted[34], "the hint bar of the note");
}

#[test]
fn one_tab_stacks_the_list_and_the_note() {
    let mut app = app();
    app.update(Action::NotesPage);
    app.update(Action::Confirm);
    let drawn = look(&app, 80, 44);

    // The list keeps to its own rows, and the note's header becomes the
    // rule between the two.
    assert!(drawn[5].starts_with("  ▪ Mention to Anna:"));
    assert!(drawn[10].starts_with("  +  new note"));
    assert!(
        drawn[11].starts_with(" Note Thu 4 Sep 16:40 ─────"),
        "{:?}",
        drawn[11]
    );
    assert_eq!(drawn[12], "  Mention to Anna:");
    assert_eq!(drawn[42], " NOTE  type to edit  esc back");
}

#[test]
fn a_note_written_in_the_small_hours_is_as_old_as_the_evening_it_came_from() {
    let mut model = wireframe_model();
    if let Some(note) = model.notes.get_mut(&1) {
        // Half past one on the Friday, which is Thursday's working day.
        note.created_at = at("2025-09-05T01:30:00+02:00[Europe/Copenhagen]");
    }
    let mut app = app_on(MemStore::holding(model));
    app.update(Action::NotesPage);

    assert!(look(&app, 120, 36)[5].contains("yesterday"));
}

#[test]
fn a_page_with_no_notes_on_it_names_the_key_that_makes_one() {
    let mut app = empty();
    app.update(Action::NotesPage);
    let drawn = look(&app, 120, 36);

    assert!(drawn[1].starts_with(" Notes 0 notes"));
    assert!(drawn[6].starts_with("  +  new note"), "{:?}", drawn[6]);
    assert!(drawn.join("\n").contains("No notes yet."));

    // And one note in is one note, not "1 notes".
    app.update(Action::Add);
    assert!(look(&app, 120, 36)[1].starts_with(" Notes 1 note "));
}

/// Everything to the right of the divider, which is the open note.
fn right(row: &str) -> String {
    row.chars()
        .skip(44)
        .collect::<String>()
        .trim_end()
        .to_owned()
}

/// The 44 columns the notes list has, without the pane beside it.
fn left(row: &str) -> String {
    row.chars()
        .take(44)
        .collect::<String>()
        .trim_end()
        .to_owned()
}

#[test]
fn a_window_too_small_for_the_frame_draws_nothing_rather_than_panicking() {
    let drawn = look(&app(), 20, 4);
    assert!(drawn.iter().all(|row| row.is_empty()));
}

#[test]
fn no_size_the_window_can_take_makes_the_drawing_panic() {
    for mut app in [app(), crowded(), long_titled()] {
        for action in [
            Action::Tick,
            Action::Commands,
            Action::Help,
            Action::Search,
            Action::MoveToDay,
            Action::DueBy,
            Action::Repeat,
            Action::Add,
        ] {
            app.update(action);
            for width in [1, 2, 23, 24, 25, 40, 99, 100, 101, 120, 200] {
                for height in [1, 2, 8, 9, 10, 12, 36, 48, 90] {
                    look(&app, width, height);
                }
            }
            app.update(Action::Cancel);
        }
    }
}

/// Every width a row can be drawn at, with more on its right than any
/// pane would ever hand it. The panes only ever ask for a few of these,
/// and the one that panicked was among them (F1).
#[test]
fn no_width_a_crowded_row_can_take_makes_the_drawing_panic() {
    let row = every_chip();
    for kind in [
        Kind::Open,
        Kind::Done,
        Kind::Waiting,
        Kind::Moved,
        Kind::Handled,
    ] {
        for narrow in [false, true] {
            for note in [None, Some("→ backlog")] {
                for width in 1..=200 {
                    one_row(
                        width,
                        &row,
                        Look {
                            kind,
                            today: on("2025-09-05"),
                            dates: DateOrder::DayFirst,
                            narrow,
                            moving: false,
                            note,
                            whole: false,
                        },
                    );
                }
            }
        }
    }
}

/// A title that is cut says so: the row ends in an ellipsis rather than
/// in the middle of a word, so a reader knows there is more.
#[test]
fn a_title_longer_than_its_row_ends_in_an_ellipsis() {
    let mut row = every_chip();
    row.title = "Review the complete kitchen renovation estimate".to_owned();
    let drawn = one_row(
        30,
        &row,
        Look {
            kind: Kind::Open,
            today: on("2025-09-05"),
            dates: DateOrder::DayFirst,
            narrow: true,
            moving: false,
            note: None,
            whole: false,
        },
    );

    assert!(drawn.contains('\u{2026}'), "{drawn:?}");
    assert!(drawn.starts_with(" [ ] Review the"), "{drawn:?}");
}

/// The cursor row is the one row that is read whole: its title goes on
/// under it, the rows below move down, and a click on any of its lines
/// is a click on it.
#[test]
fn the_cursor_row_shows_a_long_title_whole_on_the_lines_under_it() {
    let mut model = Model::empty();
    let title = "Review the complete kitchen renovation estimate and send detailed \
                 questions about delivery dates and installation costs";
    model
        .tasks
        .insert(1, task(1, title, Some(on("2025-09-05")), 0));
    model
        .tasks
        .insert(2, task(2, "Book dentist", Some(on("2025-09-05")), 1));
    let app = app_on(MemStore::holding(model));
    let (drawn, layout) = screen(&app, 120, 36);

    assert!(
        drawn[6].starts_with(" [ ] Review the complete kitchen"),
        "{:?}",
        drawn[6]
    );
    assert!(!drawn[6].contains('\u{2026}'), "read whole: {:?}", drawn[6]);
    assert!(
        drawn[7].starts_with("     "),
        "the rest is under the title: {:?}",
        drawn[7]
    );
    assert!(
        drawn[7].contains("send detailed questions"),
        "{:?}",
        drawn[7]
    );
    assert!(drawn[8].contains("installation costs"), "{:?}", drawn[8]);
    assert!(
        drawn[9].starts_with(" [ ] Book dentist"),
        "the next row moved down: {:?}",
        drawn[9]
    );

    let first = layout
        .rows
        .iter()
        .find(|area| area.id == crate::app::RowId::Task(1))
        .expect("the row");
    assert_eq!(first.area.height, 3);

    let mut app = app;
    app.update(Action::Down);
    let drawn = look(&app, 120, 36);
    assert!(
        drawn[6].contains('\u{2026}'),
        "cut again once the cursor has left: {:?}",
        drawn[6]
    );
    assert!(drawn[7].starts_with(" [ ] Book dentist"), "{:?}", drawn[7]);
}

/// The chips give way, not the row: whatever else it carries, a row keeps
/// its box and the start of its title, because that is all there is to
/// tell it from any other row (F1, F7).
#[test]
fn a_crowded_row_keeps_its_box_and_the_start_of_its_title() {
    let row = every_chip();
    for width in 14..=200 {
        let drawn = one_row(
            width,
            &row,
            Look {
                kind: Kind::Moved,
                today: on("2025-09-05"),
                dates: DateOrder::DayFirst,
                narrow: false,
                moving: false,
                note: None,
                whole: false,
            },
        );
        assert!(
            drawn.starts_with(" [\u{2192}] Write"),
            "at {width} columns: {drawn:?}"
        );
    }
}

/// The row the acceptance test crashed on: a task moved to the backlog
/// while waiting, due and reminded, whose pointer in the Moved group had
/// eaten its own title (F7).
#[test]
fn a_moved_row_keeps_its_pointer_its_title_and_its_margin() {
    let drawn = look(&crowded(), 120, 36);
    let moved = drawn
        .iter()
        .find(|row| row.contains("\u{2192}]"))
        .expect("the moved row");

    assert!(
        moved.starts_with(" [\u{2192}] Task"),
        "the margin, the pointer and the title: {moved:?}"
    );
    assert!(
        moved.contains("[w]") && moved.contains("[due]") && moved.contains("[\u{25f7}]"),
        "and every chip, as its mark: {moved:?}"
    );
}

#[test]
fn no_size_the_review_can_take_makes_the_drawing_panic() {
    let mut app = reviewing();
    for action in [Action::Tick, Action::MoveToDay, Action::Edit] {
        app.update(action);
        for width in [1, 2, 23, 24, 25, 40, 99, 100, 101, 120, 200] {
            for height in [1, 2, 8, 9, 10, 12, 36, 48, 90] {
                look(&app, width, height);
            }
        }
        app.update(Action::Cancel);
    }
    // And the second step, which has three groups and no panel hint.
    app.update(Action::Confirm);
    for width in [24, 60, 99, 100, 120] {
        for height in [9, 12, 36, 48] {
            look(&app, width, height);
        }
    }
}

#[test]
fn no_size_the_notes_page_can_take_makes_the_drawing_panic() {
    let mut app = app();
    app.update(Action::NotesPage);
    // The list, then the note open under it, then the list again.
    for action in [Action::Confirm, Action::Insert('x'), Action::Cancel] {
        app.update(action);
        assert_eq!(app.page(), Page::Notes);
        for width in [1, 2, 23, 24, 25, 40, 45, 99, 100, 101, 120, 200] {
            for height in [1, 2, 8, 9, 10, 11, 12, 13, 36, 48, 90] {
                look(&app, width, height);
            }
        }
    }
}

#[test]
fn the_narrow_hint_bar_names_five_keys_and_defers_to_help() {
    let mut app = app();

    let day = look(&app, 80, 44);
    assert_eq!(
        day[42],
        " TODAY  space done  f focus  a add  b backlog  x del                     ? more"
    );

    app.update(Action::PaneRight);
    let backlog = look(&app, 80, 44);
    assert!(backlog[42].starts_with(" BACKLOG  space done  t today  a add  x del"));
    assert!(backlog[42].ends_with("? more"));
    assert_eq!(backlog[2].chars().filter(|glyph| *glyph == '─').count(), 80);
}

#[test]
fn the_narrow_tab_row_marks_the_tab_the_keyboard_is_on() {
    let mut app = app();
    let (_, layout) = screen(&app, 80, 44);
    app.set_layout(layout);
    assert_eq!(look(&app, 80, 44)[3], "  TODAY 6   BACKLOG 12   NOTES 4");

    // Right from the backlog is the notes tab, not a pane of its own.
    app.update(Action::PaneRight);
    app.update(Action::PaneRight);
    let notes = look(&app, 80, 44);
    assert_eq!(app.page(), Page::Notes);
    assert!(notes[1].starts_with(" Notes 4 notes"));
    assert!(notes[5].contains("▪ Mention to Anna:"));
}

/// The colour and attribute parameters of every `ESC [ … m` written, with
/// an indexed colour kept together as `38;5;4`.
fn style_codes(written: &str) -> Vec<String> {
    let mut codes = Vec::new();
    for rest in written.split("\u{1b}[").skip(1) {
        let Some(end) = rest.find('m') else { continue };
        let (parameters, after) = rest.split_at(end);
        if !after.starts_with('m') || parameters.contains(|c: char| !"0123456789;".contains(c)) {
            continue;
        }
        let mut parameters = parameters.split(';').peekable();
        while let Some(parameter) = parameters.next() {
            // 38 and 48 take their colour from the parameters after them.
            if parameter == "38" || parameter == "48" {
                let kind = parameters.next().unwrap_or_default();
                let taking = if kind == "2" { 3 } else { 1 };
                let rest: Vec<&str> = (0..taking).filter_map(|_| parameters.next()).collect();
                codes.push(format!("{parameter};{kind};{}", rest.join(";")));
            } else {
                codes.push(parameter.to_owned());
            }
        }
    }
    codes
}

#[test]
fn the_bytes_the_terminal_gets_name_a_palette_slot_and_never_a_colour() {
    // This is what following a live Omarchy theme change comes down to:
    // the program names slot 4, the theme decides what blue is, and the
    // program is never told. A truecolour value here would freeze the
    // screen at one theme.
    let mut app = app();
    app.update(Action::Commands);

    let mut bytes: Vec<u8> = Vec::new();
    {
        let terminal = Terminal::with_options(
            CrosstermBackend::new(&mut bytes),
            TerminalOptions {
                viewport: Viewport::Fixed(Rect::new(0, 0, 120, 36)),
            },
        );
        terminal
            .expect("a terminal writing to a buffer")
            .draw(|frame| {
                draw(&app, frame);
            })
            .expect("a frame");
    }

    let written = String::from_utf8(bytes).expect("what was written");
    let mut colours = 0;
    for code in style_codes(&written) {
        let slot = match code.strip_prefix("38;5;").or(code.strip_prefix("48;5;")) {
            Some(slot) => slot.parse::<u16>().expect("a palette slot"),
            None => {
                assert!(
                    !code.starts_with("38;") && !code.starts_with("48;"),
                    "{code} is a colour value, not a slot the theme fills"
                );
                continue;
            }
        };
        assert!(slot < 16, "slot {slot} is outside the sixteen a theme sets");
        colours += 1;
    }
    assert!(colours > 0, "the screen has some colour on it");
}

#[test]
fn a_pane_longer_than_the_window_follows_the_cursor() {
    let mut model = Model::empty();
    for at in 0..40 {
        model.tasks.insert(
            at + 1,
            task(at + 1, &format!("Task {}", at + 1), None, at as usize),
        );
    }
    let mut app = app_on(MemStore::holding(model));
    app.update(Action::PaneRight);

    let top = look(&app, 120, 36);
    assert!(top.iter().any(|line| line.ends_with("Task 1")));
    assert!(!top.iter().any(|line| line.ends_with("Task 40")));

    for _ in 0..39 {
        app.update(Action::Down);
    }

    let bottom = look(&app, 120, 36);
    assert!(
        bottom.iter().any(|line| line.ends_with("Task 40")),
        "the cursor row is on screen"
    );
    assert!(
        !bottom.iter().any(|line| line.ends_with("Task 1")),
        "and the pane has scrolled no further than it had to"
    );
    assert!(
        bottom.iter().any(|line| line.ends_with("Task 13")),
        "the window has moved down by exactly what it had to"
    );
}

#[test]
fn a_title_of_wide_characters_keeps_every_character_and_its_chips() {
    let mut model = Model::empty();
    model.tasks.insert(
        1,
        Task {
            due_on: Some(on("2025-09-30")),
            ..task(1, "日本語 🙂 の報告", None, 0)
        },
    );
    let app = app_on(MemStore::holding(model));

    let row = glyphs(&app, 120, 36)
        .into_iter()
        .find(|row| row.contains("[due 30 Sep]"))
        .expect("the backlog row");

    assert!(row.contains("[ ] 日本語 🙂 の報告"), "{row:?}");
    assert!(
        row.ends_with("[due 30 Sep]"),
        "and the chip is where it was: {row:?}"
    );
}

#[test]
fn a_note_of_wide_characters_wraps_and_puts_its_caret_by_cells() {
    let mut model = Model::empty();
    model.notes.insert(
        1,
        Note {
            id: 1,
            body: "日本語です".to_owned(),
            created_at: at(NOW),
            updated_at: at(NOW),
            deleted_at: None,
        },
    );
    let mut app = app_on(MemStore::holding(model));
    app.update(Action::NotesPage);
    app.update(Action::Confirm);
    // Into the body, and back two characters, which is four cells.
    app.update(Action::LineEnd);
    app.update(Action::Left);
    app.update(Action::Left);

    let row = glyphs(&app, 120, 36)
        .into_iter()
        .find(|row| row.contains("日本語"))
        .expect("the body");

    assert!(row.contains("日本語▏です"), "{row:?}");
}

#[test]
fn a_field_of_wide_characters_puts_its_caret_where_the_cells_end() {
    let mut app = empty();
    app.update(Action::Add);
    for glyph in "日本🙂語".chars() {
        app.update(Action::Insert(glyph));
    }
    app.update(Action::Left);

    let row = glyphs(&app, 120, 36)
        .into_iter()
        .find(|row| row.contains('▏'))
        .expect("the field");

    assert!(row.contains("日本🙂▏語"), "{row:?}");
}

#[test]
fn a_day_header_keeps_its_counts_clear_of_its_label() {
    let mut model = Model::empty();
    let tomorrow = on("2025-09-06");
    model.tasks.insert(1, task(1, "Task", Some(tomorrow), 0));
    model.placements.insert(
        (1, tomorrow),
        Placement {
            task_id: 1,
            day: tomorrow,
            placed_at: at(NOW),
            from_place: FromPlace::New,
        },
    );
    let mut app = app_on(MemStore::holding(model));
    app.update(Action::NextDay);

    let header: Vec<char> = look(&app, 120, 36).remove(3).chars().collect();
    let pane: String = header[..59].iter().collect();
    assert_eq!(
        pane.trim_end(),
        " Sat 6 Sep future day                   1 planned · 1 open",
        "the label, blank cells between, and only the counts there are"
    );
}

/// The row that ends a schedule has no next date to preview, and said so
/// with the sentence for a rule that falls on no day, clipped mid-word
/// (F11).
#[test]
fn stopping_a_repeat_says_what_stopping_does() {
    let mut app = app();
    app.update(Action::Repeat);
    app.update(Action::StopRepeat);

    for width in [120, 80] {
        let text = look(&app, width, 36).join("\n");
        assert!(
            text.contains("No new copies; the ones already made stay."),
            "at {width} columns, in full:\n{text}"
        );
    }
}

/// One of anything is one, not one of several: the search count said
/// `1 matches` (F12), and a repeat every one week said `every 1 weeks`.
#[test]
fn a_count_of_one_puts_its_noun_in_the_singular() {
    let mut app = app();
    app.update(Action::Search);
    for typed in "Renew passport".chars() {
        app.update(Action::Insert(typed));
    }
    let text = look(&app, 120, 36).join("\n");
    assert!(text.contains("1 match"), "{text}");
    assert!(!text.contains("1 matches"), "{text}");

    app.update(Action::Cancel);
    app.update(Action::Repeat);
    app.update(Action::EveryFewWeeks);
    app.update(Action::Left);
    let text = look(&app, 120, 36).join("\n");
    assert!(text.contains("[ 1 ] week from"), "{text}");

    // And a count of none keeps the plural.
    let empty = look(&empty(), 120, 36).join("\n");
    assert!(empty.contains("0 notes"), "{empty}");
}

/// A title longer than the row showed its beginning while the caret was
/// off the end of the line, so the person could not see what they typed.
#[test]
fn a_field_longer_than_its_line_scrolls_to_keep_the_caret_on_it() {
    let mut app = empty();
    app.update(Action::Add);
    for typed in "The quick brown fox jumps over the lazy dog and keeps on going END".chars() {
        app.update(Action::Insert(typed));
    }

    let field = look(&app, 120, 36)
        .into_iter()
        .find(|row| row.contains('▏'))
        .expect("the add field");
    assert!(field.contains("going END▏"), "the end of it: {field:?}");
    assert!(!field.contains("The quick"), "and not the start: {field:?}");

    // Back to the beginning, and the line comes with it.
    app.update(Action::LineStart);
    let field = look(&app, 120, 36)
        .into_iter()
        .find(|row| row.contains('▏'))
        .expect("the add field");
    assert!(field.contains("▏The quick brown fox"), "{field:?}");
}

/// An empty group is not drawn, and the add line is not a row of Plan:
/// a day whose tasks were all done still had a PLAN label over nothing
/// but the add line (F9).
#[test]
fn a_day_with_nothing_left_open_keeps_the_add_line_and_loses_the_label() {
    let mut app = empty();
    app.update(Action::Add);
    for typed in "Task".chars() {
        app.update(Action::Insert(typed));
    }
    app.update(Action::Confirm);
    app.update(Action::Cancel);
    app.update(Action::Close);

    let drawn = look(&app, 120, 36);
    let text: String = drawn
        .iter()
        .map(|row| row.chars().take(59).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains(" +  add a task"),
        "the key that fills it: {text}"
    );
    assert!(!text.contains("PLAN"), "and no group over nothing: {text}");
    assert!(
        text.contains("DONE 1"),
        "the group that has something: {text}"
    );
}

/// Wireframe 11 groups the palette by what a key acts on, and the row's
/// own title is what says which row that is: the program had one section
/// under the page's name (F10).
#[test]
fn the_palette_names_the_row_it_is_about_and_the_app_apart() {
    let mut app = app();
    app.update(Action::Commands);
    let drawn = look(&app, 120, 36);
    let text = drawn.join("\n");

    assert!(text.contains("FOR \"SHIP INVOICE EXPORT\""), "{text}");
    assert!(text.contains("APP"), "{text}");
    let for_the_row = drawn.iter().position(|row| row.contains("FOR \""));
    let for_the_app = drawn.iter().position(|row| row.contains(" APP "));
    assert!(for_the_row < for_the_app, "the row's section comes first");

    // Filtering keeps the heading of whichever section still has a row.
    for typed in "dele".chars() {
        app.update(Action::Insert(typed));
    }
    let text = look(&app, 120, 36).join("\n");
    assert!(text.contains("FOR \"SHIP INVOICE EXPORT\""), "{text}");
    assert!(text.contains("Delete"), "{text}");
    assert!(
        !text.contains("APP"),
        "and drops the one that has none: {text}"
    );
}

/// A review whose only surfaced rows are the copies a schedule started
/// this morning, which is a step that asks nothing.
fn nothing_to_decide() -> App {
    let today = on("2025-09-05");
    let mut model = Model::empty();
    model.schedules.insert(
        1,
        Schedule {
            id: 1,
            title: "Write standup notes".to_owned(),
            rule: Rule::Workdays,
            generated_through: today,
            stopped_on: None,
            created_at: at(NOW),
        },
    );
    model.tasks.insert(
        1,
        Task {
            schedule_id: Some(1),
            scheduled_on: Some(today),
            ..task(1, "Write standup notes", Some(today), 0)
        },
    );
    app_on(MemStore::holding(model))
}

/// A surfaced step whose only rows are today's copies asks nothing, and
/// said `0 surfaced today` and `0 of 0 decided` under a full bar.
#[test]
fn a_step_with_nothing_to_decide_says_so_and_draws_no_bar() {
    let app = nothing_to_decide();

    let text = look(&app, 120, 36).join("\n");
    assert!(text.contains("ALSO STARTING TODAY 1"), "{text}");
    assert!(
        text.contains("1 starting today, nothing to decide"),
        "{text}"
    );
    assert!(text.contains("Nothing to decide here."), "{text}");
    assert!(!text.contains("of 0 decided"), "{text}");
    assert!(
        !text.contains('█'),
        "and no bar over a total of none: {text}"
    );
    for outcome in ["Pull onto today", "Keep in backlog", "Mark waiting"] {
        assert!(
            !text.contains(outcome),
            "and no outcome for a row nobody is asked about: {outcome}\n{text}"
        );
    }
    assert!(
        text.contains("Start the day"),
        "the one thing to press stays: {text}"
    );
}

/// The hint bar named the outcomes beside a step that asks for none, and
/// under 100 columns it dropped Enter, so the one thing to press was
/// named nowhere.
#[test]
fn the_step_that_asks_nothing_names_only_the_one_thing_to_press() {
    let app = nothing_to_decide();

    let wide = look(&app, 120, 36);
    assert_eq!(
        wide[34],
        " SURFACED                                                                                     ⏎ start the day  esc skip"
    );
    assert_eq!(
        wide[1],
        " MORNING REVIEW step 1 of 1 · due & reminders                    1 starting today, nothing to decide   esc skip for now"
    );

    let narrow = look(&app, 60, 44);
    assert_eq!(
        narrow[42],
        " SURFACED                         ⏎ start the day  esc skip"
    );
    // `due & remindnothing to decide` ran the two ends together.
    assert_eq!(
        narrow[1],
        " MORNING REVIEW step 1 of 1               nothing to decide"
    );
}

/// Adding leaves the field open after Enter, and the hint bar went on
/// offering `u undo` for the task just added, where `u` types a letter.
#[test]
fn the_hint_bar_does_not_offer_a_key_the_open_field_would_type() {
    let mut app = empty();
    app.update(Action::Add);
    for typed in "Task".chars() {
        app.update(Action::Insert(typed));
    }
    app.update(Action::Confirm);

    let bar = look(&app, 120, 36).remove(34);
    assert!(
        bar.contains("Added \"Task\""),
        "what just happened: {bar:?}"
    );
    assert!(!bar.contains("undo"), "and no key the field types: {bar:?}");

    // Out of the field, and the offer is there again.
    app.update(Action::Cancel);
    app.update(Action::Close);
    let bar = look(&app, 120, 36).remove(34);
    assert!(bar.contains("u  undo"), "{bar:?}");
}

/// The search box ran its query under the match count, so the end of a
/// long query and its caret were both lost (F14).
#[test]
fn a_long_query_scrolls_with_its_caret_and_stops_before_the_count() {
    let mut app = app();
    app.update(Action::Search);
    let typed = "0123456789".repeat(8);
    for glyph in typed.chars() {
        app.update(Action::Insert(glyph));
    }

    for width in [120, 60] {
        let box_row = look(&app, width, 36)
            .into_iter()
            .find(|row| row.contains("matches"))
            .unwrap_or_else(|| panic!("the search box at {width}"));
        assert!(
            box_row.contains('▏'),
            "the caret is on it at {width}: {box_row:?}"
        );
        let (query, count) = box_row.split_once('▏').expect("the caret");
        assert!(
            query.ends_with("6789"),
            "the end of what was typed at {width}: {box_row:?}"
        );
        assert!(
            count.starts_with("  0 matches "),
            "a gap, then the count with nothing of the query under it, \
             at {width}: {box_row:?}"
        );
    }
}

/// A result longer than the box was drawn from its left edge onwards, so
/// it ran through the location beside it and out through the right
/// border of the card (F15).
#[test]
fn a_long_result_stops_before_its_location_and_the_border() {
    let app = long_titled();
    let mut app = app;
    app.update(Action::Search);

    for width in [120, 80] {
        let drawn = look(&app, width, 36);
        let top = drawn
            .iter()
            .position(|row| row.contains('┌'))
            .unwrap_or_else(|| panic!("the top of the box at {width}"));
        let right = drawn[top]
            .chars()
            .position(|glyph| glyph == '┐')
            .expect("the corner");
        let found = drawn[top..]
            .iter()
            .find(|row| row.contains("[ ] 0123"))
            .unwrap_or_else(|| panic!("the result at {width}"));

        assert_eq!(
            found.chars().nth(right),
            Some('│'),
            "the border is on the result row too at {width}: {found:?}"
        );
        let inside: String = found.chars().take(right).collect();
        assert!(
            inside.trim_end().ends_with("  today"),
            "the location has its own room at {width}: {found:?}"
        );
        assert!(
            !inside.contains("0today"),
            "and the title stops before it at {width}: {found:?}"
        );
    }
}

/// `café` with the accent as a combining mark of its own, and a family
/// emoji made of four people and three joiners. One is a cluster of two
/// characters a cell wide, the other a cluster of seven two cells wide,
/// and neither is anything at all a character at a time (F6).
const CLUSTERS: &str = "cafe\u{301} \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";

/// The one row of the screen a caret is on.
fn typing(app: &App) -> String {
    glyphs(app, 120, 36)
        .into_iter()
        .find(|row| row.contains('▏'))
        .expect("the line being typed")
}

/// Every field was edited a character at a time, so the accent of a
/// decomposed `é` was dropped on the way to the screen and the caret
/// could land inside a family emoji and take it apart (F6).
#[test]
fn a_title_of_clusters_keeps_its_marks_and_steps_over_them_whole() {
    let mut app = empty();
    app.update(Action::Add);
    for typed in CLUSTERS.chars() {
        app.update(Action::Insert(typed));
    }
    assert!(
        typing(&app).contains(&format!("{CLUSTERS}▏")),
        "what was typed, with its caret after it: {:?}",
        typing(&app)
    );

    // One step left is the whole family, not one of the four people in
    // it; one more is the space.
    app.update(Action::Left);
    assert!(
        typing(&app)
            .contains("cafe\u{301} ▏\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}"),
        "the caret in front of the family: {:?}",
        typing(&app)
    );
    app.update(Action::Left);
    app.update(Action::Insert('X'));
    assert!(
        typing(&app).contains("cafe\u{301}X▏ \u{1F468}\u{200D}"),
        "a letter beside the accent, which keeps it: {:?}",
        typing(&app)
    );

    app.update(Action::Confirm);
    let title = app
        .model()
        .tasks
        .values()
        .map(|task| task.title.clone())
        .next()
        .expect("the task");
    assert_eq!(
        title, "cafe\u{301}X \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}",
        "every code point, in the order they were typed"
    );
}

/// The same in the search box, over a task whose stored title has the
/// same clusters in it.
#[test]
fn a_query_of_clusters_finds_the_row_it_is_stored_in() {
    let mut model = Model::empty();
    model
        .tasks
        .insert(1, task(1, CLUSTERS, Some(on("2025-09-05")), 0));
    let mut app = app_on(MemStore::holding(model));
    app.update(Action::Search);
    for typed in CLUSTERS.chars() {
        app.update(Action::Insert(typed));
    }

    let found = glyphs(&app, 120, 36)
        .into_iter()
        .find(|row| row.contains("[ ] cafe\u{301}"))
        .expect("the result");
    assert!(
        found.contains(&format!("[ ] {CLUSTERS}")),
        "the stored title, drawn whole: {found:?}"
    );
    assert!(
        typing(&app).contains(&format!("{CLUSTERS}▏")),
        "and the query it was found by: {:?}",
        typing(&app)
    );

    app.update(Action::Left);
    app.update(Action::Insert('X'));
    assert!(
        typing(&app).contains("cafe\u{301} X▏\u{1F468}\u{200D}"),
        "a letter typed in front of the family: {:?}",
        typing(&app)
    );
    assert_eq!(
        app.model().task(1).map(|task| task.title.clone()),
        Some(CLUSTERS.to_owned()),
        "and the stored title is untouched"
    );
}

/// And in an open note, whose body wraps by cluster as well.
#[test]
fn a_note_of_clusters_wraps_and_saves_them_whole() {
    let mut app = empty();
    app.update(Action::NotesPage);
    app.update(Action::Add);
    for typed in CLUSTERS.chars() {
        app.update(Action::Insert(typed));
    }
    assert!(
        typing(&app).contains(&format!("{CLUSTERS}▏")),
        "the body being typed: {:?}",
        typing(&app)
    );

    app.update(Action::Left);
    app.update(Action::Left);
    app.update(Action::Insert('X'));
    assert!(
        typing(&app).contains("cafe\u{301}X▏ \u{1F468}\u{200D}"),
        "a letter beside the accent: {:?}",
        typing(&app)
    );

    // A note is written on the first tick after it changes.
    app.update(Action::Tick);
    let body = app
        .model()
        .notes
        .values()
        .map(|note| note.body.clone())
        .next()
        .expect("the note");
    assert_eq!(
        body,
        "cafe\u{301}X \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}"
    );
}

/// Where a centred box is centred (DESIGN.md section 2): the pane area,
/// between the rule under the pane headers and the rule over the hint
/// bar, with the same room above it as below.
#[test]
fn a_card_is_centred_in_the_pane_area_between_the_two_rules() {
    let mut app = app();
    app.update(Action::Repeat);

    for height in [36, 48] {
        let drawn = look(&app, 120, height);
        let top = drawn
            .iter()
            .position(|row| row.contains('┌'))
            .unwrap_or_else(|| panic!("the top of the card at {height}"));
        let foot = drawn
            .iter()
            .rposition(|row| row.contains('└'))
            .unwrap_or_else(|| panic!("the foot of the card at {height}"));

        // The first and last row the panes have.
        let first = 5;
        let last = height as usize - 4;
        let above = top - first;
        let below = last - foot;
        assert!(
            above.abs_diff(below) <= 1,
            "the same room above and below at {height}: {above} and {below}"
        );
    }
}

/// A window shorter than the card keeps the card whole and lets it cover
/// the hint bar, because the card's own footer names its keys.
#[test]
fn a_card_taller_than_the_window_covers_the_hint_bar_whole() {
    let mut app = app();
    app.update(Action::Repeat);

    let drawn = look(&app, 60, 20);
    assert!(
        drawn.iter().any(|row| row.contains('┌')),
        "the top of the card: {drawn:?}"
    );
    assert!(
        drawn.iter().any(|row| row.contains("⏎ save")),
        "and its footer, which is where its keys are: {drawn:?}"
    );
}

#[test]
fn the_settings_page_matches_the_wireframe() {
    let mut app = app();
    app.update(Action::SettingsPage);
    same(
        &look(&app, 120, 36),
        &wireframe("13-settings", 0, 36),
        "the settings page",
    );
}

#[test]
fn a_narrow_settings_page_keeps_the_list_and_drops_the_description() {
    let mut app = app();
    app.update(Action::SettingsPage);
    let drawn = look(&app, 80, 44);
    let text = drawn.join("\n");

    assert!(
        !text.contains("The hour the working day rolls over"),
        "there is no room beside the list for what the row does"
    );
    assert!(
        !text.contains('\u{2502}'),
        "and so no divider either:\n{text}"
    );
    // The page is not one of the three tabs, so it keeps its header at
    // every width.
    assert!(drawn[3].starts_with(" Settings"), "{:?}", drawn[3]);
    assert!(
        text.contains("Confirm before delete"),
        "the whole list is on"
    );
}

#[test]
fn the_cursor_rows_value_is_the_one_a_key_would_change() {
    let mut app = app();
    app.update(Action::SettingsPage);
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).expect("a test terminal");
    terminal
        .draw(|frame| _ = draw(&app, frame))
        .expect("a frame");
    let buffer = terminal.backend().buffer();

    let row = lines(buffer)
        .iter()
        .position(|line| line.starts_with("  Day starts at"))
        .expect("the first row") as u16;
    let at = lines(buffer)[row as usize].find("5:00").expect("its value") as u16;
    assert_eq!(buffer[(at, row)].fg, Color::Blue, "the value is in accent");
    assert!(
        buffer[(at, row)].modifier.contains(Modifier::REVERSED),
        "under the cursor row's own weight"
    );
}

#[test]
fn a_typed_row_becomes_a_field_where_its_value_was() {
    let mut app = app();
    app.update(Action::SettingsPage);
    app.update(Action::Confirm);
    app.update(Action::Backspace);
    app.update(Action::Insert('9'));
    let drawn = look(&app, 120, 36);

    let row = drawn
        .iter()
        .find(|line| line.starts_with("  Day starts at"))
        .expect("the row");
    assert!(
        row.contains("9\u{258f}"),
        "the caret follows the digit: {row:?}"
    );
    assert!(
        drawn[34].contains("⏎ save") && drawn[34].contains("esc cancel"),
        "and the hint bar is the field's: {:?}",
        drawn[34]
    );
}
