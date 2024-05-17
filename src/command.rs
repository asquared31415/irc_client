use core::sync::atomic;
use std::sync::mpsc::Sender;

use eyre::eyre;
use log::*;
use thiserror::Error;

use crate::{
    channel::{ChannelName, Nickname},
    handlers::ctcp::CtcpCommand,
    irc_message::{IrcMessage, Message},
    state::{ClientState, ConnectedState, ConnectionState},
    targets::Target,
};

macro_rules! expect_connected_state {
    ($state:expr, $cmd:literal) => {
        match &mut $state.conn_state {
            ConnectionState::Connected(c) => Ok(c),
            _ => Err(eyre!("cannot handle command {} when not registered", $cmd)),
        }
    };
}

#[derive(Debug)]
pub enum Command {
    Join(String),
    Ctcp(Target, String),
    /// send raw text to the IRC server
    Raw(String),
    /// start a private message with the specified user
    Msg(Nickname),
    Quit,
}

#[derive(Debug, Error)]
pub enum CommandParseErr {
    #[error("command expected {} args, found {}", .0, .1)]
    IncorrectArgCount(u8, u8),
    #[error("invalid argument {}, expected {}", .0, .1)]
    InvalidArg(String, String),
    #[error("unknown command {}", .0)]
    UnknownCommand(String),
}

impl Command {
    pub fn parse<S: AsRef<str>>(s: S) -> Result<Self, CommandParseErr> {
        let s = s.as_ref();
        let (cmd, args_str) = s.split_once(' ').unwrap_or((s, ""));
        let args = args_str
            .split(' ')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();

        match cmd.to_lowercase().as_str() {
            "join" => {
                if args.len() != 1 {
                    return Err(CommandParseErr::IncorrectArgCount(1, args.len() as u8));
                }

                // note: channels.len() is at least one, split always returns at least one element
                let channels = args[0].split(',').collect::<Vec<_>>();
                if channels.len() > 1 {
                    // TODO
                }

                let channel = channels[0];
                if channel.is_empty() {
                    // NOTE: users are required to put the channel prefix, so an empty channel name
                    // is a bug
                    // TODO: err
                }

                Ok(Command::Join(channels[0].to_string()))
            }
            "raw" => {
                if args.len() == 0 {
                    return Err(CommandParseErr::IncorrectArgCount(1, 0));
                }

                Ok(Command::Raw(args_str.to_string()))
            }
            "msg" => {
                let &[nick] = args.as_slice() else {
                    return Err(CommandParseErr::IncorrectArgCount(1, args.len() as u8));
                };

                let Some(nick) = Nickname::new(nick) else {
                    return Err(CommandParseErr::InvalidArg(
                        nick.to_string(),
                        String::from("a nickname"),
                    ));
                };

                Ok(Command::Msg(nick))
            }
            "quit" => Ok(Command::Quit),
            _ => Err(CommandParseErr::UnknownCommand(cmd.to_string())),
        }
    }

    pub fn handle(&self, state: &mut ClientState, sender: &Sender<IrcMessage>) -> eyre::Result<()> {
        match self {
            Command::Join(channel) => {
                let ConnectedState { .. } = expect_connected_state!(state, "JOIN")?;

                let channel_name = ChannelName::new(channel)
                    .ok_or_else(|| eyre!("join was invalid channel {:?}", channel))?;

                sender.send(IrcMessage {
                    tags: None,
                    source: None,
                    message: Message::Join(vec![(channel.to_string(), None)]),
                })?;

                state.ensure_target_exists(Target::Channel(channel_name));
            }
            Command::Ctcp(target, command) => {
                todo!("handle CTCP command from user")
            }
            Command::Raw(text) => {
                // don't need to access the state here, just need to ensure connected
                let _ = expect_connected_state!(state, "RAW")?;

                sender.send(IrcMessage {
                    tags: None,
                    source: None,
                    message: Message::Raw(text.to_string()),
                })?;
            }
            Command::Msg(nick) => {
                let ConnectedState { .. } = expect_connected_state!(state, "PRIVMSG")?;
                debug!("nick: {:?}", nick);
                // just create the channel and switch to it, no message
                state.ensure_target_exists(Target::Nickname(nick.clone()));
                state.render()?;
            }
            Command::Quit => {
                sender.send(IrcMessage {
                    tags: None,
                    source: None,
                    message: Message::Quit(None),
                })?;
                crate::client::QUIT_REQUESTED.store(true, atomic::Ordering::Relaxed);
            }
        }

        Ok(())
    }
}
