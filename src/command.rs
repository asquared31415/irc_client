use core::sync::atomic;
use std::sync::mpsc::Sender;

use eyre::{bail, eyre};
use log::*;
use thiserror::Error;

use crate::{
    channel::channel::Channel,
    irc_message::{IRCMessage, Message},
    state::{ClientState, ConnectedState, ConnectionState},
    util::Target,
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
    /// send raw text to the IRC server
    Raw(String),
    Quit,
}

#[derive(Debug, Error)]
pub enum CommandParseErr {
    #[error("command expected {} args, found {}", .0, .1)]
    IncorrectArgCount(u8, u8),
    #[error("unknown command {}", .0)]
    UnknownCommand(String),
}

impl Command {
    pub fn parse<S: AsRef<str>>(s: S) -> Result<Self, CommandParseErr> {
        let s = s.as_ref().to_lowercase();
        let (cmd, args_str) = s.split_once(' ').unwrap_or((s.as_str(), ""));
        let args = args_str
            .split(' ')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();

        match cmd {
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
            "quit" => Ok(Command::Quit),
            _ => Err(CommandParseErr::UnknownCommand(cmd.to_string())),
        }
    }

    pub fn handle(&self, state: &mut ClientState, sender: &Sender<IRCMessage>) -> eyre::Result<()> {
        match self {
            Command::Join(channel) => {
                let ConnectedState { .. } = expect_connected_state!(state, "JOIN")?;

                let target = Target::new(channel.to_string())
                    .ok_or_else(|| eyre!("join was invalid channel {:?}", channel))?;

                sender.send(IRCMessage {
                    tags: None,
                    source: None,
                    message: Message::Join(vec![(channel.to_string(), None)]),
                })?;

                state.join_channel(Channel::from_target(&target)?);
            }
            Command::Raw(text) => {
                // don't need to access the state here, just need to ensure connected
                let _ = expect_connected_state!(state, "RAW")?;

                sender.send(IRCMessage {
                    tags: None,
                    source: None,
                    message: Message::Raw(text.to_string()),
                })?;
            }
            Command::Quit => {
                sender.send(IRCMessage {
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
