use crate::{
    channel::ChannelName,
    constants::names::{
        CHANNEL_MEMBERSHIP_PREFIXES, CHANNEL_TYPES, INVALID_NICKNAME_CHARACTERS,
        INVALID_NICKNAME_START,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// a target for a message
pub enum Target {
    Channel(ChannelName),
    Nickname(Nickname),
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
            return Some(Target::Channel(ChannelName::new(s)?));
        }

        if !INVALID_NICKNAME_START.contains(&first)
            && !CHANNEL_MEMBERSHIP_PREFIXES.contains(&first)
            && !s.chars().any(|c| INVALID_NICKNAME_CHARACTERS.contains(&c))
        {
            return Some(Target::Nickname(Nickname(s)));
        } else {
            return None;
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Target::Channel(channel_name) => channel_name.as_str(),
            Target::Nickname(_) => todo!(),
            Target::Status => "[STATUS]",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// the nickname of a user
pub struct Nickname(String);
