use core::{
    fmt::{Debug, Display},
    num::NonZeroU16,
};
use std::{fs::File, io, io::prelude::Write as _};

use crossterm::{
    cursor, execute,
    style::{ContentStyle, StyledContent},
};
use unicode_segmentation::UnicodeSegmentation;

use crate::ui::layout::Rect;

#[derive(Default)]
pub struct Line<'a> {
    content: Vec<DynStyledContentWrapper<'a>>,
}

impl<'a> Line<'a> {
    pub fn new_without_style<S: AsRef<str>>(s: S) -> Option<Line<'a>> {
        let content = s.as_ref().to_string();
        if !content.contains('\n') {
            Some(Self {
                content: vec![DynStyledContentWrapper {
                    style: ContentStyle::default(),
                    content: Box::new(content),
                }],
            })
        } else {
            None
        }
    }

    pub fn push<D: Display>(mut self, styled: StyledContent<D>) -> Self {
        self.content.push(DynStyledContentWrapper {
            style: *styled.style(),
            content: Box::new(styled.content().to_string()),
        });
        self
    }

    pub fn push_with_style<S: AsRef<str>>(mut self, content: S, style: ContentStyle) -> Self {
        let content = content.as_ref().replace(' ', "");
        self.content.push(DynStyledContentWrapper {
            style,
            content: Box::new(content),
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
                NonZeroU16::new(self.wrap(wrap, width).len() as u16).unwrap()
            }
        }
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
            // FIXME: implement this
            WrapMode::WordWrap => vec![],
            WrapMode::CharacterWrap => vec![],
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

    let mut log_file = File::options()
        .create(true)
        .append(true)
        .open("log_render.txt")?;
    // log_file.write_all(format!("pos: {:?}\n", cursor::position()?).as_bytes())?;

    let lines = line.wrap(config.wrap, width);
    let line_count = lines.len() as u16;
    log_file.write_all(format!("line: {:#?}\n", lines).as_bytes())?;
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
