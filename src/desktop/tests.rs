use super::*;

fn floating() -> String {
    block(true, WindowSize::default())
}

#[test]
fn an_untouched_file_gets_the_block_at_the_end() {
    let was = "o.bind(\"SUPER SHIFT\", \"J\", \"jobsdone\")\n";

    let now = with_block(was, &floating());

    assert_eq!(
        now,
        "o.bind(\"SUPER SHIFT\", \"J\", \"jobsdone\")\n\n\
         -- jobsdone: window (begin)\n\
         o.window(\"org.omarchy.jobsdone\", { float = true, center = true, size = { 870, 650 } })\n\
         -- jobsdone: window (end)\n"
    );
}

#[test]
fn a_block_that_is_there_is_rewritten_where_it_stands() {
    let was = "above\n\n\
               -- jobsdone: window (begin)\n\
               o.window(\"org.omarchy.jobsdone\", { float = true, center = true, size = { 870, 650 } })\n\
               -- jobsdone: window (end)\n\
               below\n";

    let now = with_block(was, &block(true, WindowSize::new(1200, 800)));

    assert_eq!(
        now,
        "above\n\n\
         -- jobsdone: window (begin)\n\
         o.window(\"org.omarchy.jobsdone\", { float = true, center = true, size = { 1200, 800 } })\n\
         -- jobsdone: window (end)\n\
         below\n"
    );
}

#[test]
fn a_tiled_window_leaves_the_markers_and_no_rule() {
    let was = with_block("", &floating());

    let now = with_block(&was, &block(false, WindowSize::default()));

    assert_eq!(
        now,
        "-- jobsdone: window (begin)\n-- jobsdone: window (end)\n"
    );
}
