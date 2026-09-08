//! The one wrapping, on its own: where the rows fall, which of them a
//! caret is on, and which character a cell of one is.

use super::{Affinity, Wrapping, viewport};

/// The rows as text, which is what a reader of the test cares about.
fn rows(body: &str, width: u16) -> Vec<String> {
    Wrapping::of(body, width)
        .rows()
        .iter()
        .map(|row| row.text.clone())
        .collect()
}

#[test]
fn a_line_too_long_for_the_pane_breaks_on_a_space() {
    assert_eq!(rows("hello there world", 8), ["hello ", "there ", "world"]);
}

#[test]
fn a_word_longer_than_the_pane_is_broken_through() {
    assert_eq!(
        rows("antidisestablishment", 8),
        ["antidise", "stablish", "ment"]
    );
}

#[test]
fn the_rows_carry_the_character_of_the_body_they_start_at() {
    let wrapping = Wrapping::of("hello there world", 8);
    let starts: Vec<usize> = wrapping.rows().iter().map(|row| row.start).collect();
    assert_eq!(starts, [0, 6, 12]);
}

#[test]
fn a_newline_the_writer_typed_is_not_a_wrap_the_pane_made() {
    let wrapping = Wrapping::of("one\ntwo", 40);
    let broken: Vec<bool> = wrapping.rows().iter().map(|row| row.continues).collect();
    assert_eq!(broken, [false, false], "neither row runs out of cells");
    let starts: Vec<usize> = wrapping.rows().iter().map(|row| row.start).collect();
    assert_eq!(starts, [0, 4], "the newline is a character of the body");
}

#[test]
fn an_empty_body_is_one_row_and_a_trailing_newline_opens_another() {
    assert_eq!(rows("", 20), [""]);
    assert_eq!(rows("one\n", 20), ["one", ""]);
    assert_eq!(rows("\n\n", 20), ["", "", ""]);
    let wrapping = Wrapping::of("one\n", 20);
    assert_eq!(wrapping.rows()[1].start, 4, "past the newline");
}

#[test]
fn wide_characters_are_counted_in_cells_and_not_in_characters() {
    // Four cells to a row, so two of these to a row.
    assert_eq!(rows("日本語です", 4), ["日本", "語で", "す"]);
    let wrapping = Wrapping::of("日本語です", 4);
    assert_eq!(
        wrapping.column_of(1, Affinity::default()),
        2,
        "one wide one"
    );
}

#[test]
fn a_combining_mark_and_a_family_are_one_character_each() {
    // A decomposed e-acute is one cluster of one cell; the family emoji
    // is one cluster of two.
    let wrapping = Wrapping::of("e\u{301}\u{1f469}\u{200d}\u{1f467}", 40);
    assert_eq!(wrapping.rows().len(), 1);
    assert_eq!(wrapping.column_of(1, Affinity::default()), 1);
    assert_eq!(wrapping.column_of(2, Affinity::default()), 3);
}

#[test]
fn a_caret_on_a_wrap_the_pane_made_is_on_whichever_row_it_came_from() {
    let wrapping = Wrapping::of("hello there world", 8);
    // Character six is the end of the first row and the start of the
    // second, and the same number either way.
    assert_eq!(wrapping.row_of(6, Affinity::AfterTheBreak), 1);
    assert_eq!(wrapping.row_of(6, Affinity::BeforeTheBreak), 0);
    assert_eq!(wrapping.column_of(6, Affinity::AfterTheBreak), 0);
    assert_eq!(wrapping.column_of(6, Affinity::BeforeTheBreak), 6);
}

#[test]
fn a_newline_is_never_two_places_at_once() {
    // The row before it does not continue, so the affinity has nothing
    // to choose between.
    let wrapping = Wrapping::of("one\ntwo", 40);
    assert_eq!(wrapping.row_of(4, Affinity::BeforeTheBreak), 1);
    assert_eq!(
        wrapping.row_of(3, Affinity::BeforeTheBreak),
        0,
        "the newline"
    );
}

#[test]
fn a_row_stepped_onto_short_of_the_column_takes_the_caret_to_its_end() {
    let wrapping = Wrapping::of("hello there world", 8);
    // Past the end of a row the pane broke: the end of that row.
    assert_eq!(
        wrapping.caret_at(0, 40),
        (6, Affinity::BeforeTheBreak),
        "and it stays on the row it was stepped onto"
    );
    // Past the end of the last row: the end of the body.
    assert_eq!(wrapping.caret_at(2, 40), (17, Affinity::AfterTheBreak));
}

#[test]
fn a_cell_of_a_row_is_the_character_drawn_in_it() {
    let wrapping = Wrapping::of("hello there world", 8);
    assert_eq!(wrapping.caret_at(1, 0), (6, Affinity::AfterTheBreak));
    assert_eq!(wrapping.caret_at(1, 3), (9, Affinity::AfterTheBreak));
    // Either cell of a wide character is that character.
    let wide = Wrapping::of("日本語", 40);
    assert_eq!(wide.caret_at(0, 2), (1, Affinity::AfterTheBreak));
    assert_eq!(wide.caret_at(0, 3), (1, Affinity::AfterTheBreak));
    assert_eq!(wide.caret_at(0, 4), (2, Affinity::AfterTheBreak));
}

#[test]
fn the_rows_on_screen_stay_where_they_are_while_the_caret_is_among_them() {
    // Twenty rows in a window ten high, showing rows five to fourteen.
    assert_eq!(viewport(5, 14, 20, 10), 5);
    assert_eq!(viewport(5, 5, 20, 10), 5);
    // The caret one row up from the top of them takes them with it, one
    // row and no more.
    assert_eq!(viewport(5, 4, 20, 10), 4);
    // And one row below the bottom, the same the other way.
    assert_eq!(viewport(5, 15, 20, 10), 6);
}

#[test]
fn a_window_that_grew_shows_as_much_of_the_note_as_it_can() {
    // Twenty rows, scrolled to the last ten, in a window that is now
    // fifteen high: the rows come back rather than leaving five blank.
    assert_eq!(viewport(10, 12, 20, 15), 5);
    // A window taller than the whole note shows all of it from the top.
    assert_eq!(viewport(10, 12, 20, 30), 0);
}

#[test]
fn a_window_that_shrank_keeps_the_caret_on_screen() {
    // Showing rows five to fourteen with the caret on the last of them,
    // in a window that is now three high.
    assert_eq!(viewport(5, 14, 20, 3), 12);
    assert_eq!(viewport(5, 14, 20, 1), 14);
    // And no height at all is still a row.
    assert_eq!(viewport(5, 14, 20, 0), 14);
}
