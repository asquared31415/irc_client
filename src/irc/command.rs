use core::{cmp, fmt::Write as _};

use thiserror::Error;

use crate::{
    irc::{param, Param},
    targets::Target,
};

// expects a parameter to be a string parameter, and extracts it, otherwise returns an invalid param
// err.
macro_rules! expect_string_param {
    ($expr:expr) => {{
        let param = $expr;
        match param.as_str() {
            Some(s) => s.to_string(),
            None => return Err(IrcCommandParseErr::InvalidParams),
        }
    }};
}

// FIXME: remove this once all variants can be constructed
#[allow(unused)]
#[derive(Debug, Clone)]
pub enum IrcCommand {
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
        target: Target,
        mode: Option<String>,
    },

    // messages
    Privmsg {
        targets: Vec<Target>,
        msg: String,
    },
    Notice {
        targets: Vec<Target>,
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
        args: Vec<Param>,
    },

    Raw(String),

    // an unknown message
    Unknown(String, Vec<Param>),
}

impl IrcCommand {
    /// parses a Message from a string. the string must not contain leading spaces and must not
    /// contain a CRLF.
    /// only parses messages that can be sent from a server to a client!
    pub(super) fn parse(s: &str) -> Result<Self, IrcCommandParseErr> {
        let (command, args) = match s.split_once(' ') {
            Some(parts) => parts,
            // there was no space after the text, this is all one command
            None => (s, ""),
        };
        let args = param::parse_params(args);

        match command {
            "CAP" => {
                todo!()
            }
            "AUTHENTICATE" => Ok(IrcCommand::Authenticate),
            "PASS" => {
                todo!()
            }
            "NICK" => {
                let [nick, ..] = args.as_slice() else {
                    return Err(IrcCommandParseErr::MissingParams(s.to_string()));
                };
                let nick = expect_string_param!(nick);
                Ok(IrcCommand::Nick(nick))
            }
            "USER" => {
                let [username, _, _, realname, ..] = args.as_slice() else {
                    return Err(IrcCommandParseErr::MissingParams(s.to_string()));
                };
                let username = expect_string_param!(username);
                let realname = expect_string_param!(realname);
                Ok(IrcCommand::User(username, realname))
            }
            "PING" => {
                let token = expect_string_param!(
                    args.first()
                        .ok_or_else(|| IrcCommandParseErr::MissingParams(s.to_string()))?
                );
                Ok(IrcCommand::Ping(token))
            }
            "PONG" => {
                // clients must ignore the server param
                let [_server, token, ..] = args.as_slice() else {
                    return Err(IrcCommandParseErr::MissingParams(s.to_string()));
                };
                let token = expect_string_param!(token);
                Ok(IrcCommand::Pong(token))
            }
            "OPER" => {
                todo!()
            }
            "QUIT" => {
                // reason is optional, can be a QUIT with no args
                let reason = match args.first() {
                    Some(p) => Some(expect_string_param!(p)),
                    None => None,
                };
                Ok(IrcCommand::Quit(reason))
            }
            "ERROR" => {
                let Some(reason) = args.first() else {
                    return Err(IrcCommandParseErr::MissingParams(s.to_string()));
                };
                let reason = expect_string_param!(reason);
                Ok(IrcCommand::Error(reason))
            }
            "JOIN" => {
                let (channels, keys) = match args.as_slice() {
                    [] => return Err(IrcCommandParseErr::MissingParams(s.to_string())),
                    [channels] => (channels.optional_list(), vec![]),
                    [channels, keys, ..] => (channels.optional_list(), keys.optional_list()),
                };

                if keys.len() > channels.len() {
                    return Err(IrcCommandParseErr::InvalidParams);
                }

                let pairs = channels
                    .into_iter()
                    .enumerate()
                    .map(|(idx, val)| (val, keys.get(idx).cloned()))
                    .collect::<Vec<_>>();
                Ok(IrcCommand::Join(pairs))
            }
            "PART" => {
                let [channels, rest @ ..] = args.as_slice() else {
                    return Err(IrcCommandParseErr::MissingParams(s.to_string()));
                };
                let channels = channels.optional_list();
                let reason = match rest.first() {
                    Some(param) => Some(expect_string_param!(param)),
                    None => None,
                };
                Ok(IrcCommand::Part(channels, reason))
            }
            "TOPIC" => {
                let [channel, rest @ ..] = args.as_slice() else {
                    return Err(IrcCommandParseErr::MissingParams(s.to_string()));
                };
                let channel = expect_string_param!(channel);
                let topic = match rest.first() {
                    Some(t) => Some(expect_string_param!(t)),
                    None => None,
                };
                Ok(IrcCommand::Topic(channel, topic))
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
                let [target, rest @ ..] = args.as_slice() else {
                    return Err(IrcCommandParseErr::MissingParams(s.to_string()));
                };
                let Some(target) = Target::new(expect_string_param!(target)) else {
                    return Err(IrcCommandParseErr::InvalidParams);
                };

                let mode = match rest {
                    [] => None,
                    [start @ .., last] => {
                        let mut mode = String::new();
                        for p in start {
                            mode.push_str(p.to_irc_string().as_str());
                            mode.push(' ');
                        }
                        mode.push_str(last.to_irc_string().as_str());
                        Some(mode)
                    }
                };

                Ok(IrcCommand::Mode { target, mode })
            }
            "PRIVMSG" => {
                let [targets, msg, ..] = args.as_slice() else {
                    return Err(IrcCommandParseErr::MissingParams(s.to_string()));
                };
                let targets = targets
                    .optional_list()
                    .into_iter()
                    .filter_map(Target::new)
                    .collect();
                let msg = expect_string_param!(msg);
                Ok(IrcCommand::Privmsg { targets, msg })
            }
            "NOTICE" => {
                let [targets, msg, ..] = args.as_slice() else {
                    return Err(IrcCommandParseErr::MissingParams(s.to_string()));
                };
                let targets = targets
                    .optional_list()
                    .into_iter()
                    .filter_map(Target::new)
                    .collect();
                let msg = expect_string_param!(msg);
                Ok(IrcCommand::Notice { targets, msg })
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
            other => match other.parse::<u16>() {
                // numerics may only be 3 digits
                Ok(num) if num <= 999 => Ok(IrcCommand::Numeric { num, args }),
                _ => Ok(IrcCommand::Unknown(other.to_string(), args)),
            },
        }
    }

    pub(super) fn to_irc_string(&self) -> Result<String, IrcCommandToStringErr> {
        // FIXME: remove this!
        #[allow(unused)]
        //errors are returned early
        let msg = match self {
            IrcCommand::Cap => todo!("CAP"),
            IrcCommand::Authenticate => String::from("AUTHENTICATE"),
            IrcCommand::Pass(pass) => format!("PASS :{}", pass),
            IrcCommand::Nick(nick) => format!("NICK :{}", nick),
            IrcCommand::User(username, realname) => format!("USER {} 0 * :{}", username, realname),
            IrcCommand::Ping(token) => format!("PING :{}", token),
            IrcCommand::Pong(token) => format!("PONG :{}", token),

            IrcCommand::Oper => todo!("OPER"),
            IrcCommand::Quit(reason) => {
                let reason = match reason {
                    Some(r) => format!(":{}", r),
                    None => String::new(),
                };
                format!("QUIT{}", reason)
            }
            IrcCommand::Error(_) => {
                return Err(IrcCommandToStringErr::ClientMayNotCreate(String::from(
                    "ERROR",
                )));
            }
            IrcCommand::Join(channels) => {
                if channels.len() == 0 {
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
            IrcCommand::Part(_, _) => todo!(),
            IrcCommand::Topic(_, _) => todo!(),
            IrcCommand::Names(_) => todo!(),
            IrcCommand::List => todo!(),
            IrcCommand::Invite { nick, channel } => todo!(),
            IrcCommand::Kick {
                channel,
                user,
                comment,
            } => todo!(),
            IrcCommand::Motd { server } => todo!(),
            IrcCommand::Version { server } => todo!(),
            IrcCommand::Admin { server } => todo!(),
            IrcCommand::Connect { server, port } => todo!(),
            IrcCommand::Lusers => todo!(),
            IrcCommand::Time { server } => todo!(),
            IrcCommand::Stats { query, server } => todo!(),
            IrcCommand::Help { subject } => todo!(),
            IrcCommand::Info => todo!(),
            IrcCommand::Mode { target, mode } => todo!(),
            IrcCommand::Privmsg { targets, msg } => {
                let mut target_str = String::new();
                match targets.as_slice() {
                    [] => {
                        return Err(IrcCommandToStringErr::InvalidParams);
                    }
                    [start @ .., last] => {
                        for target in start {
                            // UNWRAP: writing to a string is infallible
                            write!(&mut target_str, "{},", target.as_str()).unwrap();
                        }
                        target_str.push_str(last.as_str());
                    }
                }

                format!("PRIVMSG {} :{}", target_str, msg)
            }
            IrcCommand::Notice { targets, msg } => {
                let mut target_str = String::new();
                match targets.as_slice() {
                    [] => {
                        return Err(IrcCommandToStringErr::InvalidParams);
                    }
                    [start @ .., last] => {
                        for target in start {
                            // UNWRAP: writing to a string is infallible
                            write!(&mut target_str, "{},", target.as_str()).unwrap();
                        }
                        target_str.push_str(last.as_str());
                    }
                }

                format!("NOTICE {} :{}", target_str, msg)
            }
            IrcCommand::Who { mask } => todo!(),
            IrcCommand::Whois { target, nick } => todo!(),
            IrcCommand::WhoWas { nick, count } => todo!(),
            IrcCommand::Kill { nick, comment } => todo!(),
            IrcCommand::Rehash => todo!(),
            IrcCommand::Restart => todo!(),
            IrcCommand::SQuit { server, comment } => todo!(),
            IrcCommand::Away { message } => todo!(),
            IrcCommand::Links => todo!(),

            IrcCommand::Numeric { num, .. } => {
                return Err(IrcCommandToStringErr::ClientMayNotCreate(num.to_string()));
            }

            IrcCommand::Raw(text) => text.to_string(),

            IrcCommand::Unknown(name, params) => {
                let mut msg = name.to_string();
                for param in params.iter() {
                    msg.push(' ');
                    msg.push_str(param.to_irc_string().as_str());
                }
                msg
            }
        };

        Ok(msg)
    }
}

#[derive(Debug, Error)]
pub enum IrcCommandParseErr {
    #[error("message {} missing params", .0)]
    MissingParams(String),
    #[error("message had invalid params")]
    InvalidParams,
}

#[derive(Debug, Error)]
pub enum IrcCommandToStringErr {
    #[error("clients may not create a {} message", .0)]
    ClientMayNotCreate(String),
    #[error("clients must not send a source with their messages")]
    ClientMustNotSendSource,
    #[error("message had invalid params")]
    InvalidParams,
}
