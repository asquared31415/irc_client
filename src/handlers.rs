use crossterm::style::Stylize as _;
use eyre::{bail, eyre};

use crate::{
    channel::channel::Channel,
    irc_message::{IrcMessage, Message, Param, Source},
    state::{ClientState, ConnectedState, ConnectionState, NamesState, RegistrationState},
    ui::text::Line,
    util,
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

impl IrcMessage {
    fn unhandled(&self, state: &mut ClientState) {
        state.warn(format!("unhandled msg {:?}", self));
    }

    // FIXME: remove this
    #[allow(unused_must_use)]
    pub fn handle(&self, state: &mut ClientState) -> eyre::Result<()> {
        use crate::constants::numerics::*;
        match &self.message {
            Message::Cap => {
                self.unhandled(state);
            }
            Message::Authenticate => {
                self.unhandled(state);
            }
            Message::Nick(_) => {
                self.unhandled(state);
            }
            Message::Ping(token) => {
                state.msg_sender.send(IrcMessage {
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
                state.add_line(
                    Target::Status,
                    util::line_now()
                        .push(name.magenta())
                        .push_unstyled(" quit: ")
                        .push_unstyled(reason),
                );
            }
            Message::Join(join_channels) => {
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
                                .push(channel.name().dark_blue());

                            state.add_line(channel.target(), line);
                        }
                    }
                    None => {
                        state.warn_in(&Target::Status, String::from("JOIN msg without a source"));
                    }
                }
            }
            Message::Part(channels, reason) => {
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
            Message::Invite { .. } => {
                self.unhandled(state);
            }
            Message::Kick { .. } => {
                self.unhandled(state);
            }
            Message::Mode { target, mode } => {
                let Some(mode) = mode else {
                    state.warn_in(
                        &Target::Status,
                        format!("server sent MODE for {} without modestr", target),
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
                        let Some(channel) = channels.get_mut(target) else {
                            state.warn_in(
                                &Target::Status,
                                format!("unexpected MODE for not joined channel {}", channel_name),
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
            Message::Privmsg { targets, msg } => {
                for target in targets {
                    let mut line = util::line_now();
                    if let Some(source) = self.source.as_ref() {
                        line = line.join(create_nick_line(source.get_name(), false));
                    }
                    line = line.join(Line::from(msg.to_string()));
                    state.add_line(target.clone(), line);
                }
            }
            Message::Notice { targets, msg } => {
                for target in targets {
                    let mut line = util::line_now();
                    if let Some(source) = self.source.as_ref() {
                        line = line.join(create_nick_line(source.get_name(), false));
                    }
                    line = line.join(Line::default().push("NOTICE ".green()).push_unstyled(msg));
                    state.add_line(target.clone(), line);
                }
            }
            Message::Kill { .. } => {
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
            Message::Numeric {
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
                state.add_line(Target::Status, Line::from(text.to_string()));
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
                state.add_line(Target::Status, Line::from(text.to_string()));
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
                state.add_line(Target::Status, Line::from(text));
            }
            Message::Numeric {
                num: RPL_ISUPPORT,
                args: _,
            } => {
                //TODO: do we care about this?
            }

            Message::Numeric {
                num: RPL_LUSERCLIENT,
                args,
            } => {
                if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                    state.add_line(Target::Status, Line::from(msg.to_string()));
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
                    state.warn_in(&Target::Status, String::from("RPL_USEROP was not a u16"));
                    return Ok(());
                };
                let Some(msg) = msg.as_str() else {
                    return Ok(());
                };

                state.add_line(Target::Status, Line::from(format!("{} {}", ops, msg)));
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
            Message::Numeric {
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
            Message::Numeric {
                num: RPL_LUSERME,
                args,
            } => {
                if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                    state.add_line(Target::Status, Line::from(msg.to_string()));
                }
            }
            Message::Numeric {
                num: RPL_LOCALUSERS,
                args,
            } => {
                if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                    state.add_line(Target::Status, Line::from(msg.to_string()));
                }
            }
            Message::Numeric {
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
            Message::Numeric {
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

            Message::Numeric {
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
            Message::Numeric {
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
                    state.warn_in(&Target::Status, String::from("RPL_ENDOFNAMES missing args"));
                    return Ok(());
                };
                let Some(channel_name) = channel.as_str() else {
                    state.warn_in(
                        &Target::Status,
                        String::from("RPL_ENDOFNAMES malformed args"),
                    );
                    return Ok(());
                };

                let Some(target) = Target::new(channel_name) else {
                    state.warn_in(
                        &Target::Status,
                        format!("RPL_ENDOFNAMES invalid channel {:?}", channel_name),
                    );
                    return Ok(());
                };

                let Some(NamesState { names }) = messages_state.active_names.remove(channel_name)
                else {
                    state.warn_in(
                        &Target::Status,
                        format!("did not expect a RPL_ENDOFNAMES for {}", channel_name),
                    );
                    return Ok(());
                };

                let Some(channel) = channels.get_mut(&target) else {
                    state.warn_in(
                        &Target::Status,
                        format!(
                            "cannot update names for channel not joined: {}",
                            channel_name
                        ),
                    );
                    return Ok(());
                };

                channel.users.extend(names.iter().cloned());

                state.add_line(
                    target.clone(),
                    Line::default()
                        .push("NAMES".green())
                        .push_unstyled(" for ")
                        .push(channel_name.blue()),
                );
                state.add_line(target, Line::from(format!(" - {}", names.join(" "))));
            }

            // =======================
            // modes
            // =======================
            Message::Numeric {
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
            Message::Numeric { .. } => {
                self.unhandled(state);
            }

            Message::Unknown { .. } => {
                state.warn_in(&Target::Status, format!("unhandled unknown msg {:?}", self));
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
                state.warn(String::from("client received PASS"));
            }
            Message::User { .. } => {
                state.warn(String::from("client received USER"));
            }
            Message::Pong { .. } => {
                state.warn(String::from("client received PONG"));
            }
            Message::Oper => {
                state.warn(String::from("client received OPER"));
            }
            Message::Topic { .. } => {
                state.warn(String::from("client received TOPIC"));
            }
            Message::Names { .. } => {
                state.warn(String::from("client received NAMES"));
            }
            Message::List => {
                state.warn(String::from("client received LIST"));
            }
            Message::Motd { .. } => {
                state.warn(String::from("client received MOTD"));
            }
            Message::Version { .. } => {
                state.warn(String::from("client received VERSION"));
            }

            Message::Admin { .. } => {
                state.warn(String::from("client received ADMIN"));
            }
            Message::Connect { .. } => {
                state.warn(String::from("client received CONNECT"));
            }
            Message::Lusers => {
                state.warn(String::from("client received LUSERS"));
            }
            Message::Time { .. } => {
                state.warn(String::from("client received TIME"));
            }
            Message::Stats { .. } => {
                state.warn(String::from("client received STATS"));
            }
            Message::Help { .. } => {
                state.warn(String::from("client received HELP"));
            }
            Message::Info => {
                state.warn(String::from("client received INFO"));
            }
            Message::Who { .. } => {
                state.warn(String::from("client received WHO"));
            }
            Message::Whois { .. } => {
                state.warn(String::from("client received WHOIS"));
            }
            Message::WhoWas { .. } => {
                state.warn(String::from("client received WHOWAS"));
            }
            Message::Rehash => {
                state.warn(String::from("client received REHASH"));
            }
            Message::Restart => {
                state.warn(String::from("client received RESTART"));
            }
            Message::SQuit { .. } => {
                state.warn(String::from("client received SQUIT"));
            }
            Message::Away { .. } => {
                state.warn(String::from("client received AWAY"));
            }
            Message::Links => {
                state.warn(String::from("client received LINKS"));
            }
            Message::Raw { .. } => {
                state.warn(String::from(
                    "client received RAW (????? this should genuinely be unreachable?!)",
                ));
            }
        }

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
