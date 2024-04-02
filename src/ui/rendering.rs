use std::io;

use crossterm::execute;

use crate::ui::layout::Rect;

#[derive(Debug, Clone, Copy)]
pub struct DrawTextConfig {
    pub wrap: WrapMode,
}

#[derive(Debug, Clone, Copy)]
pub enum WrapMode {
    /// truncate the line if it goes beyond the edge of the target rect.
    Truncate,
    /// wrap the line at word boundaries (currently only <space>) if it goes beyond the edge of the
    /// target rect.
    WordWrap,
    /// wrap the line when it goes beyond the edge of the rect, but do not respect word boundaries.
    CharacterWrap,
}

pub fn draw_text(writer: &mut impl io::Write, rect: &Rect, text: String, config: DrawTextConfig) {
    // execute!()
}
