use core::time::Duration;
use std::{collections::VecDeque, io};

use crossterm::{
    cursor, event,
    event::Event,
    execute,
    style::{Color, Stylize},
    terminal,
};
use eyre::bail;
use log::*;

use crate::{
    state::StatusInfo,
    ui::{
        input_buffer::InputBuffer,
        keybinds::Action,
        layout::{Layout, Rect},
        text,
        text::{DrawTextConfig, Line, WrapMode},
    },
    util::unicode_width,
};

pub struct TerminalUi<'a> {
    terminal: Box<dyn io::Write + 'a + Send>,
    layout: Layout,
    history: VecDeque<Line<'a>>,
    /// the last `scrollback` messages of the history should be hidden (would be below the screen)
    pub scrollback: usize,
    pub input_buffer: InputBuffer,
}

impl<'a> TerminalUi<'a> {
    pub fn new<W: io::Write + 'a + Send>(layout: Layout, writer: W) -> eyre::Result<Self> {
        let mut terminal = Box::new(writer) as Box<dyn io::Write + Send>;
        execute!(terminal, terminal::EnterAlternateScreen)?;
        terminal::enable_raw_mode()?;
        execute!(terminal, terminal::Clear(terminal::ClearType::Purge))?;

        Ok(Self {
            terminal,
            layout,
            history: VecDeque::new(),
            scrollback: 0,
            input_buffer: InputBuffer::default(),
        })
    }

    pub fn error(&mut self, msg: impl Into<String>) -> eyre::Result<()> {
        let msg = msg.into();
        error!("{}", msg);
        self.history
            .push_back(Line::default().push(format!("ERROR: {}", msg).red()));
        Ok(())
    }

    pub fn raw_input(&mut self) -> Result<Option<Action>, io::Error> {
        const POLL_TIMEOUT: Duration = Duration::from_micros(100);

        match event::poll(POLL_TIMEOUT) {
            Ok(true) => {}
            // don't need to re-render if there's not a message to handle
            Ok(false) => return Ok(None),
            Err(e) => return Err(e),
        }

        let Ok(event) = event::read() else {
            unreachable!("poll claimed to be ready")
        };

        match event {
            Event::Key(key_event) => Ok(Action::create(key_event)),
            Event::Resize(_, _) => Ok(Some(Action::Resize)),
            Event::FocusGained | Event::FocusLost | Event::Mouse(_) | Event::Paste(_) => Ok(None),
        }
    }

    pub fn disable(&mut self) {
        execute!(self.terminal, terminal::LeaveAlternateScreen)
            .expect("unable to leave alternate screen");
        terminal::disable_raw_mode().expect("unable to disable raw mode");
    }

    const MAIN_TEXT_WRAP_MODE: WrapMode = WrapMode::WordWrap;

    /// lines contains the full history of lines to render
    pub fn render<'line, 'lines>(
        &mut self,
        status: &StatusInfo,
        lines: impl DoubleEndedIterator<Item = &'lines Line<'lines>>,
    ) -> eyre::Result<()> {
        let layout = self.layout.calc(terminal::size()?);
        let [main_rect, status_rect, input_rect] = layout.as_slice() else {
            bail!("incorrect number of components in split layout");
        };

        // TODO: save and restore cursor pos?
        execute!(self.terminal, terminal::Clear(terminal::ClearType::All))?;

        self.draw_main(*main_rect, lines)?;
        self.draw_status(status, *status_rect)?;
        self.draw_input(input_rect)?;

        Ok(())
    }

    fn draw_main<'line>(
        &mut self,
        main_rect: Rect,
        lines: impl DoubleEndedIterator<Item = &'line Line<'line>>,
    ) -> eyre::Result<()> {
        let mut lines_used = 0;
        let to_show_rev = lines
            .rev()
            .skip(self.scrollback)
            .map(|line| {
                (
                    line,
                    line.wrapped_height(Self::MAIN_TEXT_WRAP_MODE, main_rect.width),
                )
            })
            .take_while(|(_, height)| {
                let new_height = height.get();
                if lines_used + new_height <= main_rect.height {
                    lines_used += new_height;
                    true
                } else {
                    false
                }
            });

        let mut draw_y = main_rect.y + main_rect.height;
        for (line, height) in to_show_rev {
            // move the start of the line up to hold the line
            draw_y -= height.get();
            text::draw_text(
                &mut self.terminal,
                Rect {
                    x: main_rect.x,
                    y: draw_y,
                    width: main_rect.width,
                    height: height.get(),
                },
                line,
                DrawTextConfig {
                    wrap: Self::MAIN_TEXT_WRAP_MODE,
                },
            )?;
        }

        Ok(())
    }

    fn draw_status(&mut self, status: &StatusInfo, status_rect: Rect) -> eyre::Result<()> {
        const STATUS_BG: Color = Color::Rgb {
            r: 0x61,
            g: 0x2B,
            b: 0x5B,
        };
        const ADDR: Color = Color::Rgb {
            r: 0xF4,
            g: 0x5B,
            b: 0x46,
        };
        const NICK: Color = Color::Rgb {
            r: 0xFC,
            g: 0x91,
            b: 0x00,
        };

        let mut status_line = Line::default().push(status.addr.clone().with(ADDR).on(STATUS_BG));
        if !status.registered {
            status_line = status_line.push(" *REGISTRATION*".on(STATUS_BG));
        }
        status_line = status_line
            .push(format!(" {}", status.nick).with(NICK).on(STATUS_BG))
            .push(format!(" - {}", status.target.as_str()).on(STATUS_BG));

        let pad = usize::from(status_rect.width).saturating_sub(unicode_width::display_width(
            status_line.fmt_unstyled().as_str(),
        ));

        status_line = status_line.push(" ".repeat(pad).on(STATUS_BG));

        text::draw_text(
            &mut self.terminal,
            status_rect,
            &status_line,
            DrawTextConfig {
                wrap: Self::MAIN_TEXT_WRAP_MODE,
            },
        )?;

        Ok(())
    }

    fn draw_input(&mut self, input_rect: &Rect) -> eyre::Result<()> {
        // leave space on the right hand side for clarity and to place the cursor without it
        // overlapping a character
        const INPUT_BUFFER_PAD: u16 = 1;

        let input_width = input_rect.width.saturating_sub(INPUT_BUFFER_PAD);
        let (input_text, cursor_col) = self.input_buffer.get_visible_area(input_width);
        trace!("input_text {:?}, cursor_col {}", input_text, cursor_col);

        let config = DrawTextConfig {
            // note: this does not matter, we always send exactly enough characters
            wrap: WrapMode::Truncate,
        };

        //pad with spaces
        text::draw_text(
            &mut self.terminal,
            *input_rect,
            &Line::default().push(" ".repeat(usize::from(input_rect.width)).on_blue()),
            config,
        )?;

        // draw the actual text
        text::draw_text(
            &mut self.terminal,
            *input_rect,
            &Line::default().push(input_text.white().on_blue()),
            config,
        )?;

        execute!(self.terminal, cursor::MoveToColumn(cursor_col as u16))?;

        Ok(())
    }
}
