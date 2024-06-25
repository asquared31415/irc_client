use crate::channel::{ChannelName, Nickname};

#[derive(Debug, Clone)]
/// a target for an IRC command, a user or channel
pub enum Target {
    User(Nickname),
    Channel(ChannelName),
}

impl Target {
    pub fn as_str(&self) -> &str {
        match self {
            Target::User(nick) => nick.as_str(),
            Target::Channel(channel) => channel.as_str(),
        }
    }
}
