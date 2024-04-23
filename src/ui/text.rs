use core::{
    fmt::{Debug, Display, Write as _},
    num::NonZeroU16,
};
use std::io;

use crossterm::{
    cursor, execute,
    style::{ContentStyle, StyledContent},
};
use log::trace;
use unicode_segmentation::UnicodeSegmentation;

use crate::{ui::layout::Rect, util::unicode_width};

#[derive(Default, Debug)]
pub struct Line<'a> {
    content: Vec<DynStyledContentWrapper<'a>>,
}

impl<'a> Line<'a> {
    pub fn push<D: Display>(mut self, styled: StyledContent<D>) -> Self {
        self.content.push(DynStyledContentWrapper {
            style: *styled.style(),
            content: Box::new(styled.content().to_string()),
        });
        self
    }

    pub fn push_unstyled<S: AsRef<str>>(mut self, content: S) -> Self {
        let content = content.as_ref().replace('\r', "").replace('\n', "");
        self.content.push(DynStyledContentWrapper {
            style: ContentStyle::default(),
            content: Box::new(content),
        });
        self
    }

    pub fn into_iter(self) -> impl IntoIterator<Item = DynStyledContentWrapper<'a>> {
        self.content.into_iter()
    }

    pub fn wrapped_height(&self, wrap: WrapMode, width: u16) -> NonZeroU16 {
        match wrap {
            WrapMode::Truncate => NonZeroU16::new(1).unwrap(),
            WrapMode::WordWrap | WrapMode::CharacterWrap => {
                let lines = self.wrap(wrap, width);
                NonZeroU16::new(usize::max(lines.len(), 1) as u16).unwrap()
            }
        }
    }

    pub fn fmt_unstyled(&self) -> String {
        let mut out = String::new();
        for span in self.content.iter() {
            // writing to String can only fail on OoM, and that aborts
            let _ = write!(&mut out, "{}", span.content);
        }
        out
    }

    fn wrap(&self, wrap: WrapMode, width: u16) -> Vec<Vec<StyledContent<String>>> {
        // remove the style from the content by displaying the base content. this is then used to
        // determine what is a grapheme for the purposes of wrapping.
        match wrap {
            WrapMode::Truncate => {
                let mut ret = Vec::new();
                let mut remaining_width = width;
                for span in self.content.iter() {
                    if remaining_width == 0 {
                        break;
                    }

                    let unstyled = span.content.to_string();
                    // truncate to the first `width` graphemes
                    let truncated = unstyled
                        .graphemes(true)
                        .take(usize::from(remaining_width))
                        .collect::<String>();
                    remaining_width = remaining_width.saturating_sub(truncated.len() as u16);

                    ret.push(StyledContent::new(span.style, truncated));
                }
                // this truncates to one line always
                vec![ret]
            }
            WrapMode::WordWrap => {
                let mut remaining_width = width;
                let mut lines = vec![vec![]];

                fn handle_word(
                    lines: &mut Vec<Vec<StyledContent<String>>>,
                    width: u16,
                    remaining_width: &mut u16,
                    word: &str,
                    style: ContentStyle,
                ) {
                    trace!(
                        "handling word {:?}, width {}, remaining {}",
                        word,
                        width,
                        remaining_width
                    );

                    let len = unicode_width::display_width(word) as u16;
                    if len <= *remaining_width {
                        trace!("word on same line");
                        lines
                            .last_mut()
                            .unwrap()
                            .push(StyledContent::new(style, word.to_string()));
                        *remaining_width -= len;
                    } else if len >= width {
                        // this word will never fit on one line!
                        trace!("word would NEVER fit: {:?}/{}", word, width);
                        // split the graphemes at the end of the line
                        let (this_line, next) = word.graphemes(true).partition::<String, _>(|g| {
                            if *remaining_width > 0 {
                                *remaining_width -= unicode_width::display_width(g) as u16;
                                true
                            } else {
                                false
                            }
                        });
                        lines
                            .last_mut()
                            .unwrap()
                            .push(StyledContent::new(style, this_line.to_string()));
                        // wrap
                        lines.push(vec![]);
                        *remaining_width = width;
                        handle_word(lines, width, remaining_width, next.as_str(), style);
                    } else {
                        trace!("word on next line");
                        // if a word can't fit in the remaining space, but would be fine on the
                        // next line, wrap
                        lines.push(vec![StyledContent::new(style, word.to_string())]);
                        *remaining_width = width - len;
                    }
                }

                for span in self.content.iter() {
                    let unstyled = span.content.to_string();
                    let mut words = unstyled.split_word_bounds().peekable();
                    while let Some(word) = words.next() {
                        handle_word(&mut lines, width, &mut remaining_width, word, span.style);
                    }
                }

                lines
            }
            WrapMode::CharacterWrap => {
                // TODO: implement this
                vec![]
            }
        }
    }
}

impl<'a> Display for Line<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for span in self.content.iter() {
            write!(f, "{}", span)?;
        }
        Ok(())
    }
}

impl<'a, A: Into<DynStyledContentWrapper<'a>>> Extend<A> for Line<'a> {
    fn extend<T: IntoIterator<Item = A>>(&mut self, iter: T) {
        self.content.extend(iter.into_iter().map(Into::into));
    }
}

impl<'a> From<String> for Line<'a> {
    fn from(value: String) -> Self {
        Self {
            content: vec![DynStyledContentWrapper {
                style: ContentStyle::default(),
                content: Box::new(value),
            }],
        }
    }
}

impl<'a> From<&'a str> for Line<'a> {
    fn from(value: &'a str) -> Self {
        Self {
            content: vec![DynStyledContentWrapper {
                style: ContentStyle::default(),
                content: Box::new(value),
            }],
        }
    }
}

/// a wrapper for content that is not generic, so that different content types can be used
pub struct DynStyledContentWrapper<'a> {
    style: ContentStyle,
    content: Box<dyn Display + Send + 'a>,
}

impl<'a> Display for DynStyledContentWrapper<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", StyledContent::new(self.style, &self.content))
    }
}

impl<'a> Debug for DynStyledContentWrapper<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynStyledContentWrapper")
            .field("style", &self.style)
            .field("content", &self.content.to_string())
            .finish()
    }
}

impl<'a, D: Display + 'a> From<StyledContent<D>> for DynStyledContentWrapper<'static> {
    fn from(value: StyledContent<D>) -> Self {
        Self {
            style: ContentStyle::default(),
            content: Box::new(value.content().to_string()),
        }
    }
}

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
pub fn draw_text<'a>(
    writer: &mut impl io::Write,
    rect: Rect,
    line: &Line<'a>,
    config: DrawTextConfig,
) -> eyre::Result<u16> {
    let Rect {
        x,
        y,
        width,
        height: _,
    } = rect;
    execute!(writer, cursor::MoveTo(x, y))?;

    let lines = line.wrap(config.wrap, width);
    let line_count = lines.len() as u16;
    match lines.as_slice() {
        [] => {}
        [start @ .., last] => {
            for line in start {
                for span in line {
                    write!(writer, "{}", span)?;
                }
                write!(writer, "\r\n")?;
                writer.flush()?;
            }
            for span in last {
                write!(writer, "{}", span)?;
            }
            writer.flush()?;
        }
    }

    Ok(line_count)
}
