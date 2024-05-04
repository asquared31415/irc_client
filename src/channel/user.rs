use std::collections::VecDeque;

use crate::{
    constants::names::{
        CHANNEL_MEMBERSHIP_PREFIXES, INVALID_NICKNAME_CHARACTERS, INVALID_NICKNAME_START,
    },
    ui::text::Line,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// the nickname of a user
pub struct Nickname(String);

impl Nickname {
    pub fn new(nick: impl Into<String>) -> Option<Self> {
        let nick: String = nick.into();

        let Some(first) = nick.chars().next() else {
            return None;
        };

        if !INVALID_NICKNAME_START.contains(&first)
            && !CHANNEL_MEMBERSHIP_PREFIXES.contains(&first)
            && !nick
                .chars()
                .any(|c| INVALID_NICKNAME_CHARACTERS.contains(&c))
        {
            Some(Self(nick))
        } else {
            None
        }
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug)]
pub struct UserMessages {
    nick: Nickname,
    pub messages: VecDeque<Line<'static>>,
}

impl UserMessages {
    pub fn new(nick: Nickname) -> Self {
        Self {
            nick,
            messages: VecDeque::new(),
        }
    }

    pub fn add_line(&mut self, line: Line<'static>) {
        self.messages.push_back(line);
    }

    pub fn iter_lines(&self) -> impl DoubleEndedIterator<Item = &Line<'_>> {
        self.messages.iter()
    }
}
