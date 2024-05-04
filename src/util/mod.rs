pub mod unicode_width;

use core::fmt::Display;

use crossterm::style::Stylize;

use crate::{
    constants::names::{
        CHANNEL_MEMBERSHIP_PREFIXES, CHANNEL_TYPES, INVALID_NICKNAME_CHARACTERS,
        INVALID_NICKNAME_START,
    },
    ui::text::Line,
};

pub fn line_now() -> Line<'static> {
    const FMT: &str = "%H:%M:%S";
    let now = chrono::Local::now();

    Line::default()
        .push_unstyled("[")
        .push(now.format(FMT).to_string().red())
        .push_unstyled("]")
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Target {
    Channel(String),
    Nickname(String),
    Status,
}

impl Target {
    pub fn new(s: impl Into<String>) -> Option<Self> {
        let s: String = s.into();
        let Some(first) = s.chars().next() else {
            return None;
        };

        // everything that starts with a channel type is a channel
        if CHANNEL_TYPES.contains(&first) {
            return Some(Target::Channel(s));
        }

        if !INVALID_NICKNAME_START.contains(&first)
            && !CHANNEL_MEMBERSHIP_PREFIXES.contains(&first)
            && !s.chars().any(|c| INVALID_NICKNAME_CHARACTERS.contains(&c))
        {
            return Some(Target::Nickname(s));
        } else {
            return None;
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Target::Channel(channel) => channel.as_str(),
            Target::Nickname(nick) => nick.as_str(),
            Target::Status => "[STATUS]",
        }
    }
}

impl Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
