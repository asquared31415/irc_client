use core::time::Duration;
use std::{collections::VecDeque, fs::File, io, io::Write};

use crossterm::{
    event,
    event::{Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    style::Stylize,
    terminal::{self, disable_raw_mode, enable_raw_mode, ClearType},
};
use eyre::bail;

use crate::ui::{
    layout::Layout,
    rendering,
    rendering::{DrawTextConfig, Line, WrapMode},
};

pub struct TerminalUi<'a> {
    terminal: Box<dyn io::Write + 'a + Send>,
    layout: Layout,
    history: VecDeque<Line<'a>>,
    /// the last `scrollback` messages of the history should be hidden (would be below the screen)
    scrollback: usize,
    input_buffer: String,
    log_file: File,
}

impl<'a> TerminalUi<'a> {
    pub fn new<W: io::Write + 'a + Send>(layout: Layout, writer: W) -> io::Result<Self> {
        let mut terminal = Box::new(writer) as Box<dyn io::Write + Send>;
        execute!(terminal, terminal::EnterAlternateScreen)?;
        enable_raw_mode()?;
        execute!(terminal, terminal::Clear(ClearType::Purge))?;

        let log_file = File::options().create(true).append(true).open("log.txt")?;
        let mut this = Self {
            terminal,
            layout,
            history: VecDeque::new(),
            scrollback: 0,
            input_buffer: String::new(),
            log_file,
        };
        // force a re-render to move the cursor and add the input background
        let _ = this.render();
        Ok(this)
    }

    pub fn writeln(&mut self, line: impl Into<Line<'a>>) -> eyre::Result<()> {
        let line = line.into();
        // self.log_file.write_all(format!("{}\n", line).as_bytes())?;
        self.history.push_back(line);
        // update the screen
        self.render()?;
        Ok(())
    }

    pub fn debug(&mut self, msg: impl Into<String>) -> eyre::Result<()> {
        let msg = msg.into();
        self.log_file
            .write_all(format!("DEBUG {}\n", msg).as_bytes())?;
        self.history
            .push_back(Line::default().push(format!("DEBUG: {}", msg).dark_grey()));
        self.render()?;
        Ok(())
    }

    pub fn warn(&mut self, msg: impl Into<String>) -> eyre::Result<()> {
        let msg = msg.into();
        self.log_file
            .write_all(format!("WARN {}\n", msg).as_bytes())?;
        self.history
            .push_back(Line::default().push(format!("WARN: {}", msg).yellow()));

        self.render()?;
        Ok(())
    }

    pub fn error(&mut self, msg: impl Into<String>) -> eyre::Result<()> {
        let msg = msg.into();
        self.log_file.write_all(format!("{}\n", msg).as_bytes())?;
        self.history
            .push_back(Line::default().push(format!("ERROR: {}", msg).red()));
        self.render()?;
        Ok(())
    }

    pub fn input(&mut self) -> InputStatus {
        const POLL_TIMEOUT: Duration = Duration::from_micros(100);

        match event::poll(POLL_TIMEOUT) {
            Ok(true) => {}
            Ok(false) => return InputStatus::Incomplete,
            Err(e) => return InputStatus::IoErr(e),
        }

        let Ok(event) = event::read() else { todo!() };

        match event {
            Event::Key(KeyEvent { code, kind, .. }) => {
                // only detecting key down
                if kind != KeyEventKind::Press {
                    return InputStatus::Incomplete;
                }

                match code {
                    KeyCode::Enter => {
                        let input = self.input_buffer.clone();
                        self.input_buffer.clear();
                        return InputStatus::Complete(input);
                    }
                    KeyCode::Esc => return InputStatus::Quit,
                    KeyCode::Modifier(_) => todo!(),
                    KeyCode::Char(c) => {
                        self.input_buffer.push(c);
                        self.render();
                        return InputStatus::Incomplete;
                    }
                    KeyCode::Backspace => {
                        self.input_buffer.pop();
                        self.render();
                        return InputStatus::Incomplete;
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
                    | KeyCode::Delete
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
                    | KeyCode::Media(_) => {
                        return InputStatus::Incomplete;
                    }
                }
            }
            Event::FocusGained
            | Event::FocusLost
            | Event::Mouse(_)
            | Event::Paste(_)
            | Event::Resize(_, _) => {
                return InputStatus::Incomplete;
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
        // self.log_file
        //     .write_all(format!("main: {:#?}\ninput: {:#?}\n", main_rect,
        // input_rect).as_bytes())?;

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
            let used = rendering::draw_text(
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

        let needed_buffer = input_rect.width as isize - self.input_buffer.len() as isize;
        let input_text = if needed_buffer > 0 {
            let mut text = self.input_buffer.clone();
            text.push_str(" ".repeat(needed_buffer as usize).as_str());
            text
        } else if needed_buffer == 0 {
            self.input_buffer.clone()
        } else {
            self.input_buffer[needed_buffer.abs() as usize..].to_string()
        };

        rendering::draw_text(
            &mut self.terminal,
            *input_rect,
            &Line::default().push(input_text.white().on_blue()),
            DrawTextConfig {
                // note: this does not matter, we always send exactly enough characters
                wrap: WrapMode::Truncate,
            },
        )?;

        Ok(())
    }
}

#[derive(Debug)]
pub enum InputStatus {
    Complete(String),
    Incomplete,
    Quit,
    IoErr(io::Error),
}
