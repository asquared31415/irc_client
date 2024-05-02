use core::fmt::Debug;

use log::debug;
use thiserror::Error;

use crate::{
    ext::StrExt as _,
    irc_message::{Message, MessageParseErr, MessageToStringErr, Source},
};

#[derive(Debug, Clone)]
pub struct IrcMessage {
    pub tags: Option<()>,
    pub source: Option<Source>,
    pub message: Message,
}

impl IrcMessage {
    /// parses a message from a string. the string must contain only a single message. the string
    /// must not contain CRLF.
    pub fn parse(s: &str) -> Result<Self, IrcParseErr> {
        if s.contains("\r\n") {
            return Err(IrcParseErr::InteriorCRLF);
        }

        // not sure if this is valid, but just in case, trim leading whitespace.
        let mut s = s.trim_start_matches(' ');

        // optional tags section
        if s.starts_with('@') {
            // TODO: tags
            // if there is no space found, then the command part of the message is missing
            let space = s.find(' ').ok_or(IrcParseErr::MissingCommand)?;
            s = &s[space..];
        }

        // optional source section
        let source = if let Some((_, rest)) = s.split_prefix(':') {
            let Some((source, rest)) = rest.split_once(' ') else {
                // if there's not a space after the source, the command is missing
                return Err(IrcParseErr::MissingCommand);
            };

            s = rest;
            let source = Source::parse(source);
            debug!("parsed source: {:#?}", source);
            Some(source)
        } else {
            None
        };

        s = s.trim_start_matches(' ');
        if s.len() == 0 {
            return Err(IrcParseErr::MissingCommand);
        }

        Ok(IrcMessage {
            tags: None,
            source,
            message: Message::parse(s)?,
        })
    }

    // turns this message into a string that can be sent across the IRC connection directly. the
    // returned value includes the trailing CRLF that all messages must have.
    pub fn to_irc_string(&self) -> Result<String, MessageToStringErr> {
        let mut message = String::new();

        if let Some(_tags) = self.tags {
            todo!()
        }

        // clients must never send a source to the server
        if self.source.is_some() {
            return Err(MessageToStringErr::ClientMustNotSendSource);
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
    MessageParseErr(#[from] MessageParseErr),
}
