//! Ratatui 0.30 carries underline colour but not underline shape. Keep
//! its normal diff renderer, then redraw only changed spelling cells with
//! Crossterm's curly underline. SetStyle requests the ordinary underline
//! before the extension, so terminals ignoring the extension retain it.
//! Do not gate on TERM: foot is commonly configured as xterm-256color.
//! No text or escape sequences enter the model.

use std::io::{self, Write};

use crossterm::cursor::MoveTo;
use crossterm::queue;
use crossterm::style::{Attribute, Print, SetAttribute, SetStyle};
use ratatui::backend::{Backend, ClearType, CrosstermBackend, IntoCrossterm, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Size};
use ratatui::style::{Color, Modifier};

pub(super) struct SpellingBackend<W: Write> {
    inner: CrosstermBackend<W>,
}

impl<W: Write> SpellingBackend<W> {
    pub(super) fn new(writer: W) -> Self {
        Self {
            inner: CrosstermBackend::new(writer),
        }
    }
}

impl<W: Write> Backend for SpellingBackend<W> {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let mut spelling = Vec::new();
        self.inner.draw(content.inspect(|&(x, y, cell)| {
            if cell.modifier.contains(Modifier::UNDERLINED) && cell.underline_color == Color::Red {
                spelling.push((x, y, cell));
            }
        }))?;
        for (x, y, cell) in spelling {
            queue!(
                self.inner,
                MoveTo(x, y),
                SetStyle(cell.style().into_crossterm()),
                SetAttribute(Attribute::Undercurled),
                Print(cell.symbol()),
                SetAttribute(Attribute::Reset)
            )?;
        }
        Ok(())
    }

    fn append_lines(&mut self, n: u16) -> io::Result<()> {
        self.inner.append_lines(n)
    }
    fn hide_cursor(&mut self) -> io::Result<()> {
        self.inner.hide_cursor()
    }
    fn show_cursor(&mut self) -> io::Result<()> {
        self.inner.show_cursor()
    }
    fn get_cursor_position(&mut self) -> io::Result<Position> {
        self.inner.get_cursor_position()
    }
    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        self.inner.set_cursor_position(position)
    }
    fn clear(&mut self) -> io::Result<()> {
        self.inner.clear()
    }
    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        self.inner.clear_region(clear_type)
    }
    fn size(&self) -> io::Result<Size> {
        self.inner.size()
    }
    fn window_size(&mut self) -> io::Result<WindowSize> {
        self.inner.window_size()
    }
    fn flush(&mut self) -> io::Result<()> {
        Backend::flush(&mut self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;
    use ratatui::style::Style;
    use ratatui::{Terminal, TerminalOptions, Viewport};

    fn spelling() -> Style {
        Style::new()
            .underline_color(Color::Red)
            .add_modifier(Modifier::UNDERLINED)
    }

    #[test]
    fn only_red_spelling_underlines_are_curled_and_attributes_are_reset() {
        let mut bytes = Vec::new();
        let mut backend = SpellingBackend {
            inner: CrosstermBackend::new(&mut bytes),
        };
        let mut marked = Cell::new("e\u{301}");
        marked.set_style(spelling().bg(Color::Blue).add_modifier(Modifier::BOLD));
        let mut ordinary = Cell::new("x");
        ordinary.set_style(Style::new().add_modifier(Modifier::UNDERLINED));
        backend
            .draw([(2, 3, &marked), (3, 3, &ordinary)].into_iter())
            .unwrap();
        let output = String::from_utf8(bytes).unwrap();
        assert_eq!(output.matches("\x1b[4:3m").count(), 1);
        assert!(output.contains("\x1b[4:3me\u{301}\x1b[0m"));
        assert!(output.contains("\x1b[4;3H"), "redraw uses cell coordinates");
    }

    #[test]
    fn normal_underline_precedes_the_optional_shape_extension() {
        let mut bytes = Vec::new();
        let mut backend = SpellingBackend {
            inner: CrosstermBackend::new(&mut bytes),
        };
        let mut marked = Cell::new("x");
        marked.set_style(spelling());
        backend.draw([(0, 0, &marked)].into_iter()).unwrap();
        let output = String::from_utf8(bytes).unwrap();
        assert!(output.contains("\x1b[4m"));
        assert!(output.find("\x1b[4m").unwrap() < output.find("\x1b[4:3m").unwrap());
        assert!(output.contains("\x1b[4:3mx\x1b[0m"));
    }

    #[test]
    fn unchanged_frames_do_not_repaint_marks_and_corrections_remove_them() {
        let mut bytes = Vec::new();
        {
            let backend = SpellingBackend {
                inner: CrosstermBackend::new(&mut bytes),
            };
            let mut terminal = Terminal::with_options(
                backend,
                TerminalOptions {
                    viewport: Viewport::Fixed(Rect::new(0, 0, 12, 2)),
                },
            )
            .unwrap();
            for _ in 0..2 {
                terminal
                    .draw(|frame| {
                        frame.buffer_mut().set_string(0, 0, "teh", spelling());
                    })
                    .unwrap();
            }
            terminal
                .draw(|frame| {
                    frame.buffer_mut().set_string(0, 0, "the", Style::new());
                })
                .unwrap();
        }
        let output = String::from_utf8(bytes).unwrap();
        assert_eq!(
            output.matches("\x1b[4:3m").count(),
            3,
            "only the first frame curls each cell"
        );
        let corrected = output.rsplit_once("\x1b[4:3m").unwrap().1;
        assert!(corrected.contains("\x1b[0m"));
        assert!(
            corrected.contains("the"),
            "the corrected word is redrawn normally"
        );
    }
}
