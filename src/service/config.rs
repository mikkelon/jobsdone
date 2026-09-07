//! The settings and the personal dictionary.
//!
//! Neither is undoable and neither pushes an entry, the way the domain
//! has it (DOMAIN.md sections 19 and 20).
//!
//! A value out of range is **refused** here rather than held to the
//! range. The codec that reads the settings table is deliberately
//! forgiving, because a row written by another build has to be readable;
//! a request is somebody saying what they want, and 48 for the hour a
//! day starts at is a mistake worth being told about rather than a
//! quiet 23.
//!
//! Nothing here touches a window manager. A change to `floating_window`
//! or `window_size` is saved like any other setting and reported, so the
//! caller can say that `jobsdone desktop` is what hands it to Hyprland.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{Error, Session, order_name, spec};
use crate::domain::{self, DateStyle, Settings, WeekStart, Weekday, WindowSize, WorkDays};

pub(super) fn settings_get(session: &Session, fields: Value) -> Result<Value, Error> {
    let _: spec::Nothing = spec::fields("settings.get", fields)?;
    Ok(answer(session))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Set {
    settings: Map<String, Value>,
}

pub(super) fn settings_set(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: Set = spec::fields("settings.set", fields)?;
    if request.settings.is_empty() {
        return Err(Error::invalid_argument("Name at least one setting to set."));
    }

    let was = session.model().settings.clone();
    let settings = patched(&was, &request.settings)?;
    let change = domain::change_settings(session.model(), settings).map_err(Error::rejected)?;
    session.commit_change(&change)?;

    let now = &session.model().settings;
    let window_rule_changed =
        now.floating_window() != was.floating_window() || now.window_size() != was.window_size();

    let mut data = answer(session);
    if let Some(object) = data.as_object_mut() {
        object.insert(
            "desktop".to_owned(),
            json!({
                "window_rule_changed": window_rule_changed,
                "floating_window": now.floating_window(),
                "window_size": {"width": now.window_size().width, "height": now.window_size().height},
            }),
        );
        object.insert("undo".to_owned(), Value::Null);
    }
    Ok(data)
}

fn answer(session: &Session) -> Value {
    json!({
        "settings": written(&session.model().settings),
        "date_order": order_name(session.dates()),
    })
}

/// Every setting as a typed value.
fn written(settings: &Settings) -> Value {
    json!({
        "day_starts_at": settings.day_starts_at(),
        "week_starts_on": match settings.week_starts_on() {
            WeekStart::Monday => "monday",
            WeekStart::Sunday => "sunday",
        },
        "work_days": Value::Array(settings.work_days().iter()
            .map(|day| Value::String(spec::weekday_name(day).to_owned())).collect()),
        "review_opens_itself": settings.review_opens_itself(),
        "due_ahead_days": settings.due_ahead_days(),
        "backfill_days": settings.backfill_days(),
        "pile_horizon_days": settings.pile_horizon_days(),
        "floating_window": settings.floating_window(),
        "window_size": {
            "width": settings.window_size().width,
            "height": settings.window_size().height,
        },
        "mouse": settings.mouse(),
        "message_seconds": settings.message_seconds(),
        "date_style": match settings.date_style() {
            DateStyle::Locale => "locale",
            DateStyle::DayFirst => "day_first",
            DateStyle::MonthFirst => "month_first",
        },
        "confirm_delete": settings.confirm_delete(),
        "spell_check_notes": settings.spell_check_notes(),
    })
}

/// The keys a request may name, which are the keys of the table.
const KEYS: [&str; 14] = [
    "day_starts_at",
    "week_starts_on",
    "work_days",
    "review_opens_itself",
    "due_ahead_days",
    "backfill_days",
    "pile_horizon_days",
    "floating_window",
    "window_size",
    "mouse",
    "message_seconds",
    "date_style",
    "confirm_delete",
    "spell_check_notes",
];

/// The settings with the named ones changed, each held to its range by
/// refusing anything outside it.
fn patched(settings: &Settings, patch: &Map<String, Value>) -> Result<Settings, Error> {
    spec::only(patch, &KEYS, "The settings table")?;
    let mut settings = settings.clone();

    for (key, value) in patch {
        match key.as_str() {
            "day_starts_at" => settings.set_day_starts_at(number(key, value, 0, 23)?),
            "week_starts_on" => settings.set_week_starts_on(match word(key, value)? {
                "monday" => WeekStart::Monday,
                "sunday" => WeekStart::Sunday,
                other => {
                    return Err(one_of(key, other, &["monday", "sunday"]));
                }
            }),
            "work_days" => settings.set_work_days(work_days(value)?),
            "review_opens_itself" => settings.set_review_opens_itself(flag(key, value)?),
            "due_ahead_days" => settings.set_due_ahead_days(number(key, value, 0, 365)?),
            "backfill_days" => settings.set_backfill_days(number(key, value, 0, 365)?),
            "pile_horizon_days" => settings.set_pile_horizon_days(number(key, value, 0, 3650)?),
            "floating_window" => settings.set_floating_window(flag(key, value)?),
            "window_size" => settings.set_window_size(window_size(value)?),
            "mouse" => settings.set_mouse(flag(key, value)?),
            "message_seconds" => settings.set_message_seconds(number(key, value, 0, 60)?),
            "date_style" => settings.set_date_style(match word(key, value)? {
                "locale" => DateStyle::Locale,
                "day_first" => DateStyle::DayFirst,
                "month_first" => DateStyle::MonthFirst,
                other => {
                    return Err(one_of(key, other, &["locale", "day_first", "month_first"]));
                }
            }),
            "confirm_delete" => settings.set_confirm_delete(flag(key, value)?),
            "spell_check_notes" => settings.set_spell_check_notes(flag(key, value)?),
            _ => {}
        }
    }
    Ok(settings)
}

fn number(key: &str, value: &Value, least: i64, most: i64) -> Result<i64, Error> {
    match value.as_i64() {
        Some(number) if (least..=most).contains(&number) => Ok(number),
        _ => Err(Error::invalid_argument(format!(
            "`{key}` is a whole number from {least} to {most}."
        ))),
    }
}

fn flag(key: &str, value: &Value) -> Result<bool, Error> {
    value
        .as_bool()
        .ok_or_else(|| Error::invalid_argument(format!("`{key}` is true or false.")))
}

fn word<'a>(key: &str, value: &'a Value) -> Result<&'a str, Error> {
    value
        .as_str()
        .ok_or_else(|| Error::invalid_argument(format!("`{key}` is written as a string.")))
}

fn one_of(key: &str, given: &str, allowed: &[&str]) -> Error {
    Error::invalid_argument(format!(
        "`{key}` is one of {}, not {given:?}.",
        allowed.join(", ")
    ))
}

fn work_days(value: &Value) -> Result<WorkDays, Error> {
    let Some(names) = value.as_array() else {
        return Err(Error::invalid_argument(
            "`work_days` is a list of mon tue wed thu fri sat sun.",
        ));
    };
    let mut seen = BTreeSet::new();
    for name in names {
        let text = name.as_str().unwrap_or_default().to_ascii_lowercase();
        let day = Weekday::ALL
            .into_iter()
            .find(|day| spec::weekday_name(*day) == text)
            .ok_or_else(|| {
                Error::invalid_argument(format!(
                    "{name} is not a weekday. Write mon tue wed thu fri sat sun."
                ))
            })?;
        if !seen.insert(day) {
            return Err(Error::invalid_argument(format!(
                "`work_days` names {text} once."
            )));
        }
    }
    if seen.is_empty() {
        return Err(Error::invalid_argument(
            "At least one day of the week must be a work day.",
        ));
    }
    Ok(WorkDays::of(seen))
}

fn window_size(value: &Value) -> Result<WindowSize, Error> {
    let shape = "`window_size` is {\"width\": W, \"height\": H}, each from 200 to 10000.";
    let Some(object) = value.as_object() else {
        return Err(Error::invalid_argument(shape));
    };
    spec::only(object, &["width", "height"], "A window size")?;
    let side = |key: &str| -> Result<i64, Error> {
        match object.get(key).and_then(Value::as_i64) {
            Some(number) if (WindowSize::LEAST..=WindowSize::MOST).contains(&number) => Ok(number),
            _ => Err(Error::invalid_argument(shape)),
        }
    };
    Ok(WindowSize::new(side("width")?, side("height")?))
}

// ---- the personal dictionary -----------------------------------------

pub(super) fn dictionary_list(session: &Session, fields: Value) -> Result<Value, Error> {
    let _: spec::Nothing = spec::fields("dictionary.list", fields)?;
    let words: Vec<Value> = session
        .model()
        .personal_dictionary
        .iter()
        .map(|(key, word)| json!({"key": key, "word": word}))
        .collect();
    Ok(json!({"count": words.len(), "words": words}))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AddWord {
    word: String,
}

pub(super) fn dictionary_add(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: AddWord = spec::fields("dictionary.add", fields)?;
    let change =
        domain::add_dictionary_word(session.model(), &request.word).map_err(Error::rejected)?;
    session.commit_change(&change)?;
    entry(session, &domain::dictionary_key(&request.word))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EditWord {
    key: String,
    word: String,
}

/// The entry under a key, rewritten. The key is canonical, so the word
/// as it was typed anywhere finds it.
pub(super) fn dictionary_update(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: EditWord = spec::fields("dictionary.update", fields)?;
    held(session, &request.key)?;
    let change = domain::edit_dictionary_word(session.model(), &request.key, &request.word)
        .map_err(Error::rejected)?;
    session.commit_change(&change)?;
    entry(session, &domain::dictionary_key(&request.word))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoveWord {
    key: String,
}

pub(super) fn dictionary_delete(session: &mut Session, fields: Value) -> Result<Value, Error> {
    let request: RemoveWord = spec::fields("dictionary.delete", fields)?;
    let key = held(session, &request.key)?;
    let word = session
        .model()
        .personal_dictionary
        .get(&key)
        .cloned()
        .unwrap_or_default();

    let change =
        domain::remove_dictionary_word(session.model(), &request.key).map_err(Error::rejected)?;
    session.commit_change(&change)?;
    Ok(json!({"word": {"key": key, "word": word}, "undo": Value::Null}))
}

/// The canonical key of a word the dictionary actually holds.
fn held(session: &Session, key: &str) -> Result<String, Error> {
    let canonical = domain::dictionary_key(key);
    if !session.model().personal_dictionary.contains_key(&canonical) {
        return Err(Error::not_found(format!(
            "{key:?} is not in your dictionary."
        )));
    }
    Ok(canonical)
}

fn entry(session: &Session, key: &str) -> Result<Value, Error> {
    let word = session
        .model()
        .personal_dictionary
        .get(key)
        .cloned()
        .unwrap_or_default();
    Ok(json!({"word": {"key": key, "word": word}, "undo": Value::Null}))
}
