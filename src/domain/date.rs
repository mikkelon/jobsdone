//! Reading a date somebody typed, and writing one (DOMAIN.md section 2).
//!
//! The date card is the one place in the program where a date is written
//! rather than picked, and what the few shapes mean is a rule about
//! dates, not a detail of the card. Which way round a date is written is
//! a rule too, so every date on screen is formatted by one of the three
//! labels here and by nothing else.

use jiff::civil::Date;
use jiff::{Span, Zoned};

use super::rule::{Rule, Weekday, next_dates};
use super::settings::{DateOrder, WorkDays};

/// `Fri 5 Sep`, or `Fri Sep 5` where the month comes first.
pub fn day_label(date: Date, order: DateOrder) -> String {
    match order {
        DateOrder::DayFirst => date.strftime("%a %-d %b").to_string(),
        DateOrder::MonthFirst => date.strftime("%a %b %-d").to_string(),
    }
}

/// The same without the weekday: `5 Sep` or `Sep 5`.
pub fn short_label(date: Date, order: DateOrder) -> String {
    match order {
        DateOrder::DayFirst => date.strftime("%-d %b").to_string(),
        DateOrder::MonthFirst => date.strftime("%b %-d").to_string(),
    }
}

/// A day and the time of it: `Fri 5 Sep 08:12`.
pub fn stamp_label(instant: &Zoned, order: DateOrder) -> String {
    format!(
        "{} {}",
        day_label(instant.date(), order),
        instant.strftime("%H:%M")
    )
}

/// How far ahead a day of the month with no month named is looked for.
/// Twelve months is every month a "31" can miss.
const MONTHS: u8 = 12;

/// The date a typed line means, on the day it was typed.
///
/// A date with no year is the next one that has not passed, so "30 sep"
/// in December is next September. Nothing else is guessed: what cannot be
/// read is nothing, and the card says so rather than choosing a day.
pub fn parse_date(text: &str, today: Date) -> Option<Date> {
    let text = text.trim().to_lowercase();
    if text.is_empty() {
        return None;
    }
    // A count of days is signed, so that "-3" is three days ago rather
    // than the third of a month.
    if let Some(days) = text.strip_prefix(['+', '-']) {
        let days: i64 = days.trim().parse().ok()?;
        let days = if text.starts_with('-') { -days } else { days };
        return today.checked_add(Span::new().days(days)).ok();
    }
    match text.as_str() {
        "today" => return Some(today),
        "tomorrow" => return today.tomorrow().ok(),
        _ => {}
    }
    if let Some(weekday) = weekday(&text) {
        // A weekly shape names its own days, so which days are worked
        // does not come into it.
        return next_dates(
            &Rule::Weekly {
                weekdays: vec![weekday],
            },
            today,
            1,
            &WorkDays::DEFAULT,
        )
        .first()
        .copied();
    }
    // The written-out form first, so that its dashes are not read as the
    // separators of a day and a month.
    if let Ok(date) = text.parse::<Date>() {
        return Some(date);
    }

    let parts: Vec<&str> = text
        .split([' ', '/', '-', '.', ','])
        .filter(|part| !part.is_empty())
        .collect();
    match parts.as_slice() {
        [day] => day_of_a_month(number(day)?, today),
        [one, other] => {
            let (day, month) = day_and_month(one, other)?;
            in_a_year_not_yet_gone(day, month, today)
        }
        [one, other, year] => {
            let (day, month) = day_and_month(one, other)?;
            Date::new(year.parse().ok()?, month as i8, day as i8).ok()
        }
        _ => None,
    }
}

/// The day and the month of a pair, whichever way round it was written.
/// Two numbers are the day first, the way a date is said here.
fn day_and_month(one: &str, other: &str) -> Option<(u8, u8)> {
    if let Some(month) = month(one) {
        return Some((number(other)?, month));
    }
    if let Some(month) = month(other) {
        return Some((number(one)?, month));
    }
    Some((number(one)?, number(other)?))
}

/// The next day of the month with that number, this month or a later
/// one, skipping the months too short to have it.
fn day_of_a_month(day: u8, today: Date) -> Option<Date> {
    let mut first = today.first_of_month();
    for _ in 0..MONTHS {
        if let Ok(date) = Date::new(first.year(), first.month(), day as i8)
            && date >= today
        {
            return Some(date);
        }
        first = first.checked_add(Span::new().months(1)).ok()?;
    }
    None
}

/// The first such day and month that has not passed, which is this year
/// or the next.
fn in_a_year_not_yet_gone(day: u8, month: u8, today: Date) -> Option<Date> {
    let (day, month) = (day as i8, month as i8);
    match Date::new(today.year(), month, day) {
        Ok(date) if date >= today => Some(date),
        _ => Date::new(today.year() + 1, month, day).ok(),
    }
}

fn number(text: &str) -> Option<u8> {
    text.parse().ok()
}

/// A month by name or by the first three letters of one.
fn month(text: &str) -> Option<u8> {
    const MONTHS: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    named(text, &MONTHS).map(|at| at as u8 + 1)
}

/// A weekday by name or by the first three letters of one.
fn weekday(text: &str) -> Option<Weekday> {
    const DAYS: [&str; 7] = [
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
        "sunday",
    ];
    const WEEKDAYS: [Weekday; 7] = [
        Weekday::Mon,
        Weekday::Tue,
        Weekday::Wed,
        Weekday::Thu,
        Weekday::Fri,
        Weekday::Sat,
        Weekday::Sun,
    ];
    named(text, &DAYS).map(|at| WEEKDAYS[at])
}

/// Which name the text is, written out or cut to three letters. Three is
/// the shortest length no two of either set share.
fn named(text: &str, names: &[&str]) -> Option<usize> {
    if text.len() < 3 {
        return None;
    }
    names
        .iter()
        .position(|name| *name == text || name.starts_with(text) && text.len() == 3)
}
