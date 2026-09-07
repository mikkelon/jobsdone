//! Spell checking for note bodies: text in, the ranges of the words the
//! dictionary does not know out, and, for one of those words, the
//! replacements to offer for it.
//!
//! The engine is Harper's (STACK.md section 10). This module uses three
//! pieces of it and nothing else: the `PlainEnglish` lexer, which splits
//! text into words, numbers, punctuation, URLs, email addresses and
//! hostnames; the curated dictionary, which is asked whether a word is a
//! word and whether it is one an American writes; and
//! `suggest_correct_spelling`, the fuzzy search and the ranking behind
//! Harper's own corrections. Harper's `SpellCheck` linter is deliberately
//! not used: it builds a `Document`, which runs a part-of-speech tagger
//! and a neural chunker over every sentence, to reach the same search
//! this module calls directly. Underlining a word and offering
//! replacements for one need no grammar.
//!
//! **Two dictionaries.** A check asks whether a word is in the
//! dictionary, which `MutableDictionary` answers as well as anything and
//! builds in half the time. The finite-state map of `FstDictionary` is
//! there to make fuzzy matching fast, and only a request for replacements
//! matches fuzzily, so it is built by the first such request and never by
//! a check. Both are values Harper holds for the life of the program, and
//! the second is built from the first.
//!
//! **Offsets.** Every range this module returns is in Unicode grapheme
//! clusters counted from the start of the text, end exclusive, which is
//! the unit a note's caret counts in. Harper counts in `char`s, so a
//! check converts once per text.
//!
//! **Normalisation.** A check works on a composed (NFC) copy of the text
//! when the text is not composed already, and a word a caller asks
//! replacements for is composed the same way. Harper's lexer ends a word
//! at a combining mark, so a word written as an `i` and a combining
//! diaeresis would otherwise arrive as two words and be called two
//! misspellings, while the same word written with one precomposed
//! letter is a word. Composing changes how many `char`s the text is and
//! cannot change where its grapheme clusters begin and end, which is why
//! the ranges still index the note as it is stored. The note itself is
//! never touched: the copy lives for the length of the check.
//!
//! **Cost.** [`SpellChecker::default`] does no work, and neither does a
//! check of empty text. The dictionary of some 135,000 words is built on
//! the first check that has something in it, and is then shared for the
//! life of the process: a second `SpellChecker` costs a pointer. Building
//! it was measured at 105 to 110 ms in a release build on the machine
//! this was written on, and it is paid once. Every check after it was
//! 2.5 us for a line of text and 14 us for a screenful, and a check whose
//! text has not changed since the last one is a string comparison at
//! around 12 ns.
//!
//! Replacements cost more. The first word a caller asks about builds the
//! finite-state map and the automaton the search runs it with, which
//! together were 200 ms, once. Of the twenty-seven words asked about
//! after that, every one the nearest search answered took 0.2 to 0.5 ms,
//! and the worst, which had to be searched twice because the nearest
//! search found nothing, took 1.4 ms. A word there is nothing to correct
//! searches for nothing and was under a microsecond.

use harper_core::parsers::{Parser, PlainEnglish};
use harper_core::spell::{Dictionary, FstDictionary, MutableDictionary, suggest_correct_spelling};
use harper_core::{CharStringExt, Dialect, Punctuation, Token, TokenKind};
use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;
use unicode_normalization::{UnicodeNormalization, is_nfc};
use unicode_segmentation::UnicodeSegmentation;

#[cfg(test)]
mod tests;

/// The dictionary and the answer it last gave.
///
/// Hold one per place that checks text. It is cheap to make and the
/// dictionary behind it is shared, so a second one costs a pointer.
#[derive(Default)]
pub struct SpellChecker {
    /// Built on the first check rather than at construction, so that
    /// nothing is paid for by a session that never opens a note.
    dictionary: Option<Arc<MutableDictionary>>,
    /// Built on the first word a caller asks replacements for, which is
    /// a question a session may never ask.
    fuzzy: Option<Arc<FstDictionary>>,
    /// The text the answer below was given for.
    checked: String,
    found: Vec<Range<usize>>,
}

/// How many replacements a caller is offered for one word. Enough for a
/// list to be worth reading and short enough to read.
const OFFERED: usize = 8;

/// How many words a search ranks before the dialect and the duplicates
/// are taken out of them, which is the number Harper's own spell checker
/// asks for.
const CANDIDATES: usize = 200;

/// The edit distances a search widens through, nearest first. A word two
/// letters out is only ranked against words three letters out where
/// nothing nearer was found, so a near miss is never buried under a
/// distant one.
///
/// It stops at three. Searching four deep needs an automaton that Harper
/// builds once per thread and that was measured at 490 ms to build, which
/// is half a second of a keystroke doing nothing, and what it buys is a
/// list of words no reader would recognise as what they meant.
const DISTANCES: Range<u8> = 2..4;

/// The longest word a search is run for. The longest entry in Harper's
/// dictionary is 25 characters, and two words are at least as many edits
/// apart as they are letters different in length, so nothing longer than
/// this can be within [`DISTANCES`] of an entry. A search would read the
/// whole dictionary to answer nothing.
const LONGEST: usize = 28;

impl SpellChecker {
    /// The words in `text` that the American English dictionary does not
    /// know, as half-open ranges of grapheme clusters, in the order they
    /// appear and none of them overlapping.
    ///
    /// Every word is reported, the one the caret is inside included; a
    /// caller that does not want to underline the word being typed
    /// drops it by comparing the caret with the range.
    ///
    /// Asking twice about the same text costs a string comparison and
    /// the copy of the answer.
    pub fn check(&mut self, text: &str) -> Vec<Range<usize>> {
        // Before the dictionary, so that a note opened empty does not
        // pay for it and a note opened with something in it does.
        if text.is_empty() {
            return Vec::new();
        }
        if self.dictionary.is_some() && self.checked == text {
            return self.found.clone();
        }

        let dictionary = self.warm().clone();

        let composed: Cow<str> = if is_nfc(text) {
            Cow::Borrowed(text)
        } else {
            Cow::Owned(text.nfc().collect())
        };
        let source: Vec<char> = composed.chars().collect();
        let tokens = PlainEnglish.parse(&source);
        let prose = Prose::of(&tokens, &source);
        let clusters = ClusterMap::of(&composed);

        self.found.clear();
        for (at, token) in tokens.iter().enumerate() {
            if !token.kind.is_word() {
                continue;
            }
            let word = &source[token.span.start..token.span.end];
            if skipped(word) || prose.code(at) {
                continue;
            }
            if known(dictionary.as_ref(), word) {
                continue;
            }
            self.found
                .push(clusters.at(token.span.start)..clusters.after(token.span.end));
        }

        self.checked.clear();
        self.checked.push_str(text);
        self.found.clone()
    }

    /// What to offer in place of `word`, best first, at most eight of
    /// them and no two of them the same.
    ///
    /// `word` is one word as the note writes it, which is what a caller
    /// holding a range from [`check`](Self::check) has: composed or not,
    /// capitalised or not. The answer is composed either way, and is
    /// written the way the word is where the dictionary leaves that
    /// open, so `Teh` at the start of a sentence is offered `The` and
    /// `teh` is offered `the`.
    ///
    /// Nothing comes back for a word there is nothing to correct: one
    /// the American dictionary holds as it is written, an identifier, an
    /// acronym, something with no letter in it, a word longer than any
    /// the dictionary holds, or nothing at all. A word held only for
    /// another dialect's writers is none of those, and is answered:
    /// `colour` is offered `color`.
    ///
    /// The first word that reaches the search pays for the finite-state
    /// map; the words after it do not.
    pub fn suggestions(&mut self, word: &str) -> Vec<String> {
        let composed: Vec<char> = word.nfc().collect();
        // What a check underlines is a word, so anything else that
        // arrives here is answered before a dictionary is asked: nothing
        // at all, something with no letter in it, an identifier, an
        // acronym, and a word longer than anything the dictionary holds.
        if !composed.iter().any(|letter| letter.is_alphabetic())
            || composed.len() > LONGEST
            || skipped(&composed)
        {
            return Vec::new();
        }
        // Before the fuzzy dictionary, so that a word there is nothing
        // to correct never builds it.
        let dictionary = self.warm().clone();
        if known(dictionary.as_ref(), &composed) {
            return Vec::new();
        }

        let fuzzy = self.fuzzy().clone();
        DISTANCES
            .map(|distance| ranked(&composed, distance, fuzzy.as_ref()))
            .find(|offered| !offered.is_empty())
            .unwrap_or_default()
    }

    /// Builds the dictionary if it is not built yet, and answers with
    /// it.
    ///
    /// The first call is the expensive one: what it builds is a value
    /// Harper holds for the life of the program, so every checker after
    /// the first is handed the one that is already there.
    fn warm(&mut self) -> &Arc<MutableDictionary> {
        self.dictionary
            .get_or_insert_with(MutableDictionary::curated)
    }

    /// Builds the fuzzy dictionary if it is not built yet, and answers
    /// with it.
    ///
    /// A check never asks for this. The finite-state map it builds is
    /// worth its cost to something that searches the dictionary for the
    /// words near a word, and answers a plain lookup no differently.
    fn fuzzy(&mut self) -> &Arc<FstDictionary> {
        self.fuzzy.get_or_insert_with(FstDictionary::curated)
    }
}

/// The words within `distance` edits of `word` that are worth offering,
/// in the order Harper ranks them.
///
/// Three things take a candidate out: the dialect, since a word held for
/// a British writer is not a correction here; the word itself, since
/// offering a writer what they already wrote replaces nothing; and a
/// spelling already offered, which two candidates can become once they
/// are written the way `word` is.
fn ranked(word: &[char], distance: u8, dictionary: &impl Dictionary) -> Vec<String> {
    let written: String = word.iter().collect();
    let mut offered: Vec<String> = Vec::new();
    for candidate in suggest_correct_spelling(word, CANDIDATES, distance, dictionary) {
        if !american(dictionary, candidate) {
            continue;
        }
        let replacement = capitalised(word, candidate);
        if replacement == written || offered.contains(&replacement) {
            continue;
        }
        offered.push(replacement);
        if offered.len() == OFFERED {
            break;
        }
    }
    offered
}

/// A replacement written the way the word it replaces is written.
///
/// A word that begins with a capital is replaced by one that begins with
/// a capital, which is what makes the `Teh` of a sentence `The`. A word
/// the dictionary capitalises somewhere other than the front is left as
/// the dictionary writes it, because that spelling is the word: `macOS`
/// is not `MacOS`, and `berlin` is corrected to `Berlin`.
fn capitalised(word: &[char], replacement: &[char]) -> String {
    let front = word.first().is_some_and(|first| first.is_uppercase());
    let inner = replacement
        .iter()
        .skip(1)
        .any(|letter| letter.is_uppercase());
    let mut letters = replacement.iter().copied();
    match letters.next() {
        Some(first) if front && !inner => first.to_uppercase().chain(letters).collect(),
        _ => replacement.iter().collect(),
    }
}

/// Whether the dictionary holds this word for an American.
///
/// Three things have to be true, which is the rule Harper's own spell
/// checker applies: the dictionary knows some capitalisation of the
/// word, the word is one the American dialect uses, and the dictionary
/// holds either the capitalisation written or the all-lowercase one. The
/// last is what makes `Berlin` right and `berlin` wrong, and the middle
/// one is what makes `colour` wrong where `color` is right.
fn known(dictionary: &impl Dictionary, word: &[char]) -> bool {
    american(dictionary, word)
        && (dictionary.contains_exact_word(word)
            || dictionary.contains_exact_word(&word.to_lower()))
}

/// Whether the dictionary knows some capitalisation of the word and
/// holds it for the American dialect.
fn american(dictionary: &impl Dictionary, word: &[char]) -> bool {
    dictionary
        .get_word_metadata(word)
        .is_some_and(|metadata| metadata.dialects.is_dialect_enabled(Dialect::American))
}

/// Whether a word is one no English dictionary should be asked about.
///
/// Two shapes, both of them things a dictionary cannot rule on and
/// neither of them a shape ordinary prose takes:
///
/// - A word with a digit in it: `v2`, `sha256`, `h264`. The lexer keeps
///   digits inside a word, so these arrive whole.
/// - A word with a capital letter anywhere but the first: `getUserName`,
///   `SQLite`, `macOS`, `API`, `TODO`. Identifiers, acronyms and product
///   names. A sentence's first word and an ordinary lowercase word both
///   keep their check, which is where the typos are.
fn skipped(word: &[char]) -> bool {
    word.iter().any(char::is_ascii_digit) || word.iter().skip(1).any(|c| c.is_uppercase())
}

/// The tokens of one text, and the questions about a word that need
/// more than the word itself.
///
/// Built once per check. The backtick regions are found in a single
/// pass, so asking about every word in turn costs one walk of the
/// tokens rather than one per word.
struct Prose<'a> {
    tokens: &'a [Token],
    source: &'a [char],
    /// Whether each token falls between a pair of backticks.
    quoted: Vec<bool>,
}

/// Punctuation that joins two words into one name: `src/app`,
/// `spell_check`, `spelling.rs`, `Self::check`, `FOO=bar`, a Windows
/// path. On its own a mark of this kind joins nothing, which is what
/// keeps the full stop of a sentence and the colon of a heading out of
/// it.
fn joins(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Punctuation(
            Punctuation::Underscore
                | Punctuation::ForwardSlash
                | Punctuation::Backslash
                | Punctuation::Period
                | Punctuation::Colon
                | Punctuation::Equal
        )
    )
}

/// Punctuation that marks what follows it as a name by itself: `@alice`,
/// `#release`. There is nothing on the other side to join to.
fn marks(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Punctuation(Punctuation::At | Punctuation::Hash)
    )
}

/// What can stand at the far end of a join and make it a name.
fn nameable(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Word(_)
            | TokenKind::Number(_)
            | TokenKind::Url
            | TokenKind::Hostname
            | TokenKind::EmailAddress
    )
}

impl<'a> Prose<'a> {
    fn of(tokens: &'a [Token], source: &'a [char]) -> Prose<'a> {
        let mut quoted = vec![false; tokens.len()];
        let ticks: Vec<usize> = tokens
            .iter()
            .enumerate()
            .filter(|(_, token)| {
                matches!(token.kind, TokenKind::Punctuation(Punctuation::Backtick))
            })
            .map(|(at, _)| at)
            .collect();
        // Pairs, so an unclosed backtick at the end quotes nothing and
        // does not swallow the rest of a note.
        for [open, close] in ticks.as_chunks::<2>().0 {
            for flag in &mut quoted[open + 1..*close] {
                *flag = true;
            }
        }
        Prose {
            tokens,
            source,
            quoted,
        }
    }

    /// Whether the word at `at` is part of something written as code
    /// rather than as prose.
    ///
    /// URLs, email addresses and hostnames need none of this: the lexer
    /// gives each of them a token of its own, which is not a word.
    fn code(&self, at: usize) -> bool {
        self.quoted[at] || self.joined(at) || self.marked(at) || self.parenthetical_plural(at)
    }

    /// Whether a name runs through this word: a joining mark against it
    /// with something nameable against the mark's other side. A run of
    /// marks counts as one join, which is what makes `Self::check` a
    /// name and `a::b::c` one throughout.
    fn joined(&self, at: usize) -> bool {
        self.reaches(at, -1) || self.reaches(at, 1)
    }

    /// Walks off the word at `at` in one direction over a run of joining
    /// marks that touch each other, and says whether something nameable
    /// touches the far end of the run.
    fn reaches(&self, at: usize, step: isize) -> bool {
        let mut here = at;
        let mut run = 0;
        loop {
            let Some(next) = here.checked_add_signed(step) else {
                return false;
            };
            let Some(token) = self.tokens.get(next) else {
                return false;
            };
            if !self.touching(here, next) {
                return false;
            }
            if joins(&token.kind) {
                here = next;
                run += 1;
                continue;
            }
            return run > 0 && nameable(&token.kind);
        }
    }

    /// Whether two neighbouring tokens have nothing between them.
    fn touching(&self, one: usize, other: usize) -> bool {
        let (left, right) = if one < other {
            (one, other)
        } else {
            (other, one)
        };
        self.tokens[left].span.end == self.tokens[right].span.start
    }

    /// Whether the word at `at` is preceded, with no space, by a mark
    /// that names what follows it.
    fn marked(&self, at: usize) -> bool {
        at.checked_sub(1)
            .is_some_and(|before| marks(&self.tokens[before].kind) && self.touching(before, at))
    }

    /// Whether the word at `at` is the `s` of `file(s)`, which is
    /// written this way on purpose and is not a word on its own.
    fn parenthetical_plural(&self, at: usize) -> bool {
        let word = self.tokens[at].span;
        let letters = &self.source[word.start..word.end];
        if letters != ['s'] && letters != ['S'] {
            return false;
        }
        let opens = at.checked_sub(1).is_some_and(|before| {
            matches!(
                self.tokens[before].kind,
                TokenKind::Punctuation(Punctuation::OpenRound)
            )
        });
        let closes = self.tokens.get(at + 1).is_some_and(|token| {
            matches!(token.kind, TokenKind::Punctuation(Punctuation::CloseRound))
        });
        opens && closes
    }
}

/// Where each grapheme cluster of a text starts, in `char`s, so that a
/// span Harper counted in `char`s can be given back in clusters.
///
/// One of these is built per check and thrown away with it. It holds the
/// char offset of every cluster, in order, which is what makes both
/// conversions a binary search.
struct ClusterMap {
    starts: Vec<usize>,
}

impl ClusterMap {
    fn of(text: &str) -> ClusterMap {
        let mut starts = Vec::new();
        let mut chars = 0;
        for cluster in text.graphemes(true) {
            starts.push(chars);
            chars += cluster.chars().count();
        }
        ClusterMap { starts }
    }

    /// The cluster the char at `offset` belongs to, which is where a
    /// word starts.
    fn at(&self, offset: usize) -> usize {
        self.starts
            .partition_point(|start| *start <= offset)
            .saturating_sub(1)
    }

    /// The cluster one past the char before `offset`, which is where a
    /// word ends. A word that ends inside a cluster, which a combining
    /// mark could make happen, rounds out to the whole cluster rather
    /// than cutting it.
    fn after(&self, offset: usize) -> usize {
        self.starts.partition_point(|start| *start < offset)
    }
}
