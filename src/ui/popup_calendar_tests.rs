use super::*;

#[test]
fn calendar_renders_blank_cells_at_civil_limits() {
    for start in [WeekStart::Monday, WeekStart::Sunday] {
        for on in [Date::MIN, Date::MAX] {
            let area = ratatui::layout::Rect::new(0, 0, 28, 8);
            let mut buffer = ratatui::buffer::Buffer::empty(area);
            let weeks = weeks_of(on, start);
            calendar(
                &mut Canvas {
                    selection: None,
                    text_cells: Vec::new(),
                    buffer: &mut buffer,
                    area,
                },
                0,
                0,
                &weeks,
                &DateDraft {
                    kind: DateKind::Go,
                    on,
                    in_calendar: true,
                },
                on,
                start,
            );
            for (row, week) in weeks.iter().enumerate() {
                for (column, day) in week.iter().enumerate() {
                    let shown: String = (0..3)
                        .map(|cell| buffer[((column * 3 + cell) as u16, (row + 2) as u16)].symbol())
                        .collect();
                    let expected =
                        day.map_or_else(|| "   ".to_owned(), |date| format!(" {:>2}", date.day()));
                    assert_eq!(shown, expected);
                }
            }
        }
    }
}

fn check_month(on: Date, start: WeekStart) {
    let weeks = weeks_of(on, start);
    assert!((4..=6).contains(&weeks.len()));
    let first = on.first_of_month();
    let before = (first.weekday().to_monday_zero_offset()
        - first_column(start).to_monday_zero_offset())
    .rem_euclid(7) as usize;
    let mut dates = Vec::new();
    for (row, week) in weeks.iter().enumerate() {
        assert_eq!(week.len(), 7);
        for (column, cell) in week.iter().enumerate() {
            let offset = (row * 7 + column) as i64 - before as i64;
            if let Some(day) = cell {
                assert_eq!(
                    (day.weekday().to_monday_zero_offset()
                        - first_column(start).to_monday_zero_offset())
                    .rem_euclid(7) as usize,
                    column
                );
                if let Some(previous) = dates.last() {
                    assert!(previous < day, "dates must be unique and increasing");
                }
                dates.push(*day);
            } else {
                assert!(first.checked_add(Span::new().days(offset)).is_err());
            }
        }
    }
    let month: Vec<_> = dates
        .into_iter()
        .filter(|day| day.month() == on.month())
        .collect();
    assert_eq!(month.len(), on.last_of_month().day() as usize);
    assert_eq!(month.first(), Some(&first));
    assert_eq!(month.last(), Some(&on.last_of_month()));
}

#[test]
fn boundary_months_leave_unrepresentable_cells_empty() {
    for start in [WeekStart::Monday, WeekStart::Sunday] {
        for on in [Date::MIN, Date::MAX] {
            check_month(on, start);
            let column = (on.weekday().to_monday_zero_offset()
                - first_column(start).to_monday_zero_offset())
            .rem_euclid(7) as usize;
            let missing = if on == Date::MIN { column } else { 6 - column };
            assert_eq!(
                weeks_of(on, start)
                    .iter()
                    .flatten()
                    .filter(|cell| cell.is_none())
                    .count(),
                missing
            );
        }
    }
}

#[test]
fn four_five_and_six_week_calendars_preserve_weekday_columns() {
    for (year, month, start, rows) in [
        (2021, 2, WeekStart::Monday, 4),
        (2021, 2, WeekStart::Sunday, 5),
        (2015, 2, WeekStart::Sunday, 4),
        (2015, 2, WeekStart::Monday, 5),
        (2025, 3, WeekStart::Monday, 6),
        (2025, 3, WeekStart::Sunday, 6),
    ] {
        let on = Date::new(year, month, 1).unwrap();
        assert_eq!(weeks_of(on, start).len(), rows);
        check_month(on, start);
    }
    // A complete Gregorian cycle includes leap February and every
    // weekday arrangement, for both supported starts of the week.
    for year in 2000..2400 {
        for month in 1..=12 {
            for start in [WeekStart::Monday, WeekStart::Sunday] {
                check_month(Date::new(year, month, 1).unwrap(), start);
            }
        }
    }
}
