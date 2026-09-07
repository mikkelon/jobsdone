//! The personal dictionary: the words this person has told the program
//! are spelt right, and the rules for what may go in it.
//!
//! An entry is one word, held under a canonical key so that the same word
//! in two capitalisations is one entry. The key is what a checker asks
//! with; the word is what was typed, which is what the manager shows.
//! Like the settings, a change here is not undoable and puts nothing on
//! the undo stack.

use std::collections::BTreeMap;

use unicode_normalization::UnicodeNormalization;

use super::Rejected;
use super::model::{Change, Model, diff};

/// The longest word the dictionary takes. Past this it is a slip of the
/// keyboard rather than a word.
const MOST: usize = 128;

/// The key an entry is held under: the same word in any capitalisation,
/// and in either Unicode composition, comes out as one string.
///
/// Composing first means the lowercasing sees whole characters; composing
/// again settles the few whose lowercase form decomposes.
pub fn dictionary_key(word: &str) -> String {
    word.nfc()
        .collect::<String>()
        .to_lowercase()
        .nfc()
        .collect()
}

/// A word added to the personal dictionary, as it was typed.
pub fn add_dictionary_word(model: &Model, word: &str) -> Result<Change, Rejected> {
    let word = valid_word(word)?;
    let key = dictionary_key(&word);
    if model.personal_dictionary.contains_key(&key) {
        return Err(already());
    }
    Ok(written(model, |dictionary| {
        dictionary.insert(key, word);
    }))
}

/// The entry under `old_key`, rewritten. The word may change its
/// capitalisation, which keeps the same key, or become another word
/// entirely, which moves the entry to a new one.
pub fn edit_dictionary_word(model: &Model, old_key: &str, word: &str) -> Result<Change, Rejected> {
    let old_key = dictionary_key(old_key);
    if !model.personal_dictionary.contains_key(&old_key) {
        return Err(missing());
    }
    let word = valid_word(word)?;
    let key = dictionary_key(&word);
    if key != old_key && model.personal_dictionary.contains_key(&key) {
        return Err(already());
    }
    Ok(written(model, |dictionary| {
        dictionary.remove(&old_key);
        dictionary.insert(key, word);
    }))
}

/// The entry under `key`, taken out of the dictionary.
pub fn remove_dictionary_word(model: &Model, key: &str) -> Result<Change, Rejected> {
    let key = dictionary_key(key);
    if !model.personal_dictionary.contains_key(&key) {
        return Err(missing());
    }
    Ok(written(model, |dictionary| {
        dictionary.remove(&key);
    }))
}

/// The writes that get the dictionary from where it is to where the edit
/// leaves it, which is one row apiece: two windows adding two words touch
/// two rows and neither loses the other's.
fn written(model: &Model, edit: impl FnOnce(&mut BTreeMap<String, String>)) -> Change {
    let mut after = model.clone();
    edit(&mut after.personal_dictionary);
    Change {
        writes: diff(model, &after),
    }
}

/// An entry is one word: the outer whitespace goes, and what is left has
/// to be a word rather than a phrase, a punctuation mark or a stretch of
/// something a terminal cannot draw.
fn valid_word(text: &str) -> Result<String, Rejected> {
    let word: String = text.trim().nfc().collect();
    if word.is_empty() {
        return Err(Rejected("A dictionary word cannot be blank.".to_owned()));
    }
    if word.chars().any(char::is_whitespace) {
        return Err(Rejected("A dictionary word is one word.".to_owned()));
    }
    if word.chars().any(char::is_control) {
        return Err(Rejected(
            "A word cannot have control characters in it.".to_owned(),
        ));
    }
    if !word.chars().any(char::is_alphabetic) {
        return Err(Rejected("A word needs a letter in it.".to_owned()));
    }
    if word.chars().count() > MOST {
        return Err(Rejected(format!("A word is at most {MOST} characters.")));
    }
    Ok(word)
}

fn already() -> Rejected {
    Rejected("That word is already in your dictionary.".to_owned())
}

fn missing() -> Rejected {
    Rejected("That word is not in your dictionary.".to_owned())
}
