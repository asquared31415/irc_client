use core::time::Duration;
use std::{collections::VecDeque, fs::File, io, io::Write};

use crossterm::{
    cursor, event,
    event::{Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{self, disable_raw_mode, enable_raw_mode, ClearType},
};
use eyre::bail;

use crate::ui::{
    layout::{Layout, Rect},
    rendering,
    rendering::{DrawTextConfig, WrapMode},
};

pub struct TerminalUi<'a> {
    terminal: Box<dyn io::Write + 'a + Send>,
    layout: Layout,
    main_text: VecDeque<String>,
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
        Ok(Self {
            terminal,
            layout,
            main_text: VecDeque::with_capacity(25),
            input_buffer: String::new(),
            log_file,
            disabled: false,
        })
    }

    pub fn writeln(&mut self, line: impl Into<String>) -> eyre::Result<()> {
        let line = line.into();
        self.log_file.write_all(format!("{}\n", line).as_bytes())?;
        self.main_text.push_back(line);
        // update the screen
        self.render()?;
        Ok(())
    }

    pub fn debug(&mut self, msg: impl Into<String>) {
        //        const STYLE: Style = Style::new().fg(Color::DarkGray);
        let msg = msg.into();
        let _ = self.log_file.write_all(format!("{}\n", msg).as_bytes());
        self.main_text.push_back(format!("DEBUG: {}", msg));
        self.render();
    }

    pub fn warn(&mut self, msg: impl Into<String>) {
        //        const STYLE: Style = Style::new().fg(Color::Yellow);
        let msg = msg.into();
        let _ = self.log_file.write_all(format!("{}\n", msg).as_bytes());
        self.main_text.push_back(format!("WARN: {}", msg));

        self.render();
    }

    pub fn error(&mut self, msg: impl Into<String>) {
        //        const STYLE: Style = Style::new().fg(Color::Red);
        let msg = msg.into();
        let _ = self.log_file.write_all(format!("{}\n", msg).as_bytes());
        self.main_text.push_back(format!("ERROR: {}", msg));
        self.render();
    }

    /// reports a fatal error. this function first disables raw mode and returns to the main buffer
    /// so that the
    pub fn fatal(&mut self, msg: impl Into<String>) {
        self.disable();
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
            Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                state,
            }) => {
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

    fn render(&mut self) -> eyre::Result<()> {
        let layout = self.layout.calc(terminal::size()?);
        let [main_rect, input_rect] = layout.as_slice() else {
            bail!("incorrect number of components in split layout");
        };
        self.log_file
            .write_all(format!("main: {:#?}\ninput: {:#?}\n", main_rect, input_rect).as_bytes())?;

        // remove all lines that will not be visible after wrapping
        {
            let mut height = 0;
            // NOTE: the index here is the index from the *back* of the vec
            if let Some(first_hidden_rev) = self.main_text.iter().rev().position(|line| {
                let new_height = line_wrapped_height(main_rect, line.as_str());
                //let _ = self
                //    .log_file
                //    .write_all(format!("{}LINE:{}\n", new_height, line).as_bytes());
                if height + new_height > main_rect.height {
                    return true;
                } else {
                    height += new_height;
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
                self.main_text.push_front(String::from(""));
            }
        }

        let (cursor_col, cursor_row) = cursor::position()?;

        rendering::draw_text(
            &mut self.terminal,
            main_rect,
            String::from("MEOW"),
            DrawTextConfig {
                wrap: WrapMode::WordWrap,
            },
        );

        // self.terminal.draw(|frame| {
        //     let paragraph = Paragraph::new(Text::from(
        //         self.main_text.iter().cloned().collect::<Vec<_>>(),
        //     ))
        //     .wrap(Wrap { trim: false });
        //     frame.render_widget(paragraph, *main_rect);
        //     let start_idx = self
        //         .input_buffer
        //         .len()
        //         .saturating_sub(usize::from(input_rect.width));
        //     let input_to_show = self
        //         .input_buffer
        //         .get(start_idx..self.input_buffer.len())
        //         .unwrap();
        //     frame.render_widget(Span::from(input_to_show.to_string()).on_blue(), *input_rect);
        // })?;

        Ok(())
    }
}

fn line_wrapped_height(text_rect: &Rect, line: &str) -> u16 {
    1
    // let p = Paragraph::new(line.clone()).wrap(Wrap { trim: false });
    // p.line_count(text_rect.width) as u16
}

#[derive(Debug)]
pub enum InputStatus {
    Complete(String),
    Incomplete,
    Quit,
    IoErr(io::Error),
}
