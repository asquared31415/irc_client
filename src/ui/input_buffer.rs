// FIXME: none of this can handle UTF8 input properly!

use log::{debug, trace};

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
        // if the cursor is at least ceil(width/2) characters from either end of the string, it will
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

        let half_width = usize::from(width.div_ceil(2));
        let left_dist = usize::min(self.cursor_idx, half_width);
        trace!(
            "width {}(half {}) cursor {} left_dist {}",
            width,
            half_width,
            self.cursor_idx,
            left_dist
        );

        // can never overflow, `min` above ensures that lift_dist is never greater than the idx
        let start = self.cursor_idx - left_dist;
        let end = usize::min(start + usize::from(width), self.buffer.len());
        trace!("start {} end {}", start, end);

        (
            self.buffer
                .get(start..end)
                .expect("start and end should be clamped"),
            left_dist,
        )
    }
}
