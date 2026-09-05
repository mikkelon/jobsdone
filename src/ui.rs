//! Drawing: application state in, a ratatui frame out, plus the layout of
//! what was drawn.
//!
//! A view is a pure function of application state. This module orders,
//! filters and groups nothing; it formats and places what it is handed.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout as Rows, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, Layout};
use crate::input::bindings;

/// Secondary text and structure: metadata, hints, group labels, and the
/// box drawing.
fn dim() -> Style {
    Style::new().add_modifier(Modifier::DIM)
}

/// Where the keyboard is and the one thing to press. Blue stands in for
/// Omarchy's accent token, and it never marks task state.
fn accent() -> Style {
    Style::new().fg(Color::Blue).add_modifier(Modifier::BOLD)
}

/// The one-cell left and right margin, so text never touches the window
/// border. The rules run the full width.
fn inset(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1),
        width: area.width.saturating_sub(2),
        ..area
    }
}

pub fn draw(app: &App, frame: &mut Frame) -> Layout {
    let [_top, status, upper_rule, _panes, lower_rule, hints, _bottom] = Rows::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    frame.render_widget(Paragraph::new(status_line(app)), inset(status));
    frame.render_widget(rule(upper_rule.width), upper_rule);
    frame.render_widget(rule(lower_rule.width), lower_rule);
    frame.render_widget(Paragraph::new(hint_bar(app)), inset(hints));

    Layout
}

/// `Today · Fri 5 Sep`. Phase 7 adds the day navigation and phase 9 the
/// review count.
fn status_line(app: &App) -> Line<'static> {
    let day = app.today().strftime("%a %-d %b").to_string();
    Line::from(vec![
        Span::raw("Today "),
        Span::styled(format!("· {day}"), dim()),
    ])
}

fn rule(width: u16) -> Paragraph<'static> {
    Paragraph::new(Span::styled("─".repeat(width as usize), dim()))
}

/// The keys of the focused context, drawn from the key table and from
/// nothing else, so the hint bar cannot disagree with the dispatcher.
fn hint_bar(app: &App) -> Line<'static> {
    let mut spans = Vec::new();
    for binding in bindings(app.key_context()) {
        if !spans.is_empty() {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(binding.key, accent()));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(binding.label, dim()));
    }
    Line::from(spans)
}
