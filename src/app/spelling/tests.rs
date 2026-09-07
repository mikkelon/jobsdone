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

/// What a checker with nothing behind it yet offers for one word.
fn offered(word: &str) -> Vec<String> {
    SpellChecker::default().suggestions(word)
}

/// The first of those, which is what a caller shows first.
fn best(word: &str) -> String {
    offered(word).into_iter().next().unwrap_or_default()
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

// ---- what is offered for a misspelling ----------------------------

#[test]
fn a_typo_is_offered_the_word_it_was_meant_to_be() {
    assert_eq!(best("teh"), "the");
    assert_eq!(best("recieved"), "received");
    assert_eq!(best("wierd"), "weird");
    assert_eq!(best("definately"), "definitely");
    assert_eq!(best("seperate"), "separate");
    assert_eq!(best("goverment"), "government");
}

#[test]
fn a_correction_further_down_the_list_is_still_in_it() {
    // The words nearest this one are other mis- words, so what was meant
    // is not what is ranked first; a caller reads the list, not the head
    // of it.
    assert!(offered("mispeling").contains(&"misspelling".to_string()));
}

#[test]
fn a_word_the_dictionary_has_is_offered_nothing() {
    assert!(offered("received").is_empty());
    assert!(offered("the").is_empty());
    assert!(offered("Berlin").is_empty());
    assert!(offered("don\u{2019}t").is_empty());
}

#[test]
fn nothing_is_offered_for_what_a_check_leaves_alone() {
    // A caller asks about the words a check underlined, and these are
    // not among them: the shapes `skipped` names, a word with no letter
    // in it, and no word at all.
    assert!(offered("getUserName").is_empty());
    assert!(offered("API").is_empty());
    assert!(offered("v2").is_empty());
    assert!(offered("\u{1f600}").is_empty());
    assert!(offered("").is_empty());
}

// ---- the dialect of what is offered -------------------------------

#[test]
fn a_british_spelling_is_offered_the_american_one() {
    assert_eq!(best("colour"), "color");
    assert_eq!(best("realise"), "realize");
    assert_eq!(best("theatre"), "theater");
    assert_eq!(best("aluminium"), "aluminum");
}

#[test]
fn no_spelling_from_another_dialect_is_offered() {
    // The words nearest these are the rest of their own paradigm:
    // `behaviours`, `behaviour's` and `behavioural` are each one edit
    // away, and every one of them is held for a writer this checker does
    // not serve.
    assert_eq!(offered("behaviour"), ["behavior", "behaviors"]);
    assert_eq!(offered("favourite"), ["favorite", "favorites"]);
    assert!(
        offered("colour")
            .iter()
            .all(|word| !word.contains("colour"))
    );
}

// ---- how what is offered is written -------------------------------

#[test]
fn a_capitalised_word_is_offered_capitalised_replacements() {
    assert_eq!(best("Teh"), "The");
    assert_eq!(best("Recieved"), "Received");
    assert!(
        offered("Wierd")
            .iter()
            .all(|word| word.starts_with(|letter: char| letter.is_uppercase()))
    );
}

#[test]
fn a_replacement_the_dictionary_capitalises_is_left_as_it_writes_it() {
    // The capital inside `TeX` is the word, so the front of it is not
    // the checker's to change; a name is offered for the same reason,
    // whichever way the word it replaces is written.
    assert!(offered("Teh").contains(&"TeX".to_string()));
    assert_eq!(best("berlin"), "Berlin");
}

// ---- normalisation ------------------------------------------------

#[test]
fn a_decomposed_word_is_offered_what_the_composed_one_is() {
    // A caller takes the word out of the note as the note writes it,
    // which is either of these.
    let composed = "caf\u{e9}teria";
    let decomposed = "cafe\u{301}teria";
    assert_ne!(composed, decomposed);
    assert_eq!(offered(composed), ["cafeteria", "cafeterias"]);
    assert_eq!(offered(decomposed), offered(composed));
}

#[test]
fn a_decomposed_word_the_dictionary_has_is_offered_nothing() {
    // Uncomposed, the mark is a letter of its own, and the word the
    // dictionary is asked about is not the word that was written.
    assert!(offered("resum\u{e9}").is_empty());
    assert!(offered("resume\u{301}").is_empty());
}

// ---- what a list of replacements is -------------------------------

#[test]
fn a_list_is_at_most_eight_long() {
    assert_eq!(offered("teh").len(), OFFERED);
    for word in ["recieved", "seperate", "Marck", "frnce", "hte"] {
        assert!(offered(word).len() <= OFFERED, "{word}");
    }
}

#[test]
fn a_list_holds_no_word_twice_and_never_the_word_itself() {
    for word in ["teh", "Teh", "colour", "Marck", "aprill", "youre", "hte"] {
        let list = offered(word);
        let mut once = list.clone();
        once.sort();
        once.dedup();
        assert_eq!(once.len(), list.len(), "{word}");
        assert!(!list.contains(&word.to_string()), "{word}");
    }
}

#[test]
fn a_word_longer_than_any_in_the_dictionary_is_offered_nothing() {
    let long: String = std::iter::repeat_n('q', LONGEST + 1).collect();
    assert!(offered(&long).is_empty());
}

#[test]
fn a_long_word_inside_the_bound_is_searched_like_any_other() {
    assert_eq!(best("responsibilites"), "responsibilities");
}

// ---- what a request costs -----------------------------------------

#[test]
fn a_check_does_not_build_the_fuzzy_dictionary() {
    // The finite-state map is worth its cost to a search and to nothing
    // else, so a session that only underlines words never pays for it.
    let mut checker = SpellChecker::default();
    checker.check("teh recieved");
    assert!(checker.fuzzy.is_none());
    checker.suggestions("teh");
    assert!(checker.fuzzy.is_some());
}

#[test]
fn a_word_with_nothing_to_correct_does_not_build_the_fuzzy_dictionary() {
    let mut checker = SpellChecker::default();
    assert!(checker.suggestions("received").is_empty());
    assert!(checker.suggestions("API").is_empty());
    assert!(checker.suggestions(&"q".repeat(LONGEST + 1)).is_empty());
    assert!(checker.fuzzy.is_none());
}

#[test]
fn a_replacement_takes_the_case_of_the_first_letter_of_the_word() {
    let chars = |word: &str| word.chars().collect::<Vec<char>>();
    assert_eq!(capitalised(&chars("Teh"), &chars("the")), "The");
    assert_eq!(capitalised(&chars("teh"), &chars("the")), "the");
    assert_eq!(capitalised(&chars("Teh"), &chars("TeX")), "TeX");
    assert_eq!(capitalised(&chars("berlin"), &chars("Berlin")), "Berlin");
}

// ---- a dictionary of one's own ------------------------------------

/// A personal dictionary of these words, keyed the way the domain keys
/// them and shown the way they are written here.
fn own(words: &[&str]) -> BTreeMap<String, String> {
    words
        .iter()
        .map(|word| (dictionary_key(word), (*word).to_owned()))
        .collect()
}

/// A checker holding those words.
fn holding(words: &[&str]) -> SpellChecker {
    let mut checker = SpellChecker::default();
    checker.set_personal_dictionary(&own(words));
    checker
}

/// The words a check finds when the writer has added `words` to their
/// own dictionary.
fn beside(words: &[&str], text: &str) -> Vec<String> {
    let clusters: Vec<&str> = text.graphemes(true).collect();
    holding(words)
        .check(text)
        .into_iter()
        .map(|found| clusters[found].concat())
        .collect()
}

#[test]
fn a_word_of_ones_own_is_not_a_misspelling() {
    assert_eq!(words("Qwertz called back"), ["Qwertz"]);
    assert!(beside(&["Qwertz"], "Qwertz called back").is_empty());
}

#[test]
fn a_word_of_ones_own_is_matched_whatever_case_it_is_written_in() {
    assert!(beside(&["Qwertz"], "qwertz and Qwertz").is_empty());
    assert!(beside(&["qwertz"], "qwertz and Qwertz").is_empty());
}

#[test]
fn a_word_of_ones_own_is_matched_composed_or_not() {
    // The same name written with one precomposed letter and with a
    // letter and a combining mark, either way round: the note in one
    // and the dictionary in the other.
    let composed = "Qw\u{eb}rtz";
    let decomposed = "Qwe\u{308}rtz";
    assert_eq!(words(&format!("ask {composed} first")), [composed]);
    assert!(beside(&[composed], &format!("ask {decomposed} first")).is_empty());
    assert!(beside(&[decomposed], &format!("ask {composed} first")).is_empty());
}

#[test]
fn a_hyphenated_name_of_ones_own_is_not_a_misspelling() {
    assert_eq!(words("met Qwertz-Asdfgh today"), ["Qwertz", "Asdfgh"]);
    assert!(beside(&["Qwertz-Asdfgh"], "met Qwertz-Asdfgh today").is_empty());
}

#[test]
fn a_hyphenated_name_does_not_accept_its_halves_on_their_own() {
    assert_eq!(beside(&["Qwertz-Asdfgh"], "met Qwertz today"), ["Qwertz"]);
}

#[test]
fn a_name_with_an_apostrophe_is_matched_whole() {
    assert_eq!(words("rang O'qwertz about it"), ["O'qwertz"]);
    assert!(beside(&["O'qwertz"], "rang O'qwertz about it").is_empty());
}

#[test]
fn a_word_of_ones_own_is_the_word_and_not_its_shapes() {
    // An ignore list, not a dictionary entry: nothing is inflected.
    assert_eq!(beside(&["Qwertz"], "the Qwertzs arrived"), ["Qwertzs"]);
}

#[test]
fn the_dictionary_still_rules_on_everything_else() {
    assert_eq!(
        beside(&["Qwertz"], "I recieved it from Qwertz"),
        ["recieved"]
    );
    assert_eq!(beside(&["Qwertz"], "the colour of it"), ["colour"]);
}

#[test]
fn a_word_of_ones_own_is_offered_nothing() {
    assert!(holding(&["Qwertz"]).suggestions("Qwertz").is_empty());
    assert!(holding(&["Qwertz"]).suggestions("qwertz").is_empty());
    // And a word that is not in the list is still corrected.
    assert_eq!(holding(&["Qwertz"]).suggestions("recieved")[0], "received");
}

#[test]
fn a_word_of_ones_own_builds_no_dictionary_to_be_left_alone() {
    let mut checker = holding(&["Qwertz"]);
    assert!(checker.dictionary.is_none());
    assert!(checker.suggestions("Qwertz").is_empty());
    assert!(checker.dictionary.is_none());
    assert!(checker.fuzzy.is_none());
}

#[test]
fn setting_the_same_words_again_changes_nothing() {
    let mut checker = SpellChecker::default();
    assert!(checker.set_personal_dictionary(&own(&["Qwertz"])));
    assert!(!checker.set_personal_dictionary(&own(&["Qwertz"])));
    assert!(!checker.set_personal_dictionary(&own(&["Qwertz"])));
}

#[test]
fn a_new_spelling_of_a_word_already_held_is_a_change() {
    let mut checker = holding(&["qwertz"]);
    assert!(checker.set_personal_dictionary(&own(&["Qwertz"])));
    assert!(checker.check("Qwertz called back").is_empty());
}

#[test]
fn a_text_that_did_not_change_is_checked_again_when_a_word_is_added() {
    let mut checker = SpellChecker::default();
    assert_eq!(pairs(&mut checker, "Qwertz called back"), [(0, 6)]);
    assert!(checker.set_personal_dictionary(&own(&["Qwertz"])));
    assert!(checker.check("Qwertz called back").is_empty());
}

#[test]
fn a_text_that_did_not_change_is_checked_again_when_a_word_is_taken_away() {
    let mut checker = holding(&["Qwertz"]);
    assert!(checker.check("Qwertz called back").is_empty());
    assert!(checker.set_personal_dictionary(&BTreeMap::new()));
    assert_eq!(pairs(&mut checker, "Qwertz called back"), [(0, 6)]);
}

#[test]
fn a_long_run_of_hyphens_is_read_no_further_than_a_name_can_be() {
    // Every word of the chain is still a misspelling, and each of them
    // walks a bounded stretch of it rather than the whole chain.
    let text = ["qwertz"; 60].join("-");
    assert_eq!(beside(&["Qwertz-Asdfgh"], &text).len(), 60);
    assert_eq!(words(&text).len(), 60);
}
