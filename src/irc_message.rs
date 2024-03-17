use core::cmp;

use log::error;
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

#[derive(Debug, Error)]
pub enum MessageToStringErr {
    #[error("clients may not create a {} message", .0)]
    ClientMayNotCreate(String),
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
    pub fn to_irc_string(&self) -> Result<String, MessageToStringErr> {
        let mut message = String::new();

        if let Some(_tags) = self.tags {
            todo!()
        }

        if let Some(_source) = self.source {
            todo!()
        }

        message.push_str(self.message.to_string()?.as_str());
        message.push_str("\r\n");
        Ok(message)
    }
}

#[derive(Debug, Error)]
pub enum MessageParseErr {
    #[error("message {} missing params", .0)]
    MissingParams(String),
    #[error("message {} had invalid params", .0)]
    InvalidParams(String),
}

// FIXME: remove this once all variants can be constructed
#[allow(unused)]
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
    Oper,
    Quit(Option<String>),
    Error(String),

    // channel management
    Join(Vec<(String, Option<String>)>),
    Part(Vec<String>, Option<String>),
    Topic(String, Option<String>),
    Names(Vec<String>),
    List,
    Invite {
        nick: String,
        channel: String,
    },
    Kick {
        channel: String,
        user: String,
        comment: Option<String>,
    },

    // server queries and commands
    Motd {
        server: Option<String>,
    },
    Version {
        server: Option<String>,
    },
    Admin {
        server: Option<String>,
    },
    Connect {
        server: String,
        port: Option<u16>,
        // FIXME: remote server support
    },
    Lusers,
    Time {
        server: Option<String>,
    },
    Stats {
        query: char,
        server: Option<String>,
    },
    Help {
        subject: Option<String>,
    },
    Info,
    Mode {
        target: String,
        mode: Option<String>,
    },

    // messages
    Privmsg {
        targets: Vec<String>,
        msg: String,
    },
    Notice {
        targets: Vec<String>,
        msg: String,
    },

    // user queries
    Who {
        mask: String,
    },
    Whois {
        target: Option<String>,
        nick: String,
    },
    WhoWas {
        nick: String,
        count: Option<u16>,
    },

    // operator
    Kill {
        nick: String,
        comment: String,
    },
    Rehash,
    Restart,
    SQuit {
        server: String,
        comment: String,
    },

    // optional but suggested messages
    Away {
        message: Option<String>,
    },
    Links,
    // FIXME: ADD USERHOST, WALLOPS
    Numeric {
        num: u16,
        args: Vec<String>,
    },

    // an unknown message
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
            "CAP" => {
                todo!()
            }
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
                    .ok_or_else(|| MessageParseErr::MissingParams(s.to_string()))?;
                Ok(Message::Ping(token.to_string()))
            }
            "PONG" => {
                // clients must ignore the server param
                let [_server, token, ..] = args.as_slice() else {
                    return Err(MessageParseErr::MissingParams(s.to_string()));
                };
                Ok(Message::Pong(token.to_string()))
            }
            "OPER" => {
                todo!()
            }
            "QUIT" => {
                todo!()
            }
            "ERROR" => {
                todo!()
            }
            "JOIN" => {
                let (channels, keys) = match args.as_slice() {
                    [] => {
                        return Err(MessageParseErr::MissingParams(s.to_string()));
                    }
                    [channels] => (
                        channels
                            .split(',')
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>(),
                        vec![],
                    ),
                    [channels, keys, ..] => (
                        channels
                            .split(',')
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>(),
                        keys.split(',').map(|s| s.to_string()).collect::<Vec<_>>(),
                    ),
                };

                if keys.len() > channels.len() {
                    return Err(MessageParseErr::InvalidParams(s.to_string()));
                }

                let pairs = channels
                    .into_iter()
                    .enumerate()
                    .map(|(idx, val)| (val, keys.get(idx).cloned()))
                    .collect::<Vec<_>>();
                Ok(Message::Join(pairs))
            }
            "PART" => {
                todo!()
            }
            "TOPIC" => {
                todo!()
            }
            "NAMES" => {
                todo!()
            }
            "LIST" => {
                todo!()
            }
            "INVITE" => {
                todo!()
            }
            "KICK" => {
                todo!()
            }
            "MOTD" => {
                todo!()
            }
            "VERSION" => {
                todo!()
            }
            "ADMIN" => {
                todo!()
            }
            "CONNECT" => {
                todo!()
            }
            "LUSERS" => {
                todo!()
            }
            "TIME" => {
                todo!()
            }
            "STATS" => {
                todo!()
            }
            "HELP" => {
                todo!()
            }
            "INFO" => {
                todo!()
            }
            "MODE" => {
                todo!()
            }
            "PRIVMSG" => {
                let [targets, msg, ..] = args.as_slice() else {
                    return Err(MessageParseErr::MissingParams(s.to_string()));
                };
                Ok(Message::Privmsg {
                    targets: targets
                        .split(',')
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>(),
                    msg: msg.to_string(),
                })
            }
            "NOTICE" => {
                let [targets, msg, ..] = args.as_slice() else {
                    return Err(MessageParseErr::MissingParams(s.to_string()));
                };
                Ok(Message::Notice {
                    targets: targets
                        .split(',')
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>(),
                    msg: msg.to_string(),
                })
            }
            "WHO" => {
                todo!()
            }
            "WHOIS" => {
                todo!()
            }
            "WHOWAS" => {
                todo!()
            }
            "KILL" => {
                todo!()
            }
            "REHASH" => {
                todo!()
            }
            "RESTART" => {
                todo!()
            }
            "SQUIT" => {
                todo!()
            }
            "AWAY" => {
                todo!()
            }
            "LINKS" => {
                todo!()
            }
            // TODO: numerics
            other => match other.parse::<u16>() {
                // numerics may only be 3 digits
                Ok(num) if num <= 999 => Ok(Message::Numeric { num, args }),
                _ => Ok(Message::Unknown(other.to_string(), args)),
            },
        }
    }

    fn to_string(&self) -> Result<String, MessageToStringErr> {
        // FIXME: remove this!
        #[allow(unused)]
        //errors are returned early
        let msg = match self {
            Message::Cap => todo!("CAP"),
            Message::Authenticate => String::from("AUTHENTICATE"),
            Message::Pass(pass) => format!("PASS :{}", pass),
            Message::Nick(nick) => format!("NICK :{}", nick),
            Message::User(username, realname) => format!("USER {} 0 * :{}", username, realname),
            Message::Ping(token) => format!("PING :{}", token),
            Message::Pong(token) => format!("PONG :{}", token),

            Message::Oper => todo!("OPER"),
            Message::Quit(reason) => {
                let reason = match reason {
                    Some(r) => format!(":{}", r),
                    None => String::new(),
                };
                format!("QUIT{}", reason)
            }
            Message::Error(_) => {
                return Err(MessageToStringErr::ClientMayNotCreate(String::from(
                    "ERROR",
                )));
            }
            Message::Join(channels) => {
                if channels.len() == 0 {
                    error!("JOIN message had no channels");
                    todo!()
                }

                // sort channels such that all channels that have a key are first.
                // since keys are associated with channels based on their index, a gap in keys would
                // cause keys to be incorrectly associated.
                // FIXME: check if servers tend to accept an empty key?
                let mut channels = channels.clone();
                channels.sort_by(|(_, key1), (_, key2)| match (key1, key2) {
                    (None, None) => cmp::Ordering::Equal,
                    (None, Some(_)) => cmp::Ordering::Greater,
                    (Some(_), None) => cmp::Ordering::Less,
                    (Some(_), Some(_)) => cmp::Ordering::Equal,
                });

                let mut channels_str = String::new();
                let keys_str = String::new();

                for idx in 0..channels.len() - 1 {
                    let (channel, key) = &channels[idx];
                    channels_str.push_str(channel.as_str());
                    channels_str.push(',');
                    if let Some(_key) = key {
                        todo!("key formatting not yet supported");
                    }
                }
                // append final list element without a comma
                let (channel, key) = &channels[channels.len() - 1];
                channels_str.push_str(channel.as_str());
                if let Some(_key) = key {
                    todo!("key formatting not yet supported");
                }

                format!("JOIN {} {}", channels_str, keys_str)
            }
            Message::Part(_, _) => todo!(),
            Message::Topic(_, _) => todo!(),
            Message::Names(_) => todo!(),
            Message::List => todo!(),
            Message::Invite { nick, channel } => todo!(),
            Message::Kick {
                channel,
                user,
                comment,
            } => todo!(),
            Message::Motd { server } => todo!(),
            Message::Version { server } => todo!(),
            Message::Admin { server } => todo!(),
            Message::Connect { server, port } => todo!(),
            Message::Lusers => todo!(),
            Message::Time { server } => todo!(),
            Message::Stats { query, server } => todo!(),
            Message::Help { subject } => todo!(),
            Message::Info => todo!(),
            Message::Mode { target, mode } => todo!(),
            Message::Privmsg { targets, msg } => {
                format!("PRIVMSG {} :{}", targets.join(","), msg)
            }
            Message::Notice { targets, msg: text } => todo!(),
            Message::Who { mask } => todo!(),
            Message::Whois { target, nick } => todo!(),
            Message::WhoWas { nick, count } => todo!(),
            Message::Kill { nick, comment } => todo!(),
            Message::Rehash => todo!(),
            Message::Restart => todo!(),
            Message::SQuit { server, comment } => todo!(),
            Message::Away { message } => todo!(),
            Message::Links => todo!(),

            Message::Numeric { num, .. } => {
                return Err(MessageToStringErr::ClientMayNotCreate(num.to_string()));
            }

            Message::Unknown(name, params) => {
                let mut msg = name.to_string();
                for param in params.iter() {
                    msg.push(' ');
                    msg.push_str(param.as_str());
                }
                msg
            }
        };

        Ok(msg)
    }
}

// TODO: parse lists nicely too
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
