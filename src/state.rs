use core::sync::atomic;
use std::{collections::HashMap, sync::mpsc::Sender};

use crossterm::style::Stylize as _;
use eyre::bail;
use log::*;

use crate::{
    channel::channel::Channel,
    client::QUIT_REQUESTED,
    handlers,
    irc_message::{IRCMessage, Message, Param, Source},
    ui::{
        term::{TerminalUi, UiMsg},
        text::Line,
    },
    util::Target,
};

pub struct ClientState<'a> {
    pub ui: TerminalUi<'a>,
    messages: Vec<IRCMessage>,
    target_messages: HashMap<Target, Vec<IRCMessage>>,
    pub conn_state: ConnectionState,
    sender: Sender<IRCMessage>,
    pub ui_sender: Sender<UiMsg<'a>>,
}

impl<'a> ClientState<'a> {
    pub fn new(
        sender: Sender<IRCMessage>,
        ui_sender: Sender<UiMsg<'a>>,
        ui: TerminalUi<'a>,
        requested_nick: String,
    ) -> Self {
        Self {
            ui,
            messages: Vec::new(),
            target_messages: HashMap::new(),
            conn_state: ConnectionState::Registration(RegistrationState { requested_nick }),
            sender,
            ui_sender,
        }
    }

    pub fn recv_msg(&mut self, msg: IRCMessage) -> eyre::Result<()> {
        let ui = &mut self.ui;
        let sender = &self.sender;

        match &msg.message {
            // =====================
            // PING
            // =====================
            Message::Ping(token) => sender.send(IRCMessage {
                tags: None,
                source: None,
                message: Message::Pong(token.to_string()),
            })?,

            // =====================
            // ERROR
            // =====================
            Message::Error(reason) => {
                ui.error(reason.as_str())?;
                self.target_messages
                    .entry(Target::Status)
                    .or_default()
                    .push(msg.clone());
                // technically not a requested quit, but a requested quit exits silently
                QUIT_REQUESTED.store(true, atomic::Ordering::Relaxed);
            }

            // =====================
            // REGISTRATION
            // =====================
            Message::Numeric { num: 1, args } => {
                let ClientState {
                    conn_state: ConnectionState::Registration(RegistrationState { requested_nick }),
                    ..
                } = self
                else {
                    self.ui_sender
                        .send(UiMsg::Warn(String::from("001 when already registered")));
                    return Ok(());
                };

                let [nick, text, ..] = args.as_slice() else {
                    bail!("RPL_001 had no nick and msg arg");
                };
                let (Some(nick), Some(text)) = (nick.as_str(), text.as_str()) else {
                    bail!("nick must be a string argument");
                };

                if requested_nick != nick {
                    self.ui_sender.send(UiMsg::Warn(format!(
                        "WARNING: requested nick {}, but got nick {}",
                        requested_nick, nick
                    )));
                }

                self.conn_state = ConnectionState::Connected(ConnectedState {
                    nick: nick.to_string(),
                    channels: Vec::new(),
                    messages_state: MessagesState {
                        active_names: HashMap::new(),
                    },
                });
                self.ui_sender
                    .send(UiMsg::Writeln(Line::from(text.to_string())));
                self.target_messages
                    .entry(Target::Status)
                    .or_default()
                    .push(msg.clone());
            }

            // =====================
            // GREETING
            // =====================
            Message::Numeric { num: 2, args } => {
                let [_, text, ..] = args.as_slice() else {
                    bail!("RPL_YOURHOST missing msg");
                };
                let Some(text) = text.as_str() else {
                    bail!("RPL_YOURHOST msg not a string");
                };
                self.ui_sender
                    .send(UiMsg::Writeln(Line::from(text.to_string())));
                self.target_messages
                    .entry(Target::Status)
                    .or_default()
                    .push(msg.clone());
            }
            Message::Numeric { num: 3, args } => {
                let [_, text, ..] = args.as_slice() else {
                    bail!("RPL_CREATED missing msg");
                };
                let Some(text) = text.as_str() else {
                    bail!("RPL_CREATED msg not a string");
                };
                self.ui_sender
                    .send(UiMsg::Writeln(Line::from(text.to_string())));
                self.target_messages
                    .entry(Target::Status)
                    .or_default()
                    .push(msg.clone());
            }
            Message::Numeric { num: 4, args } => {
                let [_, rest @ ..] = args.as_slice() else {
                    bail!("RPL_NUMERIC missing client arg");
                };
                let text = rest
                    .iter()
                    .filter_map(Param::as_str)
                    .collect::<Vec<&str>>()
                    .join(" ");
                self.ui_sender.send(UiMsg::Writeln(Line::from(text)));
                self.target_messages
                    .entry(Target::Status)
                    .or_default()
                    .push(msg.clone());
            }
            Message::Numeric { num: 5, args: _ } => {
                //TODO: do we care about this?
                self.target_messages
                    .entry(Target::Status)
                    .or_default()
                    .push(msg.clone());
            }

            // =====================
            // CHANNEL STATE
            // =====================
            Message::Join(join_channels) => {
                let ClientState {
                    conn_state: ConnectionState::Connected(ConnectedState { nick, channels, .. }),
                    ..
                } = self
                else {
                    bail!("JOIN messages can only be processed when connected to a server");
                };

                // if the source of the join is ourself, update the list of joined channels,
                // otherwise announce a join
                match msg.source.as_ref().map(|source| source.get_name()) {
                    Some(join_nick) => {
                        let join_channels = join_channels.iter().filter_map(|(channel, _)| {
                            if let Some(channel @ Target::Channel(_)) = Target::new(channel) {
                                Some(channel)
                            } else {
                                None
                            }
                        });
                        for channel in join_channels {
                            self.ui_sender.send(UiMsg::Writeln(
                                Line::default()
                                    .push(join_nick.magenta())
                                    .push(" joined ".green())
                                    .push(channel.as_str().dark_blue()),
                            ));

                            // if we were the ones joining the channel, track that
                            if join_nick == nick {
                                channels.push(Channel::new(channel.as_str())?);
                            }

                            self.target_messages
                                .entry(channel)
                                .or_default()
                                .push(msg.clone());
                        }
                    }
                    None => {
                        self.ui_sender
                            .send(UiMsg::Warn(String::from("JOIN msg without a source")));
                    }
                }

                debug!("{:#?}", channels);
            }

            Message::Quit(reason) => {
                let Some(name) = msg.source.as_ref().map(Source::get_name) else {
                    bail!("QUIT msg had no source");
                };
                // NOTE: servers SHOULD always send a reason, but make sure
                let reason = reason.as_deref().unwrap_or("disconnected");
                self.ui_sender.send(UiMsg::Writeln(
                    Line::default()
                        .push(name.magenta())
                        .push_unstyled(" quit: ")
                        .push_unstyled(reason),
                ));
                self.target_messages
                    .entry(Target::Status)
                    .or_default()
                    .push(msg.clone());
            }

            // =====================
            // modes
            // =====================
            Message::Mode { target, mode } => {
                let Some(mode) = mode else {
                    self.ui_sender.send(UiMsg::Warn(format!(
                        "server sent MODE for {} without modestr",
                        target
                    )));
                    return Ok(());
                };

                let ClientState {
                    conn_state: ConnectionState::Connected(ConnectedState { channels, .. }),
                    ..
                } = self
                else {
                    self.ui_sender.send(UiMsg::Warn(String::from(
                        "must be connected to handle MODE",
                    )));
                    return Ok(());
                };

                match target {
                    Target::Channel(channel_name) => {
                        let Some(channel) = channels.iter_mut().find(|c| c.name() == channel_name)
                        else {
                            self.ui_sender.send(UiMsg::Warn(format!(
                                "unexpected MODE for not joined channel {}",
                                channel_name
                            )));
                            return Ok(());
                        };

                        channel.modes = mode.to_string();

                        self.target_messages
                            .entry(Target::Channel(channel_name.clone()))
                            .or_default()
                            .push(msg.clone());
                    }
                    Target::Nickname(_) => {
                        self.ui_sender
                            .send(UiMsg::Warn(String::from("MODE for nicknames NYI")));
                    }
                    _ => {
                        self.ui_sender.send(UiMsg::Warn(String::from(
                            "could not determine target for MODE",
                        )));
                    }
                }

                debug!("{:#?}", channels);
            }

            // =====================
            // MESSAGES
            // =====================
            Message::Privmsg {
                targets,
                msg: privmsg,
            } => {
                // TODO: check whether target is channel vs user
                let mut line = if let Some(source) = msg.source.as_ref() {
                    create_nick_line(source.get_name(), false)
                } else {
                    Line::default()
                };
                line.extend(Line::default().push_unstyled(privmsg).into_iter());
                self.ui_sender.send(UiMsg::Writeln(line));

                for target in targets {
                    self.target_messages
                        .entry(target.clone())
                        .or_default()
                        .push(msg.clone());
                }
            }
            Message::Notice {
                targets,
                msg: notice_msg,
            } => {
                let mut line = if let Some(source) = msg.source.as_ref() {
                    create_nick_line(source.get_name(), false)
                } else {
                    Line::default()
                };
                line.extend(
                    Line::default()
                        .push("NOTICE ".green())
                        .push_unstyled(notice_msg)
                        .into_iter(),
                );
                self.ui_sender.send(UiMsg::Writeln(line));

                for target in targets {
                    self.target_messages
                        .entry(target.clone())
                        .or_default()
                        .push(msg.clone());
                }
            }

            // =====================
            // OTHER NUMERIC REPLIES
            // =====================
            msg @ Message::Numeric { .. } => {
                handlers::numeric::handle(msg, self)?;
            }

            // =====================
            // UNKNOWN
            // =====================
            unk => {
                self.ui_sender
                    .send(UiMsg::Warn(format!("unhandled msg {:?}", unk)));
            }
        }

        self.messages.push(msg);

        debug!("targets: {:#?}", self.target_messages);

        Ok(())
    }
}

fn create_nick_line(nick: &str, me: bool) -> Line<'static> {
    let nick = if me {
        nick.to_string().magenta().bold()
    } else {
        nick.to_string().magenta()
    };
    Line::default()
        .push_unstyled("<")
        .push(nick)
        .push_unstyled(">")
}

#[derive(Debug)]
pub enum ConnectionState {
    Registration(RegistrationState),
    Connected(ConnectedState),
}

#[derive(Debug)]
pub struct RegistrationState {
    /// the nick that the user requested. the server will respond with the actual nick in the
    /// RPL_WELCOME message.
    pub requested_nick: String,
}

#[derive(Debug)]
pub struct ConnectedState {
    pub nick: String,
    // currently connected channels
    pub channels: Vec<Channel>,
    pub messages_state: MessagesState,
}

/// state for messages that are in-flight or handled across multiple messages
#[derive(Debug)]
pub struct MessagesState {
    // a list of channels with active NAMES replies
    pub active_names: HashMap<String, NamesState>,
}

#[derive(Debug)]
pub struct NamesState {
    pub names: Vec<String>,
}
