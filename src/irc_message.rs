use core::fmt::Display;

use thiserror::Error;

#[derive(Debug)]
pub struct IRCMessage {
    pub tags: Option<()>,
    pub source: Option<()>,
    pub message: Message,
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

        Ok(IRCMessage {
            tags: None,
            source: None,
            message: Message::parse(s)?,
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

        message.push_str(self.message.to_string().as_str());
        message.push_str("\r\n");
        message
    }
}

#[derive(Debug, Error)]
pub enum MessageParseErr {
    #[error("message {} missing params", .0)]
    MissingParams(String),
}

#[derive(Debug)]
pub enum Message {
    Cap, // FIXME: missing args
    Authenticate,
    Pass(String),
    Nick(String),
    User(String, String),
    Ping(String),
    // NOTE: server -> client PONG has a server param that must be ignored by the client.
    // clients must not send a server param to the server.
    Pong(String),

    Unknown(String, Vec<String>),
}

impl Message {
    /// parses a Message from a string. the string must not contain leading spaces and must not
    /// contain a CRLF.
    /// only parses messages that can be sent from a server to a client!
    fn parse(s: &str) -> Result<Self, MessageParseErr> {
        let (command, args) = match s.split_once(' ') {
            Some(parts) => parts,
            // there was no space after the text, this is all one command
            None => (s, ""),
        };
        let args = parse_params(args);

        match command {
            "AUTHENTICATE" => Ok(Message::Authenticate),
            "PASS" => {
                todo!()
            }
            "NICK" => {
                todo!()
            }
            "USER" => {
                todo!()
            }
            "PING" => {
                let token = args
                    .first()
                    .ok_or_else(|| MessageParseErr::MissingParams(command.to_string()))?;
                Ok(Message::Ping(token.to_string()))
            }
            "PONG" => {
                // clients must ignore the server param
                let [_server, token, ..] = args.as_slice() else {
                    return Err(MessageParseErr::MissingParams(command.to_string()));
                };
                Ok(Message::Pong(token.to_string()))
            }
            _ => Ok(Message::Unknown(command.to_string(), args)),
        }
    }
}

impl Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Message::Cap => todo!(),
            Message::Authenticate => todo!(),
            Message::Pass(_) => todo!(),
            Message::Nick(_) => todo!(),
            Message::User(_, _) => todo!(),
            Message::Ping(_) => todo!(),
            Message::Pong(_) => todo!(),
            Message::Unknown(name, params) => {
                write!(f, "{}", name)?;
                for param in params.iter() {
                    write!(f, " {}", param)?;
                }
                Ok(())
            }
        }
    }
}

fn parse_params(s: &str) -> Vec<String> {
    let mut params = vec![];

    let mut s = s.trim_start_matches(' ');
    // split off params by spaces
    while let Some((param, rest)) = s.split_once(' ') {
        // NOTE: if a parameter starts with a `:`, the rest of the message  is a parameter. `:`
        // is OPTIONAL if it's not necessary to disambiguate.
        if param.starts_with(':') {
            params.push(s[1..].to_string());
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
