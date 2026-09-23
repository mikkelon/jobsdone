//! Fuzzy matching: whether a query is in a text as scattered letters,
//! and how well it fits.
//!
//! A query is split into words on whitespace, and every word has to be
//! in the text as a subsequence: its letters in order, not necessarily
//! together. A word is scored on the best way its letters can be found,
//! and the words' scores are added. A letter at the start of a word in
//! the text scores more, so does one right after the letter before it,
//! and a gap between two letters costs a little for every character it
//! skips. Matching ignores case and Unicode composition, the way the
//! personal dictionary does. There is no query syntax.

use std::cmp::Reverse;

use super::dictionary_key;

/// How well a query fits a text. Higher is better; scores are only ever
/// compared with each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Score(i64);

/// What every matched letter is worth.
const LETTER: i64 = 16;
/// Extra for a letter at the start of a word in the text.
const WORD_START: i64 = 8;
/// Extra for a letter right after the one before it.
const RUN: i64 = 6;
/// What a gap between two matched letters costs, and what each further
/// character of it costs.
const GAP_OPEN: i64 = 3;
const GAP_EXTEND: i64 = 1;

/// A query made ready to be matched against many texts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    words: Vec<Vec<char>>,
}

impl Query {
    pub fn new(query: &str) -> Query {
        Query {
            words: dictionary_key(query)
                .split_whitespace()
                .map(|word| word.chars().collect())
                .collect(),
        }
    }

    /// A query with no words matches everything equally.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// How well the query fits the text, or `None` when one of its words
    /// is not in it.
    pub fn score(&self, text: &str) -> Option<Score> {
        if self.is_empty() {
            return Some(Score(0));
        }
        let text: Vec<char> = dictionary_key(text).chars().collect();
        let starts: Vec<bool> = (0..text.len())
            .map(|at| at == 0 || !text[at - 1].is_alphanumeric())
            .collect();
        let mut total = 0;
        for word in &self.words {
            total += best_alignment(word, &text, &starts)?;
        }
        Some(Score(total))
    }
}

/// How well a query fits a text, or `None` when it does not.
pub fn score(query: &str, text: &str) -> Option<Score> {
    Query::new(query).score(text)
}

/// The ids of the texts the query matches, best first. Texts that score
/// the same keep the order they were given in, so an empty query gives
/// back every id in its original order.
pub fn rank<T, S: AsRef<str>>(query: &str, items: impl IntoIterator<Item = (T, S)>) -> Vec<T> {
    let query = Query::new(query);
    let mut matched: Vec<(Score, T)> = items
        .into_iter()
        .filter_map(|(id, text)| query.score(text.as_ref()).map(|score| (score, id)))
        .collect();
    matched.sort_by_key(|(score, _)| Reverse(*score));
    matched.into_iter().map(|(_, id)| id).collect()
}

/// The best score of one word found as a subsequence of the text.
///
/// `ends[i]` is the best score of the letters of the word so far with
/// the last of them at `text[i]`. For the next letter, the best way to
/// reach `i` is straight from `i - 1`, with the run bonus, or from any
/// earlier position across a gap; `across` carries the best of those as
/// `i` moves right, paying one more `GAP_EXTEND` for each step.
fn best_alignment(word: &[char], text: &[char], starts: &[bool]) -> Option<i64> {
    const NONE: i64 = i64::MIN / 4;
    let worth = |at: usize| LETTER + if starts[at] { WORD_START } else { 0 };

    let mut ends: Vec<i64> = text
        .iter()
        .enumerate()
        .map(|(at, c)| if *c == word[0] { worth(at) } else { NONE })
        .collect();

    for letter in &word[1..] {
        let mut next = vec![NONE; text.len()];
        let mut across = NONE;
        for at in 1..text.len() {
            if at >= 2 {
                across = (across - GAP_EXTEND).max(ends[at - 2] - GAP_OPEN);
            }
            if text[at] == *letter {
                let before = (ends[at - 1] + RUN).max(across);
                if before > NONE / 2 {
                    next[at] = before + worth(at);
                }
            }
        }
        ends = next;
    }
    ends.into_iter().filter(|score| *score > NONE / 2).max()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranked<'a>(query: &str, texts: &[&'a str]) -> Vec<&'a str> {
        rank(query, texts.iter().map(|text| (*text, *text)))
    }

    #[test]
    fn every_letter_has_to_be_there_in_order() {
        assert!(score("mlk", "Milk").is_some());
        assert!(score("klm", "Milk").is_none());
        assert!(score("milks", "Milk").is_none());
    }

    #[test]
    fn every_word_of_the_query_has_to_match() {
        assert!(score("anna budget", "Mention to Anna: CI runner budget").is_some());
        assert!(score("budget anna", "Mention to Anna: CI runner budget").is_some());
        assert!(score("anna invoice", "Mention to Anna: CI runner budget").is_none());
    }

    #[test]
    fn case_and_composition_do_not_matter() {
        assert!(score("MILK", "milk").is_some());
        assert!(score("café", "CAFE\u{301}").is_some());
        assert!(score("CAFE\u{301}", "le café").is_some());
    }

    #[test]
    fn an_empty_query_matches_everything_in_the_order_given() {
        assert_eq!(score("  ", "anything"), Some(Score(0)));
        assert_eq!(
            ranked("", &["Milk", "Bread", "Eggs"]),
            ["Milk", "Bread", "Eggs"]
        );
    }

    #[test]
    fn letters_together_rank_above_letters_scattered() {
        assert_eq!(
            ranked("run", &["rain until noon", "CI runner"]),
            ["CI runner", "rain until noon"]
        );
    }

    #[test]
    fn letters_at_word_starts_rank_above_letters_inside_words() {
        assert_eq!(
            ranked("ci", &["Specific", "CI runner budget"]),
            ["CI runner budget", "Specific"]
        );
        assert_eq!(
            ranked("mt", &["Summit", "Monthly team call"]),
            ["Monthly team call", "Summit"]
        );
    }

    #[test]
    fn a_shorter_gap_ranks_above_a_longer_one() {
        assert_eq!(ranked("ab", &["a------b", "a--b"]), ["a--b", "a------b"]);
    }

    #[test]
    fn the_best_alignment_is_found_not_the_first() {
        // The first "b" is inside a word and far away; the later one
        // starts a word right after the "a".
        assert_eq!(score("ab", "a xxbxx a b"), score("ab", "a b"));
    }

    #[test]
    fn ties_keep_the_order_they_were_given_in() {
        assert_eq!(
            ranked("milk", &["milk", "Milk", "MILK"]),
            ["milk", "Milk", "MILK"]
        );
    }

    #[test]
    fn texts_that_do_not_match_are_left_out() {
        assert_eq!(ranked("egg", &["Milk", "Eggs", "Bread"]), ["Eggs"]);
    }
}
