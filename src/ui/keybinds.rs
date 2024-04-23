use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use log::debug;

pub enum Action {
    /// terminal resized
    Resize,
    /// typing a character
    Type(char),
    Enter,
    Backspace,
    Delete,

    /// scroll one line backwards in the history of the current window
    PreviousLine,
    /// scroll one line forwards in the history of the current window
    NextLine,

    /// move the cursor one character backwards in the input buffer
    PreviousCharacter,
    /// move the cursor one character forwards in the input buffer
    NextCharacter,

    /// move the cursor to the first character in the input buffer
    FirstCharacter,
    /// move the cursor to the last character in the input buffer
    LastCharacter,

    PreviousWindow,
    NextWindow,
}

impl Action {
    pub fn create(
        KeyEvent {
            code,
            modifiers,
            kind,
            ..
        }: KeyEvent,
    ) -> Option<Self> {
        // only detecting key down
        if kind != KeyEventKind::Press {
            return None;
        }

        debug!("mods {:?} code {:?}", modifiers, code);

        if modifiers.contains(KeyModifiers::CONTROL) {
            match code {
                KeyCode::Char(KEY_PREV_LINE_BASE) => Some(Action::PreviousLine),
                KeyCode::Char(KEY_NEXT_LINE_BASE) => Some(Action::NextLine),
                KeyCode::Char(KEY_PREV_CHAR_BASE) => Some(Action::PreviousCharacter),
                KeyCode::Char(KEY_NEXT_CHAR_BASE) => Some(Action::NextCharacter),
                KeyCode::Char(KEY_FIRST_CHAR_BASE) => Some(Action::FirstCharacter),
                KeyCode::Char(KEY_LAST_CHAR_BASE) => Some(Action::LastCharacter),
                KeyCode::Char(KEY_DELETE_BASE) => Some(Action::Delete),
                KeyCode::Char(KEY_PREV_WINDOW_BASE) => Some(Action::PreviousWindow),
                KeyCode::Char(KEY_NEXT_WINDOW_BASE) => Some(Action::NextWindow),
                _ => None,
            }
        } else {
            match code {
                KeyCode::Enter => Some(Action::Enter),
                KeyCode::Char(c) => Some(Action::Type(c)),
                KeyCode::Backspace => Some(Action::Backspace),
                KeyCode::Delete => Some(Action::Delete),
                _ => None,
            }
        }
    }
}

// these navigations are very emacs inspired
const KEY_PREV_LINE_BASE: char = 'p';
const KEY_NEXT_LINE_BASE: char = 'n';
const KEY_PREV_CHAR_BASE: char = 'b';
const KEY_NEXT_CHAR_BASE: char = 'f';
const KEY_FIRST_CHAR_BASE: char = 'a';
const KEY_LAST_CHAR_BASE: char = 'e';
const KEY_DELETE_BASE: char = 'd';
const KEY_PREV_WINDOW_BASE: char = 'q';
const KEY_NEXT_WINDOW_BASE: char = 'j';

/*


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

*/
