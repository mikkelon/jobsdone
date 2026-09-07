//! Spell checking for note bodies: text in, the ranges of the words the
//! dictionary does not know out.
//!
//! The engine is Harper's (STACK.md section 10). This module uses two
//! pieces of it and nothing else: the `PlainEnglish` lexer, which splits
//! text into words, numbers, punctuation, URLs, email addresses and
//! hostnames, and the curated dictionary, which is asked whether a word
//! is a word and whether it is one an American writes. Harper's own
//! `SpellCheck` linter is deliberately not used: it builds a `Document`,
//! which runs a part-of-speech tagger and a neural chunker over every
//! sentence, and it fuzzy-searches the dictionary for corrections to put
//! in a message. Underlining a word needs neither. The dictionary is
//! `MutableDictionary` rather than `FstDictionary` for the same reason:
//! the finite-state map exists to make fuzzy matching fast, and nothing
//! here matches fuzzily. Building it would double the cost below and
//! answer every question this module asks identically.
//!
//! **Offsets.** Every range this module returns is in Unicode grapheme
//! clusters counted from the start of the text, end exclusive, which is
//! the unit a note's caret counts in. Harper counts in `char`s, so a
//! check converts once per text.
//!
//! **Normalisation.** A check works on a composed (NFC) copy of the text
//! when the text is not composed already. Harper's lexer ends a word at
//! a combining mark, so a word written as an `i` and a combining
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

use harper_core::parsers::{Parser, PlainEnglish};
use harper_core::spell::{Dictionary, MutableDictionary};
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
    /// The text the answer below was given for.
    checked: String,
    found: Vec<Range<usize>>,
}

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

    /// Builds the dictionary if it is not built yet, and answers with
    /// it.
    ///
    /// The first call is the expensive one and the only expensive one in
    /// the process: what it builds is a value Harper holds for the life
    /// of the program, so every checker after the first is handed the
    /// one that is already there.
    fn warm(&mut self) -> &Arc<MutableDictionary> {
        self.dictionary
            .get_or_insert_with(MutableDictionary::curated)
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
    let Some(metadata) = dictionary.get_word_metadata(word) else {
        return false;
    };
    metadata.dialects.is_dialect_enabled(Dialect::American)
        && (dictionary.contains_exact_word(word)
            || dictionary.contains_exact_word(&word.to_lower()))
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
