use core::time::Duration;
use std::{collections::VecDeque, fs::File, io, io::Write};

use crossterm::{
    event,
    event::{Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{self, disable_raw_mode, enable_raw_mode},
};
use eyre::bail;
use ratatui::{
    prelude::*,
    widgets::{Paragraph, Wrap},
};

pub struct TerminalUi<B: Backend + io::Write> {
    terminal: Terminal<B>,
    layout: Layout,
    main_text: VecDeque<Line<'static>>,
    input_buffer: String,

    /// if this is true, the terminal is no longer functional and should not be used
    disabled: bool,
}

impl<B: Backend + io::Write> TerminalUi<B> {
    pub fn new(backend: B) -> io::Result<Self> {
        let mut terminal = Terminal::new(backend)?;
        execute!(terminal.backend_mut(), terminal::EnterAlternateScreen)?;
        enable_raw_mode()?;
        terminal.clear()?;

        // reserve 1 line at the bottom of the terminal
        let layout = Layout::vertical([Constraint::Fill(1), Constraint::Max(1)]);

        Ok(Self {
            terminal,
            layout,
            main_text: VecDeque::with_capacity(25),
            input_buffer: String::new(),
            disabled: false,
        })
    }

    pub fn writeln(&mut self, line: impl Into<Line<'static>>) -> eyre::Result<()> {
        self.main_text.push_back(line.into());
        // update the screen
        self.render()?;
        Ok(())
    }

    pub fn debug(&mut self, msg: impl AsRef<str>) {
        const STYLE: Style = Style::new().fg(Color::DarkGray);
        self.main_text
            .push_back(Line::styled(format!("DEBUG: {}", msg.as_ref()), STYLE));
        self.render();
    }

    pub fn warn(&mut self, msg: impl AsRef<str>) {
        const STYLE: Style = Style::new().fg(Color::Yellow);
        self.main_text
            .push_back(Line::styled(format!("WARN: {}", msg.as_ref()), STYLE));
        self.render();
    }

    pub fn error(&mut self, msg: impl AsRef<str>) {
        const STYLE: Style = Style::new().fg(Color::Red);
        self.main_text
            .push_back(Line::styled(format!("ERROR: {}", msg.as_ref()), STYLE));
        self.render();
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
        execute!(self.terminal.backend_mut(), terminal::LeaveAlternateScreen)
            .expect("unable to leave alternate screen");
        disable_raw_mode().expect("unable to disable raw mode");
        self.disabled = true;
    }

    fn render(&mut self) -> eyre::Result<()> {
        let [main_rect, input_rect] = &*self.layout.split(self.terminal.size()?) else {
            bail!("incorrect number of components in split layout");
        };

        // remove all lines that will not be visible after wrapping
        {
            let mut height = 0;
            // NOTE: the index here is the index from the *back* of the vec
            if let Some(first_hidden_rev) = self.main_text.iter().rev().position(|line| {
                let new_hight = calc_line_height(main_rect, line);
                if height + new_hight > main_rect.height {
                    return true;
                } else {
                    height += new_hight;
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

        self.terminal.draw(|frame| {
            let paragraph = Paragraph::new(Text::from(
                self.main_text.iter().cloned().collect::<Vec<_>>(),
            ))
            .wrap(Wrap { trim: false });
            frame.render_widget(paragraph, *main_rect);
            frame.render_widget(Span::from(self.input_buffer.clone()).on_blue(), *input_rect);
        })?;

        Ok(())
    }
}

fn calc_line_height(text_rect: &Rect, line: &Line<'static>) -> u16 {
    // NOTE: the max makes sure that empty lines are considered to be at least 1 high
    u16::max(
        line.width().div_ceil(usize::from(text_rect.width)) as u16,
        1,
    )
}

#[derive(Debug)]
pub enum InputStatus {
    Complete(String),
    Incomplete,
    Quit,
    IoErr(io::Error),
}
