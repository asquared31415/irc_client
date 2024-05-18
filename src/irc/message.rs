use core::fmt::Debug;

use log::*;
use thiserror::Error;

use crate::{
    ext::StrExt as _,
    irc::{tags::Tags, IrcCommand, IrcCommandParseErr, IrcCommandToStringErr, Source},
};

#[derive(Debug, Clone)]
pub struct IrcMessage {
    pub tags: Tags,
    pub source: Option<Source>,
    pub message: IrcCommand,
}

impl IrcMessage {
    /// creates an irc message from just its command component. this should be used by all places in
    /// the client that need to produce a message, since clients should not be allowed to send a
    /// source.
    pub fn from_command(cmd: IrcCommand) -> Self {
        Self {
            tags: Tags::empty(),
            source: None,
            message: cmd,
        }
    }

    /// parses a message from a string. the string must contain only a single message. the string
    /// must not contain CRLF.
    pub fn parse(s: &str) -> Result<Self, IrcParseErr> {
        if s.contains("\r\n") {
            return Err(IrcParseErr::InteriorCRLF);
        }

        // not sure if this is valid, but just in case, trim leading whitespace.
        let mut s = s.trim_start_matches(' ');

        // optional tags section
        let tags = if s.starts_with('@') {
            s = &s[1..];
            let Some((tags, rest)) = s.split_once(' ') else {
                // if there's not a space after the tags, the command is missing
                return Err(IrcParseErr::MissingCommand);
            };
            s = rest;
            Tags::parse(tags).unwrap_or_else(|| Tags::empty())
        } else {
            Tags::empty()
        };

        // optional source section
        let source = if let Some((_, rest)) = s.split_prefix(':') {
            let Some((source, rest)) = rest.split_once(' ') else {
                // if there's not a space after the source, the command is missing
                return Err(IrcParseErr::MissingCommand);
            };

            s = rest;
            let source = Source::parse(source);
            trace!("parsed source: {:#?}", source);
            Some(source)
        } else {
            None
        };

        s = s.trim_start_matches(' ');
        if s.len() == 0 {
            return Err(IrcParseErr::MissingCommand);
        }

        Ok(IrcMessage {
            tags,
            source,
            message: IrcCommand::parse(s)?,
        })
    }

    // turns this message into a string that can be sent across the IRC connection directly. the
    // returned value includes the trailing CRLF that all messages must have.
    pub fn to_irc_string(&self) -> Result<String, IrcCommandToStringErr> {
        let mut message = String::new();

        // TODO
        // if let Some(_tags) = self.tags {
        //     todo!()
        // }

        // clients must never send a source to the server
        if self.source.is_some() {
            return Err(IrcCommandToStringErr::ClientMustNotSendSource);
        }

        message.push_str(self.message.to_irc_string()?.as_str());
        message.push_str("\r\n");
        Ok(message)
    }
}

#[derive(Debug, Error)]
pub enum IrcParseErr {
    #[error("message contains interior CRLF")]
    InteriorCRLF,
    #[error("message is missing a command")]
    MissingCommand,
    #[error(transparent)]
    MessageParseErr(#[from] IrcCommandParseErr),
}
