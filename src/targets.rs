use crate::{
    channel::{ChannelName, Nickname},
    constants::names::CHANNEL_TYPES,
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

        // everything that starts with a channel type is a channel, everything else is a nick, if
        // it's valid
        if CHANNEL_TYPES.contains(&first) {
            Some(Target::Channel(ChannelName::new(s)?))
        } else {
            Nickname::new(s).map(|n| Target::Nickname(n))
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Target::Channel(channel_name) => channel_name.as_str(),
            Target::Nickname(nick) => nick.as_str(),
            Target::Status => "[STATUS]",
        }
    }
}
