use std::{fs::File, io, io::prelude::Write as _};

use crossterm::{cursor, execute};
use unicode_segmentation::UnicodeSegmentation;

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

/// returns the number of lines drawn
pub fn draw_text(
    writer: &mut impl io::Write,
    rect: Rect,
    text: String,
    config: DrawTextConfig,
) -> eyre::Result<u16> {
    let Rect {
        x,
        y,
        width,
        height: _,
    } = rect;
    execute!(writer, cursor::MoveTo(x, y))?;

    let mut log_file = File::options()
        .create(true)
        .append(true)
        .open("log_render.txt")?;
    log_file.write_all(format!("pos: {:?}\n", cursor::position()?).as_bytes())?;

    match config.wrap {
        WrapMode::Truncate => {
            // truncate to the first `width` graphemes
            let truncated = text
                .graphemes(true)
                .take(usize::from(width))
                .collect::<String>();
            log_file.write_all(format!("{}\n", truncated).as_bytes())?;
            write!(writer, "{}", truncated)?;
            writer.flush()?;
            Ok(1)
        }
        WrapMode::WordWrap => todo!(),
        WrapMode::CharacterWrap => todo!(),
    }
}
