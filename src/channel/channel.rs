use std::collections::HashSet;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChannelCreationErr {
    #[error("the empty string is not a valid channel name")]
    NoChannelName,
    #[error("channel {} had an invalid kind", .0)]
    InvalidKind(String),
}

#[derive(Debug)]
pub struct Channel {
    // the name of the channel, **including** the channel kind prefix (so it is suitable for direct
    // use as a target)
    name: String,
    // the kind of the channel, determined by its name
    kind: ChannelKind,
    pub modes: String,
    topic: String,
    // TODO: represent users better, may need to be `HashSet<Arc<User>>`?
    pub users: HashSet<String>,
}

impl Channel {
    /// creates a channel from the name of the channel, using default values for the modes, topic,
    /// and users
    pub fn new(name: impl Into<String>) -> Result<Self, ChannelCreationErr> {
        let name: String = name.into();
        if name.len() == 0 {
            return Err(ChannelCreationErr::NoChannelName);
        }

        // UNWRAP: there exists at least one character because of the len check above
        let kind = ChannelKind::parse(name.chars().next().unwrap())
            .ok_or_else(|| ChannelCreationErr::InvalidKind(name.clone()))?;

        Ok(Self {
            name,
            kind,
            modes: String::new(),
            topic: String::new(),
            users: HashSet::new(),
        })
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    // a standard `#` prefixed channel
    Regular,
    // a local channel, denoted with `&`. does not persist across the network.
    Local,
}

impl ChannelKind {
    pub fn parse(c: char) -> Option<Self> {
        match c {
            '#' => Some(ChannelKind::Regular),
            '&' => Some(ChannelKind::Local),
            _ => None,
        }
    }
}
