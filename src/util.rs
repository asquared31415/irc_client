use crate::constants::names::{
    CHANNEL_MEMBERSHIP_PREFIXES, CHANNEL_TYPES, INVALID_NICKNAME_CHARACTERS, INVALID_NICKNAME_START,
};

#[derive(Debug, Clone)]
pub enum TargetKind {
    Channel(String),
    Nickname(String),
    Unknown(String),
}

impl TargetKind {
    pub fn new(s: impl Into<String>) -> Self {
        let s: String = s.into();
        let Some(first) = s.chars().next() else {
            return TargetKind::Unknown(s);
        };

        // everything that starts with a channel type is a channel
        if CHANNEL_TYPES.contains(&first) {
            return TargetKind::Channel(s);
        }

        if !INVALID_NICKNAME_START.contains(&first)
            && !CHANNEL_MEMBERSHIP_PREFIXES.contains(&first)
            && !s.chars().any(|c| INVALID_NICKNAME_CHARACTERS.contains(&c))
        {
            return TargetKind::Nickname(s);
        } else {
            return TargetKind::Unknown(s);
        }
    }
}
