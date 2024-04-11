use std::collections::HashMap;

use crossterm::style::Stylize as _;
use eyre::{bail, eyre};
use log::*;

use crate::{
    channel::channel::Channel,
    irc_message::{IRCMessage, Message, Param, Source},
    state::{
        ClientState, ConnectedState, ConnectionState, MessagesState, NamesState, RegistrationState,
    },
    ui::{term::UiMsg, text::Line},
    util::Target,
};

macro_rules! expect_connected_state {
    ($state:expr, $msg:expr) => {
        match &mut $state.conn_state {
            ConnectionState::Connected(c) => Ok(c),
            _ => Err(eyre!("cannot handle msg {:?} when not connected", $msg)),
        }
    };
}

impl IRCMessage {
    // FIXME: remove this
    #[allow(unused_must_use)]
    pub fn handle(&self, state: &mut ClientState) -> eyre::Result<()> {
        use crate::constants::numerics::*;
        match &self.message {
            Message::Cap => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(format!("unhandled msg {:?}", self)));
            }
            Message::Authenticate => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(format!("unhandled msg {:?}", self)));
            }
            Message::Nick(_) => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(format!("unhandled msg {:?}", self)));
            }
            Message::Ping(token) => {
                state.msg_sender.send(IRCMessage {
                    tags: None,
                    source: None,
                    message: Message::Pong(token.to_string()),
                })?;
            }
            Message::Quit(reason) => {
                let Some(name) = self.source.as_ref().map(Source::get_name) else {
                    bail!("QUIT msg had no source");
                };
                // NOTE: servers SHOULD always send a reason, but make sure
                let reason = reason.as_deref().unwrap_or("disconnected");
                state.ui_sender.send(UiMsg::Writeln(
                    Line::default()
                        .push(name.magenta())
                        .push_unstyled(" quit: ")
                        .push_unstyled(reason),
                ));
                state
                    .target_messages
                    .entry(Target::Status)
                    .or_default()
                    .push(self.clone());
            }
            Message::Join(join_channels) => {
                let ClientState {
                    conn_state: ConnectionState::Connected(ConnectedState { nick, channels, .. }),
                    ..
                } = state
                else {
                    bail!("JOIN messages can only be processed when connected to a server");
                };

                // if the source of the join is ourself, update the list of joined channels,
                // otherwise announce a join
                match self.source.as_ref().map(|source| source.get_name()) {
                    Some(join_nick) => {
                        let join_channels = join_channels.iter().filter_map(|(channel, _)| {
                            if let Some(channel @ Target::Channel(_)) = Target::new(channel) {
                                Some(channel)
                            } else {
                                None
                            }
                        });
                        for channel in join_channels {
                            state.ui_sender.send(UiMsg::Writeln(
                                Line::default()
                                    .push(join_nick.magenta())
                                    .push(" joined ".green())
                                    .push(channel.as_str().dark_blue()),
                            ));

                            // if we were the ones joining the channel, track that
                            if join_nick == nick {
                                channels.push(Channel::new(channel.as_str())?);
                            }

                            state
                                .target_messages
                                .entry(channel)
                                .or_default()
                                .push(self.clone());
                        }
                    }
                    None => {
                        state
                            .ui_sender
                            .send(UiMsg::Warn(String::from("JOIN msg without a source")));
                    }
                }

                debug!("{:#?}", channels);
            }
            Message::Part(channels, reason) => {
                let Some(name) = self.source.as_ref().map(Source::get_name) else {
                    bail!("QUIT msg had no source");
                };

                let channels =
                    channels
                        .iter()
                        .filter_map(|channel_name| match Target::new(channel_name) {
                            t @ Some(Target::Channel(_)) => t,
                            _ => None,
                        });

                for channel in channels {
                    let mut line = Line::default().push(name.magenta()).push_unstyled(" quit");
                    // reasons are entirely optional
                    if let Some(reason) = reason {
                        line = line.push_unstyled(format!(": {}", reason));
                    }

                    state.ui_sender.send(UiMsg::Writeln(line));
                    state
                        .target_messages
                        .entry(channel)
                        .or_default()
                        .push(self.clone());
                }
            }
            Message::Invite { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(format!("unhandled msg {:?}", self)));
            }
            Message::Kick { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(format!("unhandled msg {:?}", self)));
            }
            Message::Mode { target, mode } => {
                let Some(mode) = mode else {
                    state.ui_sender.send(UiMsg::Warn(format!(
                        "server sent MODE for {} without modestr",
                        target
                    )));
                    return Ok(());
                };

                let ClientState {
                    conn_state: ConnectionState::Connected(ConnectedState { channels, .. }),
                    ..
                } = state
                else {
                    state.ui_sender.send(UiMsg::Warn(String::from(
                        "must be connected to handle MODE",
                    )));
                    return Ok(());
                };

                match target {
                    Target::Channel(channel_name) => {
                        let Some(channel) = channels.iter_mut().find(|c| c.name() == channel_name)
                        else {
                            state.ui_sender.send(UiMsg::Warn(format!(
                                "unexpected MODE for not joined channel {}",
                                channel_name
                            )));
                            return Ok(());
                        };

                        channel.modes = mode.to_string();

                        state
                            .target_messages
                            .entry(Target::Channel(channel_name.clone()))
                            .or_default()
                            .push(self.clone());
                    }
                    Target::Nickname(_) => {
                        state
                            .ui_sender
                            .send(UiMsg::Warn(String::from("MODE for nicknames NYI")));
                    }
                    _ => {
                        state.ui_sender.send(UiMsg::Warn(String::from(
                            "could not determine target for MODE",
                        )));
                    }
                }
            }
            Message::Privmsg { targets, msg } => {
                let mut line = if let Some(source) = self.source.as_ref() {
                    create_nick_line(source.get_name(), false)
                } else {
                    Line::default()
                };
                line.extend(Line::default().push_unstyled(msg).into_iter());
                state.ui_sender.send(UiMsg::Writeln(line));

                for target in targets {
                    state
                        .target_messages
                        .entry(target.clone())
                        .or_default()
                        .push(self.clone());
                }
            }
            Message::Notice { targets, msg } => {
                let mut line = if let Some(source) = self.source.as_ref() {
                    create_nick_line(source.get_name(), false)
                } else {
                    Line::default()
                };
                line.extend(
                    Line::default()
                        .push("NOTICE ".green())
                        .push_unstyled(msg)
                        .into_iter(),
                );
                state.ui_sender.send(UiMsg::Writeln(line));

                for target in targets {
                    state
                        .target_messages
                        .entry(target.clone())
                        .or_default()
                        .push(self.clone());
                }
            }
            Message::Kill { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(format!("unhandled msg {:?}", self)));
            }

            // =========================================
            // =========================================
            // =========================================
            // =========================================
            // numerics
            // =========================================
            // =========================================
            // =========================================
            // =========================================
            Message::Numeric {
                num: RPL_WELCOME,
                args,
            } => {
                let ClientState {
                    conn_state: ConnectionState::Registration(RegistrationState { requested_nick }),
                    ..
                } = state
                else {
                    state
                        .ui_sender
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
                    state.ui_sender.send(UiMsg::Warn(format!(
                        "WARNING: requested nick {}, but got nick {}",
                        requested_nick, nick
                    )));
                }

                state.conn_state = ConnectionState::Connected(ConnectedState {
                    nick: nick.to_string(),
                    channels: Vec::new(),
                    messages_state: MessagesState {
                        active_names: HashMap::new(),
                    },
                });
                state
                    .ui_sender
                    .send(UiMsg::Writeln(Line::from(text.to_string())));
                state
                    .target_messages
                    .entry(Target::Status)
                    .or_default()
                    .push(self.clone());
            }

            Message::Numeric {
                num: RPL_YOURHOST,
                args,
            } => {
                let [_, text, ..] = args.as_slice() else {
                    bail!("RPL_YOURHOST missing msg");
                };
                let Some(text) = text.as_str() else {
                    bail!("RPL_YOURHOST msg not a string");
                };
                state
                    .ui_sender
                    .send(UiMsg::Writeln(Line::from(text.to_string())));
                state
                    .target_messages
                    .entry(Target::Status)
                    .or_default()
                    .push(self.clone());
            }
            Message::Numeric {
                num: RPL_CREATED,
                args,
            } => {
                let [_, text, ..] = args.as_slice() else {
                    bail!("RPL_CREATED missing msg");
                };
                let Some(text) = text.as_str() else {
                    bail!("RPL_CREATED msg not a string");
                };
                state
                    .ui_sender
                    .send(UiMsg::Writeln(Line::from(text.to_string())));
                state
                    .target_messages
                    .entry(Target::Status)
                    .or_default()
                    .push(self.clone());
            }
            Message::Numeric {
                num: RPL_MYINFO,
                args,
            } => {
                let [_, rest @ ..] = args.as_slice() else {
                    bail!("RPL_MYINFO missing client arg");
                };
                let text = rest
                    .iter()
                    .filter_map(Param::as_str)
                    .collect::<Vec<&str>>()
                    .join(" ");
                state.ui_sender.send(UiMsg::Writeln(Line::from(text)));
                state
                    .target_messages
                    .entry(Target::Status)
                    .or_default()
                    .push(self.clone());
            }
            Message::Numeric {
                num: RPL_ISUPPORT,
                args: _,
            } => {
                //TODO: do we care about this?
                state
                    .target_messages
                    .entry(Target::Status)
                    .or_default()
                    .push(self.clone());
            }

            Message::Numeric {
                num: RPL_LUSERCLIENT,
                args,
            } => {
                if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                    state
                        .ui_sender
                        .send(UiMsg::Writeln(Line::from(msg.to_string())));
                }
            }
            Message::Numeric {
                num: RPL_LUSEROP,
                args,
            } => {
                let [_, ops, msg, ..] = args.as_slice() else {
                    // just ignore malformed replies here
                    return Ok(());
                };
                let Some(ops): Option<u16> = ops.as_str().and_then(|s| s.parse().ok()) else {
                    state
                        .ui_sender
                        .send(UiMsg::Warn(String::from("RPL_USEROP was not a u16")));
                    return Ok(());
                };
                let Some(msg) = msg.as_str() else {
                    return Ok(());
                };

                state
                    .ui_sender
                    .send(UiMsg::Writeln(Line::from(format!("{} {}", ops, msg))));
            }
            Message::Numeric {
                num: RPL_LUSERUNKNOWN,
                args,
            } => {
                let [_, ops, msg, ..] = args.as_slice() else {
                    // just ignore malformed replies here
                    return Ok(());
                };
                let Some(connections): Option<u16> = ops.as_str().and_then(|s| s.parse().ok())
                else {
                    state
                        .ui_sender
                        .send(UiMsg::Warn(String::from("RPL_LUSERUNKNOWN was not a u16")));
                    return Ok(());
                };
                let Some(msg) = msg.as_str() else {
                    return Ok(());
                };

                state.ui_sender.send(UiMsg::Writeln(Line::from(format!(
                    "{} {}",
                    connections, msg
                ))));
            }
            Message::Numeric {
                num: RPL_LUSERCHANNELS,
                args,
            } => {
                let [_, ops, msg, ..] = args.as_slice() else {
                    // just ignore malformed replies here
                    return Ok(());
                };
                let Some(channels): Option<u16> = ops.as_str().and_then(|s| s.parse().ok()) else {
                    state
                        .ui_sender
                        .send(UiMsg::Warn(String::from("RPL_LUSERCHANNELS was not a u16")));
                    return Ok(());
                };
                let Some(msg) = msg.as_str() else {
                    return Ok(());
                };

                state
                    .ui_sender
                    .send(UiMsg::Writeln(Line::from(format!("{} {}", channels, msg))));
            }
            Message::Numeric {
                num: RPL_LUSERME,
                args,
            } => {
                if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                    state
                        .ui_sender
                        .send(UiMsg::Writeln(Line::from(msg.to_string())));
                }
            }
            Message::Numeric {
                num: RPL_LOCALUSERS,
                args,
            } => {
                if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                    state
                        .ui_sender
                        .send(UiMsg::Writeln(Line::from(msg.to_string())));
                }
            }
            Message::Numeric {
                num: RPL_GLOBALUSERS,
                args,
            } => {
                if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                    state
                        .ui_sender
                        .send(UiMsg::Writeln(Line::from(msg.to_string())));
                }
            }

            // =======================
            // MOTD
            // =======================
            Message::Numeric {
                num: ERR_NOMOTD,
                args,
            } => {
                state.ui_sender.send(UiMsg::Writeln(
                    Line::default().push(
                        format!(
                            "no MOTD: {}",
                            args.get(2).and_then(|p| p.as_str()).unwrap_or("<MISSING>")
                        )
                        .yellow(),
                    ),
                ));
            }

            Message::Numeric {
                num: RPL_MOTDSTART | RPL_MOTD | RPL_ENDOFMOTD,
                args,
            } => {
                // display the MOTD to the user
                if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                    state
                        .ui_sender
                        .send(UiMsg::Writeln(Line::from(msg.to_string())));
                }
            }

            // =======================
            // names
            // =======================
            Message::Numeric {
                num: RPL_NAMREPLY,
                args,
            } => {
                let ConnectedState { messages_state, .. } = expect_connected_state!(state, self)?;

                let [_, _, channel, names_list @ ..] = args.as_slice() else {
                    state
                        .ui_sender
                        .send(UiMsg::Warn(String::from("RPL_NAMREPLY missing params")));
                    return Ok(());
                };
                let Some(channel) = channel.as_str() else {
                    state
                        .ui_sender
                        .send(UiMsg::Warn(String::from("RPL_NAMEREPLY malformed args")));
                    return Ok(());
                };

                let NamesState { names } = messages_state
                    .active_names
                    .entry(channel.to_string())
                    .or_insert_with(|| NamesState { names: Vec::new() });
                names.extend(
                    names_list
                        .into_iter()
                        .filter_map(|p| p.as_str().map(str::to_string)),
                );
            }
            Message::Numeric {
                num: RPL_ENDOFNAMES,
                args,
            } => {
                let ConnectedState {
                    messages_state,
                    channels,
                    ..
                } = expect_connected_state!(state, self)?;
                let [_, channel, ..] = args.as_slice() else {
                    state
                        .ui_sender
                        .send(UiMsg::Warn(String::from("RPL_ENDOFNAMES missing args")));
                    return Ok(());
                };
                let Some(channel_name) = channel.as_str() else {
                    state
                        .ui_sender
                        .send(UiMsg::Warn(String::from("RPL_ENDOFNAMES malformed args")));
                    return Ok(());
                };

                let Some(NamesState { names }) = messages_state.active_names.remove(channel_name)
                else {
                    state.ui_sender.send(UiMsg::Warn(format!(
                        "unexpected RPL_NAMEREPLY for {}",
                        channel_name
                    )));
                    return Ok(());
                };

                let Some(channel) = channels.iter_mut().find(|c| c.name() == channel_name) else {
                    state.ui_sender.send(UiMsg::Warn(format!(
                        "cannot update names for channel not joined: {}",
                        channel_name
                    )));
                    return Ok(());
                };

                channel.users.extend(names.iter().cloned());
                debug!("{:#?}", channel);

                state.ui_sender.send(UiMsg::Writeln(
                    Line::default()
                        .push("NAMES".green())
                        .push_unstyled(" for ")
                        .push(channel_name.blue()),
                ));
                state.ui_sender.send(UiMsg::Writeln(Line::from(format!(
                    " - {}",
                    names.join(" ")
                ))));
            }

            // =======================
            // modes
            // =======================
            Message::Numeric {
                num: RPL_UMODEIS, ..
            } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("TODO: RPL_UMODEIS")));
            }

            // =========================================
            // =========================================
            // =========================================
            // =========================================
            // misc unhandled
            // =========================================
            // =========================================
            // =========================================
            // =========================================
            Message::Numeric { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(format!("unhandled msg {:?}", self)));
            }

            Message::Unknown { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(format!("unhandled unknown msg {:?}", self)));
            }

            // fatal error, the connection will be terminated
            Message::Error(err) => {
                state.ui.error(err)?;
            }

            // ==============================================
            // ==============================================
            // ==============================================
            // ==============================================
            // servers should never send these messages!
            // ==============================================
            // ==============================================
            // ==============================================
            // ==============================================
            Message::Pass { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received PASS")));
            }
            Message::User { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received USER")));
            }
            Message::Pong { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received PONG")));
            }
            Message::Oper => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received OPER")));
            }
            Message::Topic { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received TOPIC")));
            }
            Message::Names { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received NAMES")));
            }
            Message::List => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received LIST")));
            }
            Message::Motd { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received MOTD")));
            }
            Message::Version { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received VERSION")));
            }

            Message::Admin { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received ADMIN")));
            }
            Message::Connect { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received CONNECT")));
            }
            Message::Lusers => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received LUSERS")));
            }
            Message::Time { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received TIME")));
            }
            Message::Stats { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received STATS")));
            }
            Message::Help { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received HELP")));
            }
            Message::Info => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received INFO")));
            }
            Message::Who { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received WHO")));
            }
            Message::Whois { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received WHOIS")));
            }
            Message::WhoWas { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received WHOWAS")));
            }
            Message::Rehash => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received REHASH")));
            }
            Message::Restart => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received RESTART")));
            }
            Message::SQuit { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received SQUIT")));
            }
            Message::Away { .. } => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received AWAY")));
            }
            Message::Links => {
                state
                    .ui_sender
                    .send(UiMsg::Warn(String::from("client received LINKS")));
            }
            Message::Raw { .. } => {
                state.ui_sender.send(UiMsg::Warn(String::from(
                    "client received RAW (????? this should genuinely be unreachable?!)",
                )));
            }
        }

        state.messages.push(self.clone());
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
