//! Reading a request: the fields of an operation, and the four values
//! that are written as more than a number or a string.
//!
//! Nothing here is lenient. A field the operation does not have, a date
//! that is not one of the shapes, a rule with a key that is not part of
//! it, a number outside its range: each is refused with what was wrong
//! with it, because a noninteractive caller has no screen to notice a
//! value having been quietly changed on.

use std::collections::BTreeSet;

use jiff::civil::Date;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

use super::Error;
use crate::domain::{Id, MonthDay, Place, Rule, Weekday, WorkDays, next_dates};

/// The fields of one operation, read into its structure.
pub(super) fn fields<T: DeserializeOwned>(op: &str, value: Value) -> Result<T, Error> {
    serde_json::from_value(value).map_err(|error| Error::invalid_request(format!("{op}: {error}.")))
}

/// An operation that takes no fields at all.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Nothing {}

/// A field that tells being absent from being null: absent leaves a
/// setting alone, null clears it, a value sets it.
pub(super) fn clearable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

// ---- dates -----------------------------------------------------------

/// A date as a request writes it, before it is resolved.
#[derive(Clone, Debug, Deserialize)]
#[serde(transparent)]
pub(super) struct DateText(String);

impl DateText {
    pub(super) fn date(&self, today: Date, work_days: WorkDays) -> Result<Date, Error> {
        date_of(&self.0, today, work_days)
    }
}

/// The date a request means, resolved against the day the request is
/// being answered on.
///
/// The four words are the ones the move card offers, and "next work day"
/// is the work-days rule's own answer, so a command line and a key press
/// never disagree about which day comes next.
pub(super) fn date_of(text: &str, today: Date, work_days: WorkDays) -> Result<Date, Error> {
    let text = text.trim();
    let unreachable = || Error::invalid_argument(format!("There is no day {text:?} reaches."));

    match text.to_ascii_lowercase().as_str() {
        "today" => return Ok(today),
        "yesterday" => return today.yesterday().map_err(|_| unreachable()),
        "tomorrow" => return today.tomorrow().map_err(|_| unreachable()),
        "next-work-day" | "next_work_day" => {
            return next_dates(&Rule::Workdays, today, 1, &work_days)
                .first()
                .copied()
                .ok_or_else(unreachable);
        }
        _ => {}
    }
    text.parse::<Date>().map_err(|_| {
        Error::invalid_argument(format!(
            "{text:?} is not a date. Write YYYY-MM-DD, or today, yesterday, tomorrow or \
             next-work-day."
        ))
    })
}

// ---- places ----------------------------------------------------------

/// A place as a request writes it, before its day is resolved.
#[derive(Clone, Debug, Deserialize)]
#[serde(transparent)]
pub(super) struct PlaceValue(Value);

impl PlaceValue {
    pub(super) fn place(&self, today: Date, work_days: WorkDays) -> Result<Place, Error> {
        place_of(&self.0, today, work_days)
    }
}

const PLACE_SHAPE: &str =
    "A place is {\"kind\":\"backlog\"} or {\"kind\":\"day\",\"day\":\"YYYY-MM-DD\"}.";

pub(super) fn place_of(value: &Value, today: Date, work_days: WorkDays) -> Result<Place, Error> {
    let object = object_of(value, PLACE_SHAPE)?;
    match kind_of(object, PLACE_SHAPE)? {
        "backlog" => {
            only(object, &["kind"], "A backlog place")?;
            Ok(Place::Backlog)
        }
        "day" => {
            only(object, &["kind", "day"], "A day place")?;
            let day = text(object, "day", "A day place needs a `day`.")?;
            Ok(Place::Day(date_of(day, today, work_days)?))
        }
        other => Err(Error::invalid_argument(format!(
            "There is no place of kind {other:?}. {PLACE_SHAPE}"
        ))),
    }
}

// ---- rules -----------------------------------------------------------

/// A repeat rule as a request writes it.
#[derive(Clone, Debug, Deserialize)]
#[serde(transparent)]
pub(super) struct RuleValue(Value);

impl RuleValue {
    pub(super) fn rule(&self, today: Date, work_days: WorkDays) -> Result<Rule, Error> {
        rule_of(&self.0, today, work_days)
    }
}

const RULE_SHAPE: &str = "A rule is one of {\"kind\":\"workdays\"}, {\"kind\":\"daily\"}, \
                          {\"kind\":\"weekly\",\"weekdays\":[…]}, \
                          {\"kind\":\"monthly\",\"day\":15|\"last\"}, \
                          {\"kind\":\"every_n_weeks\",\"n\":2,\"from\":\"YYYY-MM-DD\"}.";

/// The five shapes of DOMAIN.md section 10, checked here rather than by
/// the codec that reads them back out of a schedule row: the stored JSON
/// is the program's own writing and this is somebody else's.
pub(super) fn rule_of(value: &Value, today: Date, work_days: WorkDays) -> Result<Rule, Error> {
    let object = object_of(value, RULE_SHAPE)?;
    let rule = match kind_of(object, RULE_SHAPE)? {
        "workdays" => {
            only(object, &["kind"], "The work-days rule")?;
            Rule::Workdays
        }
        "daily" => {
            only(object, &["kind"], "The daily rule")?;
            Rule::Daily
        }
        "weekly" => {
            only(object, &["kind", "weekdays"], "A weekly rule")?;
            let Some(Value::Array(names)) = object.get("weekdays") else {
                return Err(Error::invalid_argument(
                    "A weekly rule needs `weekdays`, a list of mon tue wed thu fri sat sun.",
                ));
            };
            let mut seen = BTreeSet::new();
            let mut weekdays = Vec::new();
            for name in names {
                let day = weekday_of(name)?;
                if !seen.insert(day) {
                    return Err(Error::invalid_argument(format!(
                        "A weekly rule names {} once.",
                        weekday_name(day)
                    )));
                }
                weekdays.push(day);
            }
            if weekdays.is_empty() {
                return Err(Error::invalid_argument(
                    "A weekly rule needs at least one weekday.",
                ));
            }
            Rule::Weekly { weekdays }
        }
        "monthly" => {
            only(object, &["kind", "day"], "A monthly rule")?;
            let day = match object.get("day") {
                Some(Value::String(word)) if word == "last" => MonthDay::Last,
                Some(Value::Number(number)) => match number.as_u64() {
                    Some(day @ 1..=31) => MonthDay::Day(day as u8),
                    _ => {
                        return Err(Error::invalid_argument(
                            "A monthly rule falls on a day from 1 to 31, or on \"last\".",
                        ));
                    }
                },
                _ => {
                    return Err(Error::invalid_argument(
                        "A monthly rule needs a `day`: a number from 1 to 31, or \"last\".",
                    ));
                }
            };
            Rule::Monthly { day }
        }
        "every_n_weeks" => {
            only(object, &["kind", "n", "from"], "An every-N-weeks rule")?;
            let n = match object.get("n").and_then(Value::as_u64) {
                Some(n @ 1..=520) => n as u32,
                _ => {
                    return Err(Error::invalid_argument(
                        "An every-N-weeks rule repeats every 1 to 520 weeks.",
                    ));
                }
            };
            let from = text(object, "from", "An every-N-weeks rule needs a `from` date.")?;
            Rule::EveryNWeeks {
                n,
                from: date_of(from, today, work_days)?,
            }
        }
        other => {
            return Err(Error::invalid_argument(format!(
                "There is no rule of kind {other:?}. {RULE_SHAPE}"
            )));
        }
    };

    if !rule.is_usable() {
        return Err(Error::invalid_argument("That repeat never comes round."));
    }
    Ok(rule)
}

fn weekday_of(value: &Value) -> Result<Weekday, Error> {
    let name = value.as_str().unwrap_or_default().to_ascii_lowercase();
    Weekday::ALL
        .into_iter()
        .find(|day| weekday_name(*day) == name)
        .ok_or_else(|| {
            Error::invalid_argument(format!(
                "{value} is not a weekday. Write mon tue wed thu fri sat sun."
            ))
        })
}

pub(super) fn weekday_name(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "mon",
        Weekday::Tue => "tue",
        Weekday::Wed => "wed",
        Weekday::Thu => "thu",
        Weekday::Fri => "fri",
        Weekday::Sat => "sat",
        Weekday::Sun => "sun",
    }
}

// ---- ids -------------------------------------------------------------

/// A non-empty list of distinct ids, in the order the request gave them.
pub(super) fn distinct(ids: &[Id], what: &str) -> Result<Vec<Id>, Error> {
    if ids.is_empty() {
        return Err(Error::invalid_argument(format!(
            "Name at least one {what}."
        )));
    }
    let mut seen = BTreeSet::new();
    for id in ids {
        if *id < 1 {
            return Err(Error::invalid_argument(format!(
                "{id} is not a {what} id; ids count from 1."
            )));
        }
        if !seen.insert(*id) {
            return Err(Error::invalid_argument(format!(
                "{what} {id} is named twice."
            )));
        }
    }
    Ok(ids.to_vec())
}

// ---- reading an object -----------------------------------------------

fn object_of<'a>(value: &'a Value, shape: &str) -> Result<&'a Map<String, Value>, Error> {
    value
        .as_object()
        .ok_or_else(|| Error::invalid_argument(shape.to_owned()))
}

fn kind_of<'a>(object: &'a Map<String, Value>, shape: &str) -> Result<&'a str, Error> {
    object
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::invalid_argument(shape.to_owned()))
}

fn text<'a>(object: &'a Map<String, Value>, key: &str, missing: &str) -> Result<&'a str, Error> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::invalid_argument(missing.to_owned()))
}

/// The keys an object may have, and no others.
pub(super) fn only(object: &Map<String, Value>, allowed: &[&str], what: &str) -> Result<(), Error> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(Error::invalid_argument(format!(
                "{what} has no {key:?} in it."
            )));
        }
    }
    Ok(())
}
