use core::time::Duration;
use std::{collections::VecDeque, io};

use crossterm::{
    cursor, event,
    event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    style::Stylize,
    terminal::{self, disable_raw_mode, enable_raw_mode, ClearType},
};
use eyre::bail;
use log::{debug, error, info, trace, warn};

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

#[derive(Debug)]
pub enum UiMsg<'a> {
    Writeln(Line<'a>),
    Warn(String),
    /// cause the UI to render everything again, typically used in response to user input
    ReRender,
}

impl<'a> TerminalUi<'a> {
    pub fn new<W: io::Write + 'a + Send>(layout: Layout, writer: W) -> eyre::Result<Self> {
        let mut terminal = Box::new(writer) as Box<dyn io::Write + Send>;
        execute!(terminal, terminal::EnterAlternateScreen)?;
        enable_raw_mode()?;
        execute!(terminal, terminal::Clear(ClearType::Purge))?;

        let mut this = Self {
            terminal,
            layout,
            history: VecDeque::new(),
            scrollback: 0,
            input_buffer: InputBuffer::default(),
        };
        // force a re-render to move the cursor and add the input background
        this.render()?;
        Ok(this)
    }

    pub fn handle_msg(&mut self, msg: UiMsg<'a>) -> eyre::Result<()> {
        debug!("{:#?}", msg);
        match msg {
            UiMsg::Writeln(line) => {
                self.writeln(line)?;
                self.render()?;
            }
            UiMsg::Warn(msg) => {
                self.warn(msg)?;
                self.render()?;
            }
            UiMsg::ReRender => {
                self.render()?;
            }
        }
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
        self.render()?;
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
        disable_raw_mode().expect("unable to disable raw mode");
    }

    pub fn render(&mut self) -> eyre::Result<()> {
        let layout = self.layout.calc(terminal::size()?);
        let [main_rect, input_rect] = layout.as_slice() else {
            bail!("incorrect number of components in split layout");
        };

        const MAIN_TEXT_WRAP_MODE: WrapMode = WrapMode::WordWrap;

        let mut lines_used = 0;
        let mut shown_lines = self
            .history
            .iter()
            .rev()
            .skip(self.scrollback)
            .take_while(|line| {
                let new_height = line
                    .wrapped_height(MAIN_TEXT_WRAP_MODE, main_rect.width)
                    .get();
                if lines_used + new_height <= main_rect.height {
                    lines_used += new_height;
                    true
                } else {
                    false
                }
            })
            .collect::<Vec<_>>();
        shown_lines.reverse();

        // this has to be out here because it has to be in the same scope as the vec for borrow
        // checker reasons, since the vec has to hold a reference since lines can't be moved or
        // cloned.
        let line = Line::default();
        // pad shown_lines at the top to ensure that the last line it shows is placed at the bottom
        for _ in 0..(main_rect.height - lines_used) {
            shown_lines.insert(0, &line);
        }

        // TODO: save and restore cursor pos?
        execute!(self.terminal, terminal::Clear(ClearType::All))?;

        let mut draw_rect = *main_rect;
        for line in shown_lines.iter() {
            let used = text::draw_text(
                &mut self.terminal,
                draw_rect,
                line,
                DrawTextConfig {
                    wrap: MAIN_TEXT_WRAP_MODE,
                },
            )?;

            draw_rect.y += used;
            draw_rect.height -= used;
        }

        self.draw_input(input_rect)?;

        Ok(())
    }

    fn draw_input(&mut self, input_rect: &Rect) -> eyre::Result<()> {
        // leave space on the right hand side for clarity and to place the cursor without it
        // overlapping a character
        const INPUT_BUFFER_PAD: u16 = 1;

        let input_width = input_rect.width.saturating_sub(INPUT_BUFFER_PAD);
        let (input_text, cursor_col) = self.input_buffer.get_visible_area(input_width);
        trace!("input_text {:?}, cursor_col {}", input_text, cursor_col);
        let mut input_text = input_text.to_string();

        // re-pad with spaces
        let needed_pad = usize::from(input_rect.width).saturating_sub(input_text.len());
        if needed_pad > 0 {
            trace!("padding with {} spaces", needed_pad);
            input_text.push_str(" ".repeat(needed_pad).as_str());
        }

        text::draw_text(
            &mut self.terminal,
            *input_rect,
            &Line::default().push(input_text.white().on_blue()),
            DrawTextConfig {
                // note: this does not matter, we always send exactly enough characters
                wrap: WrapMode::Truncate,
            },
        )?;

        execute!(self.terminal, cursor::MoveToColumn(cursor_col as u16))?;

        Ok(())
    }
}
