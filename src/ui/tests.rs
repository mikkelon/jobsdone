use super::*;

use std::fs;

use jiff::Zoned;
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::style::Color;
use ratatui::{Terminal, TerminalOptions, Viewport};

use crate::app::App;
use crate::domain::tests::MemStore;
use crate::input::Action;

/// The wireframes are the source of truth for the layout, so the test is a
/// character-by-character comparison against them at the three sizes
/// DESIGN.md names.
const WIREFRAMES: &str = "wireframes";

fn app() -> App {
    let now: Zoned = "2026-09-05T09:00:00+02:00[Europe/Copenhagen]"
        .parse()
        .expect("a zoned timestamp");
    App::new(Box::new(MemStore::new()), &now).expect("an app")
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
    assert_eq!(backlog.len(), 12);
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

    let mut app = app();
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).expect("a test terminal");
    for action in [Action::Tick, Action::Commands, Action::Help, Action::Search] {
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
fn search_groups_what_it_finds() {
    let mut app = app();
    app.update(Action::Search);
    for typed in "invoice".chars() {
        app.update(Action::Insert(typed));
    }
    let text = look(&app, 120, 36).join("\n");

    assert!(text.contains("9 matches"));
    assert!(text.contains("OPEN"));
    assert!(text.contains("CLOSED"));
    assert!(text.contains("Ship invoice export"));
}

#[test]
fn an_empty_list_names_the_keys_that_fill_it() {
    let mut app = app();
    app.show_empty();
    let text = look(&app, 120, 36).join("\n");

    assert!(text.contains("Nothing planned."));
    assert!(text.contains("a add a task · l then t pull from the backlog"));
    assert!(text.contains("Backlog is empty."));
    assert!(text.contains("a add · b on a day task sends it here"));
    assert!(text.contains("nothing planned"), "and the header says so");
}

#[test]
fn the_notes_page_gives_the_open_note_the_width() {
    let mut app = app();
    app.update(Action::NotesPage);
    let drawn = look(&app, 120, 36);

    assert!(drawn[1].contains("Notes 4 notes"));
    assert!(drawn[1].contains("n or esc back to today"));
    assert!(drawn[3].contains("Note Thu 4 Sep 16:40"));
    assert!(drawn[5].contains("▪ Mention to Anna"));
    assert!(drawn[5].contains("Mention to Anna:"));
    let divider = drawn[4].chars().position(|glyph| glyph == '┬');
    assert_eq!(
        divider,
        Some(44),
        "the list is a column, not half the window"
    );
}

#[test]
fn a_window_too_small_for_the_frame_draws_nothing_rather_than_panicking() {
    let drawn = look(&app(), 20, 4);
    assert!(drawn.iter().all(|row| row.is_empty()));
}

#[test]
fn no_size_the_window_can_take_makes_the_drawing_panic() {
    let mut app = app();
    for action in [Action::Tick, Action::Commands, Action::Help, Action::Search] {
        app.update(action);
        for width in [1, 2, 23, 24, 25, 40, 99, 100, 101, 120, 200] {
            for height in [1, 2, 8, 9, 10, 12, 36, 48, 90] {
                look(&app, width, height);
            }
        }
        app.update(Action::Cancel);
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
    assert!(notes[5].contains("▪ Mention to Anna"));
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
