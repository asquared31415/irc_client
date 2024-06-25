use core::{cmp, fmt::Write as _};

use crate::{
    channel::{ChannelName, Nickname},
    irc::{client::message::ClientMessageToStringErr, target::Target},
};

#[derive(Debug)]
pub enum ClientIrcCommand {
    Cap, // FIXME: args
    Authenticate,
    Pass(String),
    Nick(String),
    User(String, String),
    Ping(String),
    Pong(String),
    Oper, // FIXME: args
    Quit(Option<String>),
    Join(Vec<(ChannelName, Option<String>)>),
    Part(Vec<ChannelName>, Option<String>),
    Topic(ChannelName, Option<String>),
    Names(Vec<ChannelName>),
    List, // FIXME: weird optional args
    Invite {
        nick: Nickname,
        channel: ChannelName,
    },
    Kick {
        channel: ChannelName,
        users: Vec<Nickname>,
        comment: Option<String>,
    },
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
    Who {
        mask: String,
    },
    Whois {
        target: Option<String>,
        nick: Nickname,
    },
    WhoWas {
        nick: Nickname,
        count: Option<u16>,
    },
    Kill {
        nick: Nickname,
        comment: String,
    },
    Rehash,
    Restart,
    SQuit {
        server: String,
        comment: String,
    },
    Away {
        message: Option<String>,
    },
    Links,

    /// the client wants to send the following text directly to the server. this is typically used
    /// because there does not yet exist a nice interface for the IRC command in question.
    Raw(String),
}

impl ClientIrcCommand {
    pub fn irc_str(&self) -> Result<String, ClientMessageToStringErr> {
        // FIXME: remove this!
        #[allow(unused)]
        //errors are returned early
        let msg = match self {
            ClientIrcCommand::Cap => todo!("CAP"),
            ClientIrcCommand::Authenticate => String::from("AUTHENTICATE"),
            ClientIrcCommand::Pass(pass) => format!("PASS :{}", pass),
            ClientIrcCommand::Nick(nick) => format!("NICK :{}", nick),
            ClientIrcCommand::User(username, realname) => {
                format!("USER {} 0 * :{}", username, realname)
            }
            ClientIrcCommand::Ping(token) => format!("PING :{}", token),
            ClientIrcCommand::Pong(token) => format!("PONG :{}", token),

            ClientIrcCommand::Oper => todo!("OPER"),
            ClientIrcCommand::Quit(reason) => {
                let reason = match reason {
                    Some(r) => format!(":{}", r),
                    None => String::new(),
                };
                format!("QUIT{}", reason)
            }
            ClientIrcCommand::Join(channels) => {
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
            ClientIrcCommand::Part(_, _) => todo!(),
            ClientIrcCommand::Topic(_, _) => todo!(),
            ClientIrcCommand::Names(_) => todo!(),
            ClientIrcCommand::List => todo!(),
            ClientIrcCommand::Invite { nick, channel } => todo!(),
            ClientIrcCommand::Kick {
                channel,
                users,
                comment,
            } => todo!(),
            ClientIrcCommand::Motd { server } => todo!(),
            ClientIrcCommand::Version { server } => todo!(),
            ClientIrcCommand::Admin { server } => todo!(),
            ClientIrcCommand::Connect { server, port } => todo!(),
            ClientIrcCommand::Lusers => todo!(),
            ClientIrcCommand::Time { server } => todo!(),
            ClientIrcCommand::Stats { query, server } => todo!(),
            ClientIrcCommand::Help { subject } => todo!(),
            ClientIrcCommand::Info => todo!(),
            ClientIrcCommand::Mode { target, mode } => todo!(),
            ClientIrcCommand::Privmsg { targets, msg } => {
                let mut target_str = String::new();
                match targets.as_slice() {
                    [] => {
                        return Err(ClientMessageToStringErr::InvalidParams);
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
            ClientIrcCommand::Notice { targets, msg } => {
                let mut target_str = String::new();
                match targets.as_slice() {
                    [] => {
                        return Err(ClientMessageToStringErr::InvalidParams);
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
            ClientIrcCommand::Who { mask } => todo!(),
            ClientIrcCommand::Whois { target, nick } => todo!(),
            ClientIrcCommand::WhoWas { nick, count } => todo!(),
            ClientIrcCommand::Kill { nick, comment } => todo!(),
            ClientIrcCommand::Rehash => todo!(),
            ClientIrcCommand::Restart => todo!(),
            ClientIrcCommand::SQuit { server, comment } => todo!(),
            ClientIrcCommand::Away { message } => todo!(),
            ClientIrcCommand::Links => todo!(),

            ClientIrcCommand::Raw(text) => text.to_string(),
        };

        Ok(msg)
    }
}
