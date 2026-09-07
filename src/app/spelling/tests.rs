use super::*;

/// The words a check found, as the text of each one, which is what a
/// test can read. Ranges are in clusters, so the text is taken in
/// clusters too.
fn words(text: &str) -> Vec<String> {
    let mut checker = SpellChecker::default();
    let clusters: Vec<&str> = text.graphemes(true).collect();
    checker
        .check(text)
        .into_iter()
        .map(|found| clusters[found].concat())
        .collect()
}

/// The ranges a check found, as pairs, for the tests that are about the
/// offsets rather than about the words. Pairs rather than `Range`s
/// because a one-element array of `Range` reads as a typo to clippy.
fn ranges(text: &str) -> Vec<(usize, usize)> {
    pairs(&mut SpellChecker::default(), text)
}

/// The same of a checker a test is holding, so that asking twice can be
/// told from asking once.
fn pairs(checker: &mut SpellChecker, text: &str) -> Vec<(usize, usize)> {
    checker
        .check(text)
        .into_iter()
        .map(|found| (found.start, found.end))
        .collect()
}

// ---- what is a misspelling ---------------------------------------

#[test]
fn a_misspelled_word_is_found() {
    assert_eq!(words("I recieved the parcel"), ["recieved"]);
}

#[test]
fn several_misspellings_come_back_in_order() {
    assert_eq!(words("teh quick brown fox jmups over it"), ["teh", "jmups"]);
}

#[test]
fn ordinary_prose_is_clean() {
    assert!(words("Ask the landlord about the boiler before Friday.").is_empty());
    assert!(words("Send the invoice, then chase the reply.").is_empty());
}

#[test]
fn an_empty_text_has_nothing_in_it() {
    assert!(words("").is_empty());
    assert!(words("   \n\n  ").is_empty());
}

// ---- the dialect --------------------------------------------------

#[test]
fn american_spelling_is_right() {
    assert!(words("the color of the theater program").is_empty());
    assert!(words("I realize we have to organize the catalog").is_empty());
}

#[test]
fn british_spelling_is_wrong() {
    assert_eq!(words("the colour of it"), ["colour"]);
    assert_eq!(words("book the theatre"), ["theatre"]);
    assert_eq!(words("I realise it now"), ["realise"]);
}

// ---- contractions -------------------------------------------------

#[test]
fn contractions_are_words() {
    assert!(words("I don't think it's ready, we'll wait").is_empty());
    assert!(words("they're here and you've seen them").is_empty());
    assert!(words("I can't, shouldn't and wouldn't").is_empty());
}

#[test]
fn a_curly_apostrophe_is_an_apostrophe_too() {
    assert!(words("I don\u{2019}t think it\u{2019}s ready").is_empty());
}

#[test]
fn a_possessive_is_a_word() {
    assert!(words("the landlord's boiler").is_empty());
}

// ---- offsets ------------------------------------------------------

#[test]
fn a_range_covers_exactly_the_word() {
    // "I recieved" - the word starts at cluster 2 and is 8 long.
    assert_eq!(ranges("I recieved it"), [(2, 10)]);
}

#[test]
fn offsets_are_clusters_not_bytes() {
    // "cafe\u{e9}" is four clusters, four chars and five bytes, so a
    // range counted in bytes would start the next word one late.
    let text = "caf\u{e9} recieved";
    assert_eq!(text.len(), 14);
    assert_eq!(ranges(text), [(5, 13)]);
}

#[test]
fn a_combining_mark_is_one_cluster() {
    // The same word written as an "e" and a combining acute is one char
    // longer and the same four clusters, so the range does not move.
    let text = "cafe\u{301} recieved";
    assert_eq!(text.chars().count(), 14);
    assert_eq!(ranges(text), [(5, 13)]);
}

#[test]
fn a_decomposed_word_reads_as_the_composed_one() {
    // Harper's lexer ends a word at a combining mark, so without the
    // composing step this arrives as "nai" and "ve" and is two
    // misspellings. It is one word either way it is written.
    let composed = "na\u{ef}ve";
    let decomposed = "nai\u{308}ve";
    assert_ne!(composed, decomposed);
    assert_eq!(
        composed.nfc().collect::<String>(),
        decomposed.nfc().collect::<String>()
    );
    assert!(ranges(composed).is_empty());
    assert!(ranges(decomposed).is_empty());
}

#[test]
fn composing_does_not_move_the_words_after_it() {
    // The two forms are five clusters and four or five chars, so a
    // range counted in chars would move; counted in clusters it does
    // not, and the same range indexes both notes.
    let composed = "na\u{ef}ve recieved";
    let decomposed = "nai\u{308}ve recieved";
    assert_eq!(composed.chars().count(), 14);
    assert_eq!(decomposed.chars().count(), 15);
    assert_eq!(composed.graphemes(true).count(), 14);
    assert_eq!(decomposed.graphemes(true).count(), 14);
    assert_eq!(ranges(composed), [(6, 14)]);
    assert_eq!(ranges(decomposed), [(6, 14)]);
}

#[test]
fn a_range_indexes_the_note_as_it_is_stored() {
    // The clusters are taken from the decomposed text, which is what a
    // note holds; the check was made on a composed copy of it.
    let text = "nai\u{308}ve but recieved";
    let clusters: Vec<&str> = text.graphemes(true).collect();
    let found = ranges(text);
    assert_eq!(found.len(), 1);
    assert_eq!(clusters[found[0].0..found[0].1].concat(), "recieved");
}

#[test]
fn a_misspelling_with_a_combining_mark_covers_whole_clusters() {
    // Composing leaves this one alone: there is no precomposed letter
    // for a "z" with an acute, so the word still ends at the mark. What
    // matters is that the ranges are whole clusters, that they touch,
    // and that they do not overlap.
    let text = "recq\u{301}ved";
    assert_eq!(text.nfc().collect::<String>(), text);
    assert_eq!(text.chars().count(), 8);
    assert_eq!(text.graphemes(true).count(), 7);
    assert_eq!(ranges(text), [(0, 4), (4, 7)]);
}

#[test]
fn an_emoji_counts_as_one() {
    // A family emoji is one cluster and five chars.
    let text = "\u{1f469}\u{200d}\u{1f469}\u{200d}\u{1f467} recieved";
    assert_eq!(text.chars().count(), 14);
    assert_eq!(ranges(text), [(2, 10)]);
}

#[test]
fn ranges_index_the_clusters_of_the_text() {
    let text = "the caf\u{e9} was recieved";
    let clusters: Vec<&str> = text.graphemes(true).collect();
    let found = ranges(text);
    assert_eq!(found.len(), 1);
    assert_eq!(clusters[found[0].0..found[0].1].concat(), "recieved");
}

// ---- what is skipped ----------------------------------------------

#[test]
fn a_url_is_left_alone() {
    assert!(words("see https://github.com/mikkelon/jobsdone for it").is_empty());
    assert!(words("http://localhost:8080/qwertz").is_empty());
}

#[test]
fn an_email_address_is_left_alone() {
    assert!(words("write to mikkel@overgaard.dev today").is_empty());
}

#[test]
fn a_hostname_is_left_alone() {
    assert!(words("deploy to jobsdone.example.com tonight").is_empty());
}

#[test]
fn code_shaped_tokens_are_left_alone() {
    assert!(words("open src/app/spelling.rs and read it").is_empty());
    assert!(words("the spell_check helper").is_empty());
    assert!(words("call SpellChecker::check on it").is_empty());
    assert!(words("ask @alice about it").is_empty());
    assert!(words("set FOO=bar first").is_empty());
}

#[test]
fn a_word_with_a_digit_in_it_is_left_alone() {
    assert!(words("upgrade to v2 and h264").is_empty());
}

#[test]
fn an_acronym_is_left_alone() {
    assert!(words("the API and the SDK, and a TODO").is_empty());
}

#[test]
fn a_camel_case_identifier_is_left_alone() {
    assert!(words("getUserName reads from macOS").is_empty());
}

#[test]
fn backticked_text_is_left_alone() {
    assert!(words("run `qwertz asdfgh` and wait").is_empty());
}

#[test]
fn an_unclosed_backtick_does_not_swallow_the_rest() {
    assert_eq!(words("a `qwertz and then jmups"), ["qwertz", "jmups"]);
}

#[test]
fn the_s_of_a_parenthetical_plural_is_left_alone() {
    assert!(words("send the file(s) today").is_empty());
}

#[test]
fn skipping_does_not_reach_the_prose_beside_it() {
    // The URL and the identifier go; the typo between them stays.
    assert_eq!(
        words("see https://example.com for the recieved src/app note"),
        ["recieved"]
    );
}

#[test]
fn a_sentence_full_stop_joins_nothing() {
    // A mark with nothing on its other side joins nothing, so a typo at
    // the end of a sentence is still a typo.
    assert_eq!(words("It is teh end. Then jmups."), ["teh", "jmups"]);
    assert_eq!(words("jmups."), ["jmups"]);
    assert_eq!(words("Was it jmups?"), ["jmups"]);
}

#[test]
fn a_heading_colon_joins_nothing() {
    assert_eq!(words("jmups: the whole story"), ["jmups"]);
    assert_eq!(words("Monday - jmups, then rest"), ["jmups"]);
}

#[test]
fn a_lone_mark_beside_a_word_joins_nothing() {
    assert_eq!(words("/jmups"), ["jmups"]);
    assert_eq!(words("jmups/"), ["jmups"]);
    assert_eq!(words("(jmups)"), ["jmups"]);
    assert_eq!(words("\"jmups\""), ["jmups"]);
}

#[test]
fn a_dotted_pair_is_a_name_throughout() {
    // Both halves go, and a run of marks counts as one join.
    assert!(words("read qwertz.asdfgh today").is_empty());
    assert!(words("call qwertz::asdfgh now").is_empty());
    assert!(words("open qwertz/asdfgh/zxcvbn there").is_empty());
}

#[test]
fn a_hyphenated_word_is_still_checked() {
    assert_eq!(words("a well-knwon problem"), ["knwon"]);
}

// ---- the cache ----------------------------------------------------

#[test]
fn the_same_text_twice_gives_the_same_answer() {
    let mut checker = SpellChecker::default();
    let first = pairs(&mut checker, "I recieved it");
    let second = pairs(&mut checker, "I recieved it");
    assert_eq!(first, second);
    assert_eq!(first, [(2, 10)]);
}

#[test]
fn a_changed_text_is_checked_again() {
    let mut checker = SpellChecker::default();
    assert_eq!(pairs(&mut checker, "I recieved it"), [(2, 10)]);
    assert!(checker.check("I received it").is_empty());
    assert_eq!(pairs(&mut checker, "I recieved it"), [(2, 10)]);
}

#[test]
fn an_empty_text_is_cached_like_any_other() {
    let mut checker = SpellChecker::default();
    assert!(checker.check("").is_empty());
    assert!(checker.check("").is_empty());
    assert_eq!(pairs(&mut checker, "jmups"), [(0, 5)]);
}

// ---- the pieces ---------------------------------------------------

#[test]
fn a_cluster_map_of_plain_text_is_the_identity() {
    let map = ClusterMap::of("abc");
    assert_eq!((map.at(0), map.after(3)), (0, 3));
    assert_eq!((map.at(1), map.after(2)), (1, 2));
}

#[test]
fn a_cluster_map_counts_a_combining_mark_once() {
    // "e" + combining acute, then "b": three chars, two clusters.
    let map = ClusterMap::of("e\u{301}b");
    assert_eq!(map.starts, [0, 2]);
    assert_eq!((map.at(0), map.after(2)), (0, 1));
    assert_eq!((map.at(2), map.after(3)), (1, 2));
    // A span that ends inside a cluster rounds out to the whole cluster.
    assert_eq!(map.after(1), 1);
}

#[test]
fn an_empty_cluster_map_answers_zero() {
    let map = ClusterMap::of("");
    assert_eq!((map.at(0), map.after(0)), (0, 0));
}

#[test]
fn a_digit_or_an_inner_capital_makes_a_word_skipped() {
    let chars = |word: &str| word.chars().collect::<Vec<char>>();
    assert!(skipped(&chars("v2")));
    assert!(skipped(&chars("getUserName")));
    assert!(skipped(&chars("API")));
    assert!(!skipped(&chars("Friday")));
    assert!(!skipped(&chars("boiler")));
}
