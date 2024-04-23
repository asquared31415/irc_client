use log::*;
use unicode_segmentation::UnicodeSegmentation as _;

#[derive(Debug, Clone, Default)]
pub struct InputBuffer {
    /// the full text in the input buffer
    buffer: String,
    /// the current **byte** index of the cursor within the input buffer.
    /// INVARIANT: cursor_idx is at *most* equal to buffer.len() (representing selecting the 0 size
    /// slice at the end of the buffer)
    cursor_idx: usize,
}

impl InputBuffer {
    /// insert a character into the input buffer at the current selected position and advances the
    /// selected position by 1 character (maybe more than one byte).
    pub fn insert(&mut self, char: char) {
        self.buffer.insert(self.cursor_idx, char);
        self.cursor_idx += char.len_utf8();
    }

    /// removes the character at the cursor, if it exists. does not move the cursor.
    pub fn delete(&mut self) {
        if self.cursor_idx < self.buffer.len() {
            self.buffer.remove(self.cursor_idx);
        }
    }

    /// finish editing the current contents of the input buffer, returning them, and resetting the
    /// buffer.
    pub fn finish(&mut self) -> String {
        self.cursor_idx = 0;
        // note: this resets the buffer to String::default()
        core::mem::take(&mut self.buffer)
    }

    /// removes the character before the cursor, if it exists, and then moves the selected position
    /// backwards by 1.
    pub fn backspace(&mut self) {
        if 1 <= self.cursor_idx && self.cursor_idx <= self.buffer.len() {
            let prev_idx = self.buffer.floor_char_boundary(self.cursor_idx - 1);
            self.buffer.remove(prev_idx);
            self.cursor_idx = prev_idx;
        }
    }

    /// set the selection index to the character with index idx
    pub fn select(&mut self, idx: usize) {
        let byte_idx = match self.buffer.char_indices().skip(idx).next() {
            Some((idx, _)) => idx,
            // skipped past the end, clamp to end
            None => self.buffer.len(),
        };
        self.cursor_idx = byte_idx;
    }

    /// offset the selection index by `offset` *characters*
    pub fn offset(&mut self, offset: isize) {
        let mut offset = offset;

        // advance forwards
        while offset > 0 {
            if let Some(c) = self.buffer.get(self.cursor_idx..self.cursor_idx + 1) {
                self.cursor_idx += c.len();
                offset -= 1;
            } else {
                // at end, cannot go farther
                return;
            }
        }

        // go backwards
        while offset < 0 {
            if self.cursor_idx > 0 {
                self.cursor_idx = self.buffer.floor_char_boundary(self.cursor_idx - 1);
                offset += 1;
            } else {
                // at start
                return;
            }
        }
    }

    pub fn char_len(&self) -> usize {
        self.buffer.chars().count()
    }

    // TODO: this is likely slightly incorrect in the face of different widths for graphemes
    pub fn get_visible_area(&self, width: u16) -> (&str, usize) {
        // we cannot meaningfully lay out any real text in 0 width, and really the cursor doesn't
        // fit at position 0 either
        if width == 0 {
            return ("", 0);
        }

        // try to place the cursor as close to the middle of the returned string as possible.
        // if the cursor is at least width/2 characters from either end of the string, it will
        // be exactly in the middle.

        trace!("buffer {:?}", self.buffer);
        let graphemes = self.buffer.grapheme_indices(true);
        let (before, after) = graphemes.partition::<Vec<_>, _>(|(idx, _)| *idx < self.cursor_idx);
        trace!("before: {:#?}, after {:#?}", before, after);

        let width = usize::from(width);
        let half_width = width / 2;

        let (range, cursor_pos) = match (before.len() >= half_width, after.len() >= half_width) {
            // there are not enough graphemes to the left of the cursor to center it, the text
            // should be pinned to the left, and the cursor based on that
            (false, _) => {
                let after_count = usize::min(width - before.len(), after.len());
                let end = after
                    .iter()
                    .nth(after_count)
                    .map_or(self.buffer.len(), |(idx, _)| *idx);
                (0..end, before.len())
            }
            // there are enough graphemes to center the cursor exactly
            (true, true) => {
                let start = before
                    .iter()
                    .rev()
                    .nth(half_width.saturating_sub(1))
                    .map(|(idx, _)| *idx)
                    .unwrap();
                let end = after
                    .iter()
                    .nth(half_width)
                    .map_or(self.buffer.len(), |(idx, _)| *idx);
                (start..end, half_width)
            }
            // there are not enough graphemes to the right, may or may not need to pin depending on
            // whether there's enough total graphemes to fill the width
            (true, false) => {
                //
                if before.len() + after.len() >= width {
                    // the entire width can be filled, pin the text to the right
                    let before_count = usize::min(width - after.len(), before.len());
                    let start = before
                        .iter()
                        .rev()
                        .nth(before_count.saturating_sub(1))
                        .map(|(idx, _)| *idx)
                        .unwrap();
                    (start..self.buffer.len(), width - after.len())
                } else {
                    // cannot fill the buffer width, pin left
                    let after_count = usize::min(width - before.len(), after.len());
                    let end = after
                        .iter()
                        .nth(after_count)
                        .map_or(self.buffer.len(), |(idx, _)| *idx);
                    (0..end, before.len())
                }
            }
        };

        trace!(
            "buf.len {} range {:?} cursor_pos {}",
            self.buffer.len(),
            range,
            cursor_pos
        );

        (
            self.buffer
                .get(range.clone())
                .expect("start and end should be clamped"),
            cursor_pos,
        )
    }
}
