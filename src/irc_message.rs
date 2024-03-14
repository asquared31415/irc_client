use core::{fmt::Display, str::FromStr};

use thiserror::Error;

#[derive(Debug)]
pub struct IRCMessage {
    tags: Option<()>,
    source: Option<()>,
    message: Message,
}

#[derive(Debug, Error)]
pub enum IrcParseErr {
    #[error("message contains interior CRLF")]
    InteriorCRLF,
    #[error("message is missing a command")]
    MissingCommand,
}

impl IRCMessage {
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
        if s.starts_with(':') {
            // TODO: source
            // if there is no space found, then the command part of the message is missing
            let space = s.find(' ').ok_or(IrcParseErr::MissingCommand)?;
            s = &s[space..];
        }

        s = s.trim_start_matches(' ');
        if s.len() == 0 {
            return Err(IrcParseErr::MissingCommand);
        }

        let message = match s.split_once(' ') {
            Some((message_kind, rest)) => {
                s = rest;
                Message {
                    kind: MessageKind::from_str(message_kind).unwrap(),
                    params: IRCMessage::parse_params(rest),
                }
            }
            None => {
                // there was no space after the text, this is all one command
                Message {
                    kind: MessageKind::from_str(s).unwrap(),
                    params: vec![],
                }
            }
        };

        Ok(IRCMessage {
            tags: None,
            source: None,
            message,
        })
    }

    // turns this message into a string that can be sent across the IRC connection directly. the
    // returned value includes the trailing CRLF that all messages must have.
    pub fn to_irc_string(&self) -> String {
        let mut message = String::new();

        if let Some(_tags) = self.tags {
            todo!()
        }

        if let Some(_source) = self.source {
            todo!()
        }

        message.push_str(self.message.kind.to_string().as_str());

        for param in self.message.params.iter() {
            message.push(' ');
            message.push_str(param.as_str());
        }

        assert!(!(message.contains('\r') || message.contains('\n')));
        message.push('\r');
        message.push('\n');
        message
    }

    fn parse_params(s: &str) -> Vec<String> {
        let mut params = vec![];

        let mut s = s.trim_start_matches(' ');
        // split off params by spaces
        while let Some((param, rest)) = s.split_once(' ') {
            // NOTE: if a parameter starts with a `:`, the rest of the message  is a parameter. `:`
            // is OPTIONAL if it's not necessary to disambiguate.
            if param.starts_with(':') {
                let mut param = param[1..].to_string();
                param.push_str(rest);
                params.push(param);
                // we have handled it all, exit
                return params;
            } else {
                // TODO: remove spaces
                params.push(param.to_string());
                // remove all spaces that are after this param
                s = rest.trim_start_matches(' ');
            }
        }

        // if the loop falls through there is only one param left, push it
        // cannot use a trim function, `::meow` as a final param means that the param is `:meow` but
        // trim would remove all counts of `:`
        if s.starts_with(':') {
            s = &s[1..];
        }
        s = s.trim_end_matches(' ');
        params.push(s.to_string());

        params
    }
}

#[derive(Debug)]
pub struct Message {
    kind: MessageKind,
    params: Vec<String>,
}

#[derive(Debug)]
pub enum MessageKind {
    Cap,
    Authenticate,
    Pass,
    Nick,
    Ping,
    Pong,
    Oper,
    Quit,
    // server to client only
    Error,
    Join,
    Part,
    Topic,

    Unknown(String),
}

impl Display for MessageKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            MessageKind::Cap => "CAP",
            MessageKind::Authenticate => "AUTHENTICATE",
            MessageKind::Pass => "PASS",
            MessageKind::Nick => "NICK",
            MessageKind::Ping => "PING",
            MessageKind::Pong => "PONG",
            MessageKind::Oper => "OPER",
            MessageKind::Quit => "QUIT",
            MessageKind::Error => "ERROR",
            MessageKind::Join => "JOIN",
            MessageKind::Part => "PART",
            MessageKind::Topic => "TOPIC",

            MessageKind::Unknown(s) => s.as_str(),
        };

        write!(f, "{}", name)
    }
}

impl FromStr for MessageKind {
    type Err = !;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "CAP" => Ok(MessageKind::Cap),
            "AUTHENTICATE" => Ok(MessageKind::Authenticate),
            "PASS" => Ok(MessageKind::Pass),
            "NICK" => Ok(MessageKind::Nick),
            "PING" => Ok(MessageKind::Ping),
            "PONG" => Ok(MessageKind::Pong),
            "OPER" => Ok(MessageKind::Oper),
            "QUIT" => Ok(MessageKind::Quit),
            "ERROR" => Ok(MessageKind::Error),
            "JOIN" => Ok(MessageKind::Join),
            "PART" => Ok(MessageKind::Part),
            "TOPIC" => Ok(MessageKind::Topic),

            s => Ok(MessageKind::Unknown(s.to_string())),
        }
    }
}
