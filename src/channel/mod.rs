mod channel;
mod mode;
mod user;

pub use channel::{Channel, ChannelCreationErr, ChannelKind};
pub use user::{Nickname, UserMessages};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// the name of a channel, including the channel type character (typically `#`)
pub struct ChannelName {
    /// the full name of the channel, including the kind character
    name: String,
    kind: ChannelKind,
}

impl ChannelName {
    pub fn new(name: impl Into<String>) -> Option<Self> {
        let name: String = name.into();

        // if no kind character, not valid
        let kind_char = name.chars().next()?;
        let kind = ChannelKind::parse(kind_char)?;

        Some(Self { name, kind })
    }

    pub fn as_str(&self) -> &str {
        self.name.as_str()
    }
}
