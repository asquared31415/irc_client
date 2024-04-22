use core::time::Duration;
use std::{collections::VecDeque, io};

use crossterm::{
    cursor, event,
    event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    style::Stylize,
    terminal,
};
use eyre::bail;
use log::*;

use crate::ui::{
    input_buffer::InputBuffer,
    layout::{Layout, Rect},
    text,
    text::{DrawTextConfig, Line, WrapMode},
};

pub struct TerminalUi<'a> {
    terminal: Box<dyn io::Write + 'a + Send>,
    layout: Layout,
    history: VecDeque<Line<'a>>,
    /// the last `scrollback` messages of the history should be hidden (would be below the screen)
    scrollback: usize,
    input_buffer: InputBuffer,
}

#[derive(Debug)]
pub enum InputStatus {
    Complete(String),
    Incomplete { rerender: bool },
    IoErr(io::Error),
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

    const MAIN_TEXT_WRAP_MODE: WrapMode = WrapMode::WordWrap;

    /// lines contains the full history of lines to render
    pub fn render<'line>(
        &mut self,
        lines: impl DoubleEndedIterator<Item = &'line Line<'line>>,
    ) -> eyre::Result<()> {
        let layout = self.layout.calc(terminal::size()?);
        let [main_rect, input_rect] = layout.as_slice() else {
            bail!("incorrect number of components in split layout");
        };

        // TODO: save and restore cursor pos?
        execute!(self.terminal, terminal::Clear(terminal::ClearType::All))?;

        self.draw_main(*main_rect, lines)?;
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

    fn writeln(&mut self, line: impl Into<Line<'a>>) -> eyre::Result<()> {
        let line = line.into();
        info!("{}", line.fmt_unstyled());
        self.history.push_back(line);
        Ok(())
    }

    fn warn(&mut self, msg: impl Into<String>) -> eyre::Result<()> {
        let msg = msg.into();
        warn!("{}", msg);
        self.history
            .push_back(Line::default().push(format!("WARN: {}", msg).yellow()));
        Ok(())
    }

    pub fn error(&mut self, msg: impl Into<String>) -> eyre::Result<()> {
        let msg = msg.into();
        error!("{}", msg);
        self.history
            .push_back(Line::default().push(format!("ERROR: {}", msg).red()));
        Ok(())
    }

    pub fn input(&mut self) -> InputStatus {
        const POLL_TIMEOUT: Duration = Duration::from_micros(100);

        match event::poll(POLL_TIMEOUT) {
            Ok(true) => {}
            // don't need to re-render if there's not a message to handle
            Ok(false) => return InputStatus::Incomplete { rerender: false },
            Err(e) => return InputStatus::IoErr(e),
        }

        let Ok(event) = event::read() else { todo!() };

        match event {
            Event::Key(KeyEvent {
                code,
                kind,
                modifiers,
                ..
            }) => {
                // only detecting key down
                if kind != KeyEventKind::Press {
                    // don't need to re-render if we do nothing
                    return InputStatus::Incomplete { rerender: false };
                }

                // KEYBINDS
                if modifiers.contains(KeyModifiers::CONTROL) {
                    if let KeyCode::Char(c) = code {
                        let status = match c {
                            // these navigations are very emacs inspired
                            'p' => {
                                self.scrollback = usize::min(
                                    self.scrollback.saturating_add(1),
                                    self.history.len().saturating_sub(1),
                                );
                                InputStatus::Incomplete { rerender: true }
                            }
                            'n' => {
                                // this can never over-scroll because saturating caps at 0
                                self.scrollback = self.scrollback.saturating_sub(1);
                                InputStatus::Incomplete { rerender: true }
                            }
                            'f' => {
                                self.input_buffer.offset(1);
                                InputStatus::Incomplete { rerender: true }
                            }
                            'b' => {
                                self.input_buffer.offset(-1);
                                InputStatus::Incomplete { rerender: true }
                            }
                            'a' => {
                                self.input_buffer.select(0);
                                InputStatus::Incomplete { rerender: true }
                            }
                            'e' => {
                                self.input_buffer.select(self.input_buffer.buffer().len());
                                InputStatus::Incomplete { rerender: true }
                            }
                            'd' => {
                                self.input_buffer.delete();
                                InputStatus::Incomplete { rerender: true }
                            }
                            // didn't handle a command, don't rerender
                            _ => InputStatus::Incomplete { rerender: false },
                        };
                        return status;
                    }
                }

                match code {
                    KeyCode::Enter => InputStatus::Complete(self.input_buffer.finish()),
                    KeyCode::Char(c) => {
                        self.input_buffer.insert(c);
                        InputStatus::Incomplete { rerender: true }
                    }
                    KeyCode::Backspace => {
                        self.input_buffer.backspace();
                        InputStatus::Incomplete { rerender: true }
                    }
                    KeyCode::Delete => {
                        self.input_buffer.delete();
                        InputStatus::Incomplete { rerender: true }
                    }

                    KeyCode::Right
                    | KeyCode::Left
                    | KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::Home
                    | KeyCode::End
                    | KeyCode::PageUp
                    | KeyCode::PageDown
                    | KeyCode::Tab
                    | KeyCode::BackTab
                    | KeyCode::Insert
                    | KeyCode::F(_)
                    | KeyCode::Null
                    | KeyCode::CapsLock
                    | KeyCode::ScrollLock
                    | KeyCode::NumLock
                    | KeyCode::PrintScreen
                    | KeyCode::Pause
                    | KeyCode::Menu
                    | KeyCode::KeypadBegin
                    | KeyCode::Media(_)
                    | KeyCode::Esc
                    | KeyCode::Modifier(_) => InputStatus::Incomplete { rerender: false },
                }
            }
            Event::Resize(_, _) => InputStatus::Incomplete { rerender: true },
            Event::FocusGained | Event::FocusLost | Event::Mouse(_) | Event::Paste(_) => {
                InputStatus::Incomplete { rerender: false }
            }
        }
    }

    pub fn disable(&mut self) {
        execute!(self.terminal, terminal::LeaveAlternateScreen)
            .expect("unable to leave alternate screen");
        terminal::disable_raw_mode().expect("unable to disable raw mode");
    }
}
