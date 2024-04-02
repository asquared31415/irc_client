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
    main_text: VecDeque<Line<'a>>,
    input_buffer: String,
    log_file: File,
    /// if this is true, the terminal is no longer functional and should not be used
    disabled: bool,
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
            main_text: VecDeque::with_capacity(25),
            input_buffer: String::new(),
            log_file,
            disabled: false,
        };
        // force a re-render to move the cursor and add the input background
        let _ = this.render();
        Ok(this)
    }

    pub fn writeln(&mut self, line: impl Into<Line<'a>>) -> eyre::Result<()> {
        let line = line.into();
        // self.log_file.write_all(format!("{}\n", line).as_bytes())?;
        self.main_text.push_back(line);
        // update the screen
        self.render()?;
        Ok(())
    }

    pub fn debug(&mut self, msg: impl Into<String>) -> eyre::Result<()> {
        let msg = msg.into();
        self.log_file
            .write_all(format!("DEBUG {}\n", msg).as_bytes())?;
        self.main_text
            .push_back(Line::default().push(format!("DEBUG: {}", msg).dark_grey()));
        self.render()?;
        Ok(())
    }

    pub fn warn(&mut self, msg: impl Into<String>) -> eyre::Result<()> {
        let msg = msg.into();
        self.log_file
            .write_all(format!("WARN {}\n", msg).as_bytes())?;
        self.main_text
            .push_back(Line::default().push(format!("WARN: {}", msg).yellow()));

        self.render()?;
        Ok(())
    }

    pub fn error(&mut self, msg: impl Into<String>) -> eyre::Result<()> {
        let msg = msg.into();
        self.log_file.write_all(format!("{}\n", msg).as_bytes())?;
        self.main_text
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
        self.disabled = true;
    }

    pub fn render(&mut self) -> eyre::Result<()> {
        let layout = self.layout.calc(terminal::size()?);
        let [main_rect, input_rect] = layout.as_slice() else {
            bail!("incorrect number of components in split layout");
        };
        // self.log_file
        //     .write_all(format!("main: {:#?}\ninput: {:#?}\n", main_rect,
        // input_rect).as_bytes())?;

        const MAIN_TEXT_WRAP_MODE: WrapMode = WrapMode::Truncate;

        // remove all lines that will not be visible after wrapping
        {
            let mut height = 0;
            // NOTE: the index here is the index from the *back* of the vec
            if let Some(first_hidden_rev) = self.main_text.iter().rev().position(|line| {
                let new_height = line.wrapped_height(MAIN_TEXT_WRAP_MODE, main_rect.width);
                let _ = self
                    .log_file
                    .write_all(format!("{}LINE:{}\n", new_height, line).as_bytes());
                if height + new_height.get() > main_rect.height {
                    return true;
                } else {
                    height += new_height.get();
                    return false;
                }
            }) {
                // remove all the lines that are no longer visible
                self.main_text
                    .drain(0..(self.main_text.len() - first_hidden_rev));
            }
            assert!(height <= main_rect.height);
            // buffer the top of the screen with empty lines if it was not filled
            for _ in 0..(main_rect.height - height) {
                self.main_text.push_front(Line::from(""));
            }
        }

        // TODO: save and restore cursor pos?
        execute!(self.terminal, terminal::Clear(ClearType::All))?;

        // self.log_file
        //     .write_all(format!("lines: {:#?}\n", self.main_text).as_bytes())?;

        let mut draw_rect = *main_rect;
        for line in self.main_text.iter() {
            // self.log_file
            //     .write_all(format!("rect: {:#?}\n", draw_rect).as_bytes())?;
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

        self.log_file
            .write_all(format!("input: {:#?} {:#?}\n", self.input_buffer, input_rect).as_bytes())?;

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
