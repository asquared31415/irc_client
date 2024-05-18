use crossterm::style::Stylize as _;
use eyre::{bail, eyre};

use crate::{
    channel::{Channel, ChannelName},
    irc::{IrcCommand, IrcMessage, Param, Source},
    state::{ClientState, ConnectedState, ConnectionState, NamesState, RegistrationState},
    targets::Target,
    ui::text::Line,
    util,
};

pub mod ctcp;
mod msg;

macro_rules! expect_connected_state {
    ($state:expr, $msg:expr) => {
        match &mut $state.conn_state {
            ConnectionState::Connected(c) => Ok(c),
            _ => Err(eyre!("cannot handle msg {:?} when not connected", $msg)),
        }
    };
}

impl IrcMessage {
    fn unhandled(&self, state: &mut ClientState) {
        state.warn(format!("unhandled msg {:?}", self));
    }

    // FIXME: remove this
    #[allow(unused_must_use)]
    pub fn handle(&self, state: &mut ClientState) -> eyre::Result<()> {
        use crate::constants::numerics::*;
        match &self.message {
            IrcCommand::Cap => {
                self.unhandled(state);
            }
            IrcCommand::Authenticate => {
                self.unhandled(state);
            }
            IrcCommand::Nick(_) => {
                self.unhandled(state);
            }
            IrcCommand::Ping(token) => {
                state
                    .msg_sender
                    .send(IrcMessage::from_command(IrcCommand::Pong(
                        token.to_string(),
                    )))?;
            }
            IrcCommand::Quit(reason) => {
                let Some(name) = self.source.as_ref().map(Source::get_name) else {
                    bail!("QUIT msg had no source");
                };
                // NOTE: servers SHOULD always send a reason, but make sure
                let reason = reason.as_deref().unwrap_or("disconnected");
                state.add_line(
                    Target::Status,
                    util::line_now()
                        .push(name.magenta())
                        .push_unstyled(" quit: ")
                        .push_unstyled(reason),
                );
            }
            IrcCommand::Join(join_channels) => {
                let ClientState {
                    conn_state: ConnectionState::Connected(ConnectedState { .. }),
                    ..
                } = state
                else {
                    bail!("JOIN messages can only be processed when connected to a server");
                };

                // if the source of the join is ourself, update the list of joined channels,
                // otherwise announce a join
                match self.source.as_ref().map(|source| source.get_name()) {
                    Some(join_nick) => {
                        let join_channels = join_channels
                            .iter()
                            .filter_map(|(channel, _)| Channel::new(channel).ok())
                            .collect::<Vec<_>>();

                        for channel in join_channels.into_iter() {
                            let line = util::line_now()
                                .push(join_nick.magenta())
                                .push(" joined ".green())
                                .push(channel.name().as_str().dark_blue());

                            state.add_line(Target::Channel(channel.name().clone()), line);
                        }
                    }
                    None => {
                        state.warn_in(&Target::Status, String::from("JOIN msg without a source"));
                    }
                }
            }
            IrcCommand::Part(channels, reason) => {
                let Some(name) = self.source.as_ref().map(Source::get_name) else {
                    bail!("PART msg had no source");
                };

                let channels =
                    channels
                        .iter()
                        .filter_map(|channel_name| match Target::new(channel_name) {
                            t @ Some(Target::Channel(_)) => t,
                            _ => None,
                        });

                for channel in channels {
                    let mut line = util::line_now().push(name.magenta()).push_unstyled(" left");
                    // reasons are entirely optional
                    if let Some(reason) = reason {
                        line = line.push_unstyled(format!(": {}", reason));
                    }

                    state.add_line(channel, line);
                }
            }
            IrcCommand::Invite { .. } => {
                self.unhandled(state);
            }
            IrcCommand::Kick { .. } => {
                self.unhandled(state);
            }
            IrcCommand::Mode { target, mode } => {
                let Some(mode) = mode else {
                    state.warn_in(
                        &Target::Status,
                        format!("server sent MODE for {} without modestr", target.as_str()),
                    );
                    return Ok(());
                };

                let ClientState {
                    conn_state: ConnectionState::Connected(ConnectedState { channels, .. }),
                    ..
                } = state
                else {
                    state.warn_in(
                        &Target::Status,
                        String::from("must be connected to handle MODE"),
                    );
                    return Ok(());
                };

                match target {
                    Target::Channel(channel_name) => {
                        let Some(channel) = channels.get_mut(channel_name) else {
                            state.warn_in(
                                &Target::Status,
                                format!(
                                    "unexpected MODE for not joined channel {:?}",
                                    channel_name
                                ),
                            );
                            return Ok(());
                        };

                        channel.modes = mode.to_string();

                        // TODO: mode messages
                    }
                    Target::Nickname(_) => {
                        state.warn_in(&Target::Status, String::from("MODE for nicknames NYI"));
                    }
                    _ => {
                        state.warn_in(
                            &Target::Status,
                            String::from("could not determine target for MODE"),
                        );
                    }
                }
            }
            IrcCommand::Privmsg { targets, msg } => {
                for target in targets {
                    msg::handle_message(
                        state,
                        msg::MessageKind::Privmsg,
                        &self.source,
                        &target,
                        msg.as_str(),
                    );
                }
            }
            IrcCommand::Notice { targets, msg } => {
                for target in targets {
                    msg::handle_message(
                        state,
                        msg::MessageKind::Notice,
                        &self.source,
                        &target,
                        msg.as_str(),
                    );
                }
            }
            IrcCommand::Kill { .. } => {
                self.unhandled(state);
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
            IrcCommand::Numeric {
                num: RPL_WELCOME,
                args,
            } => {
                let ClientState {
                    conn_state: ConnectionState::Registration(RegistrationState { requested_nick }),
                    ..
                } = state
                else {
                    state.warn_in(
                        &Target::Status,
                        String::from("RPL_WELCOME when already registered"),
                    );
                    return Ok(());
                };
                let requested_nick = requested_nick.clone();

                let [nick, text, ..] = args.as_slice() else {
                    bail!("RPL_001 had no nick and msg arg");
                };
                let (Some(nick), Some(text)) = (nick.as_str(), text.as_str()) else {
                    bail!("nick must be a string argument");
                };

                if requested_nick != nick {
                    state.warn_in(
                        &Target::Status,
                        format!(
                            "WARNING: requested nick {}, but got nick {}",
                            requested_nick, nick
                        ),
                    );
                }

                state.conn_state =
                    ConnectionState::Connected(ConnectedState::new(nick.to_string()));
                state.add_line(Target::Status, Line::from(text.to_string()));
            }

            IrcCommand::Numeric {
                num: RPL_YOURHOST,
                args,
            } => {
                let [_, text, ..] = args.as_slice() else {
                    bail!("RPL_YOURHOST missing msg");
                };
                let Some(text) = text.as_str() else {
                    bail!("RPL_YOURHOST msg not a string");
                };
                state.add_line(Target::Status, Line::from(text.to_string()));
            }
            IrcCommand::Numeric {
                num: RPL_CREATED,
                args,
            } => {
                let [_, text, ..] = args.as_slice() else {
                    bail!("RPL_CREATED missing msg");
                };
                let Some(text) = text.as_str() else {
                    bail!("RPL_CREATED msg not a string");
                };
                state.add_line(Target::Status, Line::from(text.to_string()));
            }
            IrcCommand::Numeric {
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
                state.add_line(Target::Status, Line::from(text));
            }
            IrcCommand::Numeric {
                num: RPL_ISUPPORT,
                args: _,
            } => {
                //TODO: do we care about this?
            }

            IrcCommand::Numeric {
                num: RPL_LUSERCLIENT,
                args,
            } => {
                if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                    state.add_line(Target::Status, Line::from(msg.to_string()));
                }
            }
            IrcCommand::Numeric {
                num: RPL_LUSEROP,
                args,
            } => {
                let [_, ops, msg, ..] = args.as_slice() else {
                    // just ignore malformed replies here
                    return Ok(());
                };
                let Some(ops): Option<u16> = ops.as_str().and_then(|s| s.parse().ok()) else {
                    state.warn_in(&Target::Status, String::from("RPL_USEROP was not a u16"));
                    return Ok(());
                };
                let Some(msg) = msg.as_str() else {
                    return Ok(());
                };

                state.add_line(Target::Status, Line::from(format!("{} {}", ops, msg)));
            }
            IrcCommand::Numeric {
                num: RPL_LUSERUNKNOWN,
                args,
            } => {
                let [_, ops, msg, ..] = args.as_slice() else {
                    // just ignore malformed replies here
                    return Ok(());
                };
                let Some(connections): Option<u16> = ops.as_str().and_then(|s| s.parse().ok())
                else {
                    state.warn_in(
                        &Target::Status,
                        String::from("RPL_LUSERUNKNOWN was not a u16"),
                    );
                    return Ok(());
                };
                let Some(msg) = msg.as_str() else {
                    return Ok(());
                };

                state.add_line(
                    Target::Status,
                    Line::from(format!("{} {}", connections, msg)),
                );
            }
            IrcCommand::Numeric {
                num: RPL_LUSERCHANNELS,
                args,
            } => {
                let [_, ops, msg, ..] = args.as_slice() else {
                    // just ignore malformed replies here
                    return Ok(());
                };
                let Some(channels): Option<u16> = ops.as_str().and_then(|s| s.parse().ok()) else {
                    state.warn_in(
                        &Target::Status,
                        String::from("RPL_LUSERCHANNELS was not a u16"),
                    );
                    return Ok(());
                };
                let Some(msg) = msg.as_str() else {
                    return Ok(());
                };

                state.add_line(Target::Status, Line::from(format!("{} {}", channels, msg)));
            }
            IrcCommand::Numeric {
                num: RPL_LUSERME,
                args,
            } => {
                if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                    state.add_line(Target::Status, Line::from(msg.to_string()));
                }
            }
            IrcCommand::Numeric {
                num: RPL_LOCALUSERS,
                args,
            } => {
                if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                    state.add_line(Target::Status, Line::from(msg.to_string()));
                }
            }
            IrcCommand::Numeric {
                num: RPL_GLOBALUSERS,
                args,
            } => {
                if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                    state.add_line(Target::Status, Line::from(msg.to_string()));
                }
            }

            // =======================
            // MOTD
            // =======================
            IrcCommand::Numeric {
                num: ERR_NOMOTD,
                args,
            } => {
                state.add_line(
                    Target::Status,
                    Line::default().push(
                        format!(
                            "no MOTD: {}",
                            args.get(2).and_then(|p| p.as_str()).unwrap_or("<MISSING>")
                        )
                        .yellow(),
                    ),
                );
            }

            IrcCommand::Numeric {
                num: RPL_MOTDSTART | RPL_MOTD | RPL_ENDOFMOTD,
                args,
            } => {
                // display the MOTD to the user
                if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                    state.add_line(Target::Status, Line::from(msg.to_string()));
                }
            }

            // =======================
            // names
            // =======================
            IrcCommand::Numeric {
                num: RPL_NAMREPLY,
                args,
            } => {
                let ConnectedState { messages_state, .. } = expect_connected_state!(state, self)?;

                let [_, _, channel, names_list @ ..] = args.as_slice() else {
                    state.warn_in(&Target::Status, String::from("RPL_NAMREPLY missing params"));
                    return Ok(());
                };
                let Some(channel) = channel.as_str() else {
                    state.warn_in(
                        &Target::Status,
                        String::from("RPL_NAMREPLY malformed params"),
                    );
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
            IrcCommand::Numeric {
                num: RPL_ENDOFNAMES,
                args,
            } => {
                let ConnectedState {
                    messages_state,
                    channels,
                    ..
                } = expect_connected_state!(state, self)?;
                let [_, channel, ..] = args.as_slice() else {
                    state.warn_in(&Target::Status, String::from("RPL_ENDOFNAMES missing args"));
                    return Ok(());
                };
                let Some(name) = channel.as_str() else {
                    state.warn_in(
                        &Target::Status,
                        String::from("RPL_ENDOFNAMES malformed args"),
                    );
                    return Ok(());
                };

                let Some(channel_name) = ChannelName::new(name) else {
                    state.warn_in(
                        &Target::Status,
                        format!("RPL_ENDOFNAMES invalid channel {:?}", name),
                    );
                    return Ok(());
                };

                let Some(NamesState { names }) = messages_state.active_names.remove(name) else {
                    state.warn_in(
                        &Target::Status,
                        format!("did not expect a RPL_ENDOFNAMES for {}", name),
                    );
                    return Ok(());
                };

                let Some(channel) = channels.get_mut(&channel_name) else {
                    state.warn_in(
                        &Target::Status,
                        format!(
                            "cannot update names for channel not joined: {}",
                            channel_name.as_str()
                        ),
                    );
                    return Ok(());
                };

                channel.users.extend(names.iter().cloned());

                state.add_line(
                    Target::Channel(channel_name.clone()),
                    Line::default()
                        .push("NAMES".green())
                        .push_unstyled(" for ")
                        .push(name.blue()),
                );
                state.add_line(
                    Target::Channel(channel_name),
                    Line::from(format!(" - {}", names.join(" "))),
                );
            }

            // =======================
            // modes
            // =======================
            IrcCommand::Numeric {
                num: RPL_UMODEIS, ..
            } => {
                self.unhandled(state);
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
            IrcCommand::Numeric { .. } => {
                self.unhandled(state);
            }

            IrcCommand::Unknown { .. } => {
                state.warn_in(&Target::Status, format!("unhandled unknown msg {:?}", self));
            }

            // fatal error, the connection will be terminated
            IrcCommand::Error(err) => {
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
            IrcCommand::Pass { .. } => {
                state.warn(String::from("client received PASS"));
            }
            IrcCommand::User { .. } => {
                state.warn(String::from("client received USER"));
            }
            IrcCommand::Pong { .. } => {
                state.warn(String::from("client received PONG"));
            }
            IrcCommand::Oper => {
                state.warn(String::from("client received OPER"));
            }
            IrcCommand::Topic { .. } => {
                state.warn(String::from("client received TOPIC"));
            }
            IrcCommand::Names { .. } => {
                state.warn(String::from("client received NAMES"));
            }
            IrcCommand::List => {
                state.warn(String::from("client received LIST"));
            }
            IrcCommand::Motd { .. } => {
                state.warn(String::from("client received MOTD"));
            }
            IrcCommand::Version { .. } => {
                state.warn(String::from("client received VERSION"));
            }

            IrcCommand::Admin { .. } => {
                state.warn(String::from("client received ADMIN"));
            }
            IrcCommand::Connect { .. } => {
                state.warn(String::from("client received CONNECT"));
            }
            IrcCommand::Lusers => {
                state.warn(String::from("client received LUSERS"));
            }
            IrcCommand::Time { .. } => {
                state.warn(String::from("client received TIME"));
            }
            IrcCommand::Stats { .. } => {
                state.warn(String::from("client received STATS"));
            }
            IrcCommand::Help { .. } => {
                state.warn(String::from("client received HELP"));
            }
            IrcCommand::Info => {
                state.warn(String::from("client received INFO"));
            }
            IrcCommand::Who { .. } => {
                state.warn(String::from("client received WHO"));
            }
            IrcCommand::Whois { .. } => {
                state.warn(String::from("client received WHOIS"));
            }
            IrcCommand::WhoWas { .. } => {
                state.warn(String::from("client received WHOWAS"));
            }
            IrcCommand::Rehash => {
                state.warn(String::from("client received REHASH"));
            }
            IrcCommand::Restart => {
                state.warn(String::from("client received RESTART"));
            }
            IrcCommand::SQuit { .. } => {
                state.warn(String::from("client received SQUIT"));
            }
            IrcCommand::Away { .. } => {
                state.warn(String::from("client received AWAY"));
            }
            IrcCommand::Links => {
                state.warn(String::from("client received LINKS"));
            }
            IrcCommand::Raw { .. } => {
                state.warn(String::from(
                    "client received RAW (????? this should genuinely be unreachable?!)",
                ));
            }
        }

        Ok(())
    }
}
