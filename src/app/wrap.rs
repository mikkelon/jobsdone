//! One wrapping of a note body: the rows it is drawn on, where a caret
//! falls among them, and which character a cell of one of them is.
//!
//! Drawing, the caret's vertical steps and the mouse all read this, so
//! that what is on screen, what `↑` lands on and what a click points at
//! can never be three different pictures of the same body. Rows are
//! terminal cells, because that is what a body is drawn in; carets are
//! grapheme clusters, because that is what a body is written in
//! (ARCHITECTURE.md rule 6's unit for text).

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[cfg(test)]
mod tests;

/// Which row a caret sitting exactly on a soft wrap belongs to.
///
/// A wrap the pane made rather than the writer has one place in the body
/// and two on screen: the cell after the last character of the row that
/// was broken, and the first cell of the row that carries the rest. They
/// are the same character, so only the way the caret arrived can tell
/// them apart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Affinity {
    /// The row that carries the rest. Where typing, `←` and `→` leave a
    /// caret: a body walked through a character at a time has no reason
    /// to stop twice at a break the writer never put there.
    #[default]
    AfterTheBreak,
    /// The row that was broken. Where `End`, a click past the last
    /// character of a row, and a vertical step onto a row too short to
    /// reach the column leave a caret.
    BeforeTheBreak,
}

/// One row of a wrapped body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The characters drawn on it, without the newline that ended it.
    pub text: String,
    /// The cluster of the body its first character is, which is what the
    /// caret and the spelling marks are both counted from.
    pub start: usize,
    /// Whether the row ran out of cells and the row after it carries on
    /// the same line. A row that ends at a newline, and the last row of
    /// the body, did not.
    pub continues: bool,
}

impl Row {
    /// How many clusters are on the row.
    fn glyphs(&self) -> usize {
        self.text.graphemes(true).count()
    }

    /// The cluster after the last one on the row, which for a row that
    /// continues is the first cluster of the next.
    fn end(&self) -> usize {
        self.start + self.glyphs()
    }
}

/// A note body broken into the rows it is drawn on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wrapping {
    rows: Vec<Row>,
}

impl Wrapping {
    /// Every line of the body broken at `width` cells, on a space where
    /// there is one. A body has at least one row, and a trailing newline
    /// opens another.
    pub fn of(body: &str, width: u16) -> Self {
        let room = usize::from(width.max(1));
        let mut rows = Vec::new();
        let mut at = 0;
        for line in body.split('\n') {
            let glyphs: Vec<&str> = line.graphemes(true).collect();
            let mut from = 0;
            loop {
                // How many clusters of the rest the row has room for. A
                // cluster wider than the whole pane still takes a row of
                // its own rather than none.
                let mut fits = 0;
                let mut taken = 0;
                while from + fits < glyphs.len() {
                    let cells = usize::from(cells(glyphs[from + fits]));
                    if taken + cells > room {
                        break;
                    }
                    taken += cells;
                    fits += 1;
                }
                let fits = fits.max(1);
                if from + fits >= glyphs.len() {
                    rows.push(Row {
                        text: glyphs[from..].concat(),
                        start: at + from,
                        // The line ends here; only another line after it
                        // is a break, and that one is the writer's.
                        continues: false,
                    });
                    break;
                }
                // After the last space that fits, or through a word
                // longer than the pane.
                let take = glyphs[from..from + fits]
                    .iter()
                    .rposition(|glyph| *glyph == " ")
                    .map_or(fits, |space| space + 1);
                rows.push(Row {
                    text: glyphs[from..from + take].concat(),
                    start: at + from,
                    continues: true,
                });
                from += take;
            }
            // The newline the split took off, which the next line starts
            // one cluster after.
            at += glyphs.len() + 1;
        }
        Self { rows }
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The character after the last one in the body, which is as far as
    /// a caret goes.
    pub fn end(&self) -> usize {
        self.rows.last().map_or(0, Row::end)
    }

    /// Which drawn row the caret is on.
    pub fn row_of(&self, caret: usize, affinity: Affinity) -> usize {
        let row = self
            .rows
            .partition_point(|row| row.start <= caret)
            .saturating_sub(1);
        // On a wrap the pane made, the row before it is the other answer
        // and the caret says which one it means.
        if affinity == Affinity::BeforeTheBreak
            && row > 0
            && self.rows[row].start == caret
            && self.rows[row - 1].continues
        {
            return row - 1;
        }
        row
    }

    /// The cell of that row the caret is drawn in, counted from the first
    /// cell of the row.
    pub fn column_of(&self, caret: usize, affinity: Affinity) -> u16 {
        let row = self.row_of(caret, affinity);
        let Some(drawn) = self.rows.get(row) else {
            return 0;
        };
        let cells: usize = drawn
            .text
            .graphemes(true)
            .take(caret.saturating_sub(drawn.start))
            .map(|glyph| usize::from(cells(glyph)))
            .sum();
        u16::try_from(cells).unwrap_or(u16::MAX)
    }

    /// Where the caret goes when a row is stepped onto at a column: in
    /// front of the character that column is drawn in, and at the end of
    /// the row where the row does not reach that far.
    pub fn caret_at(&self, row: usize, column: u16) -> (usize, Affinity) {
        let Some(drawn) = self.rows.get(row) else {
            return (0, Affinity::AfterTheBreak);
        };
        let column = usize::from(column);
        let mut taken = 0;
        for (at, glyph) in drawn.text.graphemes(true).enumerate() {
            // A wide character is one character wherever in it the
            // column falls.
            if taken + usize::from(cells(glyph)) > column {
                return (drawn.start + at, Affinity::AfterTheBreak);
            }
            taken += usize::from(cells(glyph));
        }
        let affinity = if drawn.continues {
            Affinity::BeforeTheBreak
        } else {
            Affinity::AfterTheBreak
        };
        (drawn.end(), affinity)
    }
}

/// The first row on screen: the one the note was left showing, moved as
/// little as it takes for the caret's row to be among the rows on screen
/// and for the last row not to be scrolled past.
///
/// The note keeps its place rather than working one out from the caret
/// every frame, which is what makes `↑` from the bottom of a long note
/// walk up the rows already on screen before any of them move.
pub fn viewport(first: usize, caret: usize, rows: usize, height: usize) -> usize {
    let height = height.max(1);
    first
        .min(rows.saturating_sub(height))
        .min(caret)
        .max((caret + 1).saturating_sub(height))
}

/// How many cells one grapheme cluster takes. A cluster whose only
/// characters are combining marks has no cell of its own; every other one
/// has one or two.
fn cells(glyph: &str) -> u16 {
    UnicodeWidthStr::width(glyph) as u16
}
