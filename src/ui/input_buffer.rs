// FIXME: none of this can handle UTF8 input properly!

use log::trace;

#[derive(Debug, Clone, Default)]
pub struct InputBuffer {
    /// the full text in the input buffer
    buffer: String,
    /// the current index of the cursor within the input buffer.
    /// INVARIANT: cursor_idx is at *most* equal to buffer.len() (representing selecting the 0 size
    /// slice at the end of the buffer)
    cursor_idx: usize,
}

impl InputBuffer {
    /// insert a character into the input buffer at the current selected position and advances the
    /// selected position by 1.
    pub fn insert(&mut self, char: char) {
        self.buffer.insert(self.cursor_idx, char);
        self.cursor_idx += 1;
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
            self.buffer.remove(self.cursor_idx - 1);
            self.cursor_idx -= 1;
        }
    }

    /// set the selection index to idx. clamps the value to within the valid range.
    pub fn select(&mut self, idx: usize) {
        self.cursor_idx = usize::min(idx, self.buffer.len());
    }

    /// move the selection index. clamps the value to within the valid range.
    pub fn offset(&mut self, offset: isize) {
        let ideal_idx = self.cursor_idx.saturating_add_signed(offset);
        self.cursor_idx = usize::min(ideal_idx, self.buffer.len());
    }

    pub fn buffer(&self) -> &str {
        self.buffer.as_str()
    }

    pub fn cursor_pos(&self) -> usize {
        self.cursor_idx
    }

    pub fn get_visible_area(&self, width: u16) -> (&str, usize) {
        // try to place the cursor as close to the middle of the returned string as possible.
        // if the cursor is at least width/2 characters from either end of the string, it will
        // be exactly in the middle.
        //
        // CASES:
        // centered cursor:
        // xxxxxxxxxxxxxxxxxxxxxxxxxxxxx
        //      |        ^        |
        // left cursor
        // xxxxxxxxxxxxxxxxxxxxxxxxxxxxx
        // |    ^            |
        // right cursor
        // xxxxxxxxxxxxxxxxxxxxxxxxxxxxx
        //           |             ^   |
        trace!("buffer {:?}", self.buffer);

        let width = usize::from(width);

        let half_width = width / 2;
        // the distance to the left side of the buffer (not window)
        let left_dist = self.cursor_idx;
        // the distance to the right side of the buffer (not window)
        let right_dist = self.buffer.len().saturating_sub(self.cursor_idx);

        trace!(
            "width {}(half {}) cursor {} left_dist {} right_dist {}",
            width,
            half_width,
            self.cursor_idx,
            left_dist,
            right_dist
        );

        let range = if left_dist >= half_width && right_dist <= half_width {
            // if the cursor is close to the end of the text, stop moving the text
            // leftwards, and move the cursor right instead
            let start = self.buffer.len().saturating_sub(width);
            start..self.buffer.len()
        } else {
            // the text extends half_width characters to the left, or to the start of the text,
            // whichever is less (as to not overflow in the negative direction)
            let start = self.cursor_idx - usize::min(left_dist, half_width);
            let end = usize::min(start + width, self.buffer.len());
            start..end
        };
        let cursor_pos = self.cursor_idx - range.start;

        trace!("range {:?}", range);

        (
            self.buffer
                .get(range.clone())
                .expect("start and end should be clamped"),
            cursor_pos,
        )
    }
}
