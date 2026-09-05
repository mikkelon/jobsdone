//! Repeat rules and the dates they fall on (DOMAIN.md section 10).
//!
//! Exactly five shapes. The JSON here is the JSON stored in
//! `schedules.rule`, so the serde attributes are part of the format.

use jiff::Span;
use jiff::civil::{Date, Weekday as Civil};
use serde::de::{Error as _, Unexpected};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// How many days `next_dates` walks per date it is asked for before it
/// gives up. The widest gap the walked shapes can leave is a month, so a
/// year is slack, and the bound is what keeps a rule that never falls due
/// from looping. "Every N weeks" does not walk at all.
const HORIZON: usize = 366;

/// A day of the week, spelled the way the stored JSON spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Weekday {
    /// The weekday a date falls on, which the repeat card needs to open
    /// on the weekday of the day it is about.
    pub fn of(date: Date) -> Weekday {
        match date.weekday() {
            Civil::Monday => Weekday::Mon,
            Civil::Tuesday => Weekday::Tue,
            Civil::Wednesday => Weekday::Wed,
            Civil::Thursday => Weekday::Thu,
            Civil::Friday => Weekday::Fri,
            Civil::Saturday => Weekday::Sat,
            Civil::Sunday => Weekday::Sun,
        }
    }

    /// Monday to Friday, which is what "every work day" and the move
    /// card's "next work day" both mean. No holidays, ever.
    fn is_work_day(self) -> bool {
        !matches!(self, Weekday::Sat | Weekday::Sun)
    }
}

/// Which day of the month a monthly rule falls on: a number, clamped to
/// the month's last day, or the last day itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MonthDay {
    Day(u8),
    Last,
}

impl Serialize for MonthDay {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            MonthDay::Day(day) => serializer.serialize_u8(*day),
            MonthDay::Last => serializer.serialize_str("last"),
        }
    }
}

impl<'de> Deserialize<'de> for MonthDay {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<MonthDay, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Stored {
            Day(u8),
            Word(String),
        }

        match Stored::deserialize(deserializer)? {
            Stored::Day(day) => Ok(MonthDay::Day(day)),
            Stored::Word(word) if word == "last" => Ok(MonthDay::Last),
            Stored::Word(word) => Err(D::Error::invalid_value(
                Unexpected::Str(&word),
                &"a day of the month or \"last\"",
            )),
        }
    }
}

/// A repeat schedule's rule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Rule {
    Workdays,
    Daily,
    Weekly { weekdays: Vec<Weekday> },
    Monthly { day: MonthDay },
    EveryNWeeks { n: u32, from: Date },
}

impl Rule {
    /// Whether the rule can ever fall due. A rule that cannot is refused
    /// rather than stored, because generation would then never end a
    /// schedule and the repeat card would preview nothing.
    pub fn is_usable(&self) -> bool {
        match self {
            Rule::Workdays | Rule::Daily => true,
            Rule::Weekly { weekdays } => !weekdays.is_empty(),
            Rule::Monthly { day } => match day {
                MonthDay::Day(day) => (1..=31).contains(day),
                MonthDay::Last => true,
            },
            Rule::EveryNWeeks { n, .. } => *n >= 1,
        }
    }

    /// Whether `date` is one of the rule's dates.
    pub fn falls_on(&self, date: Date) -> bool {
        match self {
            Rule::Workdays => Weekday::of(date).is_work_day(),
            Rule::Daily => true,
            Rule::Weekly { weekdays } => weekdays.contains(&Weekday::of(date)),
            Rule::Monthly { day } => {
                let last = date.days_in_month();
                let wanted = match day {
                    MonthDay::Day(day) => i8::try_from(*day).unwrap_or(last).min(last),
                    MonthDay::Last => last,
                };
                date.day() == wanted
            }
            Rule::EveryNWeeks { n, from } => {
                let step = i64::from(*n) * 7;
                if step <= 0 || date < *from {
                    return false;
                }
                from.until(date)
                    .is_ok_and(|span| i64::from(span.get_days()) % step == 0)
            }
        }
    }
}

/// The next `count` dates a rule falls on strictly after `after`. The
/// repeat card's preview and copy generation are the same function.
pub fn next_dates(rule: &Rule, after: Date, count: usize) -> Vec<Date> {
    if count == 0 || !rule.is_usable() {
        return Vec::new();
    }
    // Every N weeks is arithmetic; the other four are rare enough in any
    // week or month to be found by walking.
    if let Rule::EveryNWeeks { n, from } = rule {
        return every_n_weeks(*n, *from, after, count);
    }

    let mut dates = Vec::new();
    let mut date = after;
    for _ in 0..HORIZON.saturating_mul(count) {
        let Ok(next) = date.tomorrow() else { break };
        date = next;
        if rule.falls_on(date) {
            dates.push(date);
            if dates.len() == count {
                break;
            }
        }
    }
    dates
}

fn every_n_weeks(n: u32, from: Date, after: Date, count: usize) -> Vec<Date> {
    let step = i64::from(n) * 7;
    let mut date = from;
    if after >= from {
        let gone = from
            .until(after)
            .map_or(0, |span| i64::from(span.get_days()));
        let Ok(next) = from.checked_add(Span::new().days((gone / step + 1) * step)) else {
            return Vec::new();
        };
        date = next;
    }

    let mut dates = Vec::with_capacity(count);
    while dates.len() < count {
        dates.push(date);
        let Ok(next) = date.checked_add(Span::new().days(step)) else {
            break;
        };
        date = next;
    }
    dates
}
