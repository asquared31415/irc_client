use crossterm::style::Stylize;
use eyre::bail;
use log::debug;

use crate::{
    irc_message::Message,
    state::{ClientState, ConnectedState, ConnectionState, NamesState},
    ui::{term::UiMsg, text::Line},
};

pub fn handle(msg: &Message, state: &mut ClientState) -> eyre::Result<()> {
    let Message::Numeric { num, args } = msg else {
        unreachable!()
    };
    let ui_sender = &mut state.ui_sender;

    let ClientState {
        conn_state:
            ConnectionState::Connected(ConnectedState {
                channels,
                messages_state,
                ..
            }),
        ..
    } = state
    else {
        bail!("cannot handle messages when not yet connected");
    };

    use crate::constants::numerics::*;
    match *num {
        // =======================
        // LUSERS responses
        // =======================
        RPL_LUSERCLIENT => {
            if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                ui_sender.send(UiMsg::Writeln(Line::from(msg.to_string())));
            }
        }
        RPL_LUSEROP => {
            let [_, ops, msg, ..] = args.as_slice() else {
                // just ignore malformed replies here
                return Ok(());
            };
            let Some(ops): Option<u16> = ops.as_str().and_then(|s| s.parse().ok()) else {
                ui_sender.send(UiMsg::Warn(String::from("RPL_USEROP was not a u16")));
                return Ok(());
            };
            let Some(msg) = msg.as_str() else {
                return Ok(());
            };

            ui_sender.send(UiMsg::Writeln(Line::from(format!("{} {}", ops, msg))));
        }
        RPL_LUSERUNKNOWN => {
            let [_, ops, msg, ..] = args.as_slice() else {
                // just ignore malformed replies here
                return Ok(());
            };
            let Some(connections): Option<u16> = ops.as_str().and_then(|s| s.parse().ok()) else {
                ui_sender.send(UiMsg::Warn(String::from("RPL_LUSERUNKNOWN was not a u16")));
                return Ok(());
            };
            let Some(msg) = msg.as_str() else {
                return Ok(());
            };

            ui_sender.send(UiMsg::Writeln(Line::from(format!(
                "{} {}",
                connections, msg
            ))));
        }
        RPL_LUSERCHANNELS => {
            let [_, ops, msg, ..] = args.as_slice() else {
                // just ignore malformed replies here
                return Ok(());
            };
            let Some(channels): Option<u16> = ops.as_str().and_then(|s| s.parse().ok()) else {
                ui_sender.send(UiMsg::Warn(String::from("RPL_LUSERCHANNELS was not a u16")));
                return Ok(());
            };
            let Some(msg) = msg.as_str() else {
                return Ok(());
            };

            ui_sender.send(UiMsg::Writeln(Line::from(format!("{} {}", channels, msg))));
        }
        RPL_LUSERME => {
            if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                ui_sender.send(UiMsg::Writeln(Line::from(msg.to_string())));
            }
        }
        RPL_LOCALUSERS => {
            if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                ui_sender.send(UiMsg::Writeln(Line::from(msg.to_string())));
            }
        }
        RPL_GLOBALUSERS => {
            if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                ui_sender.send(UiMsg::Writeln(Line::from(msg.to_string())));
            }
        }

        // =======================
        // MOTD
        // =======================
        ERR_NOMOTD => {
            ui_sender.send(UiMsg::Writeln(
                Line::default().push(
                    format!(
                        "no MOTD: {}",
                        args.get(2).and_then(|p| p.as_str()).unwrap_or("<MISSING>")
                    )
                    .yellow(),
                ),
            ));
        }

        RPL_MOTDSTART | RPL_MOTD | RPL_ENDOFMOTD => {
            // display the MOTD to the user
            if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                ui_sender.send(UiMsg::Writeln(Line::from(msg.to_string())));
            }
        }

        // =======================
        // names
        // =======================
        RPL_NAMREPLY => {
            let [_, _, channel, names_list @ ..] = args.as_slice() else {
                ui_sender.send(UiMsg::Warn(String::from("RPL_NAMREPLY missing params")));
                return Ok(());
            };
            let Some(channel) = channel.as_str() else {
                ui_sender.send(UiMsg::Warn(String::from("RPL_NAMEREPLY malformed args")));
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
        RPL_ENDOFNAMES => {
            let [_, channel, ..] = args.as_slice() else {
                ui_sender.send(UiMsg::Warn(String::from("RPL_ENDOFNAMES missing args")));
                return Ok(());
            };
            let Some(channel_name) = channel.as_str() else {
                ui_sender.send(UiMsg::Warn(String::from("RPL_ENDOFNAMES malformed args")));
                return Ok(());
            };

            let Some(NamesState { names }) = messages_state.active_names.remove(channel_name)
            else {
                ui_sender.send(UiMsg::Warn(format!(
                    "unexpected RPL_NAMEREPLY for {}",
                    channel_name
                )));
                return Ok(());
            };

            let Some(channel) = channels.iter_mut().find(|c| c.name() == channel_name) else {
                ui_sender.send(UiMsg::Warn(format!(
                    "cannot update names for channel not joined: {}",
                    channel_name
                )));
                return Ok(());
            };

            channel.users.extend(names.iter().cloned());
            debug!("{:#?}", channel);

            ui_sender.send(UiMsg::Writeln(
                Line::default()
                    .push("NAMES".green())
                    .push_unstyled(" for ")
                    .push(channel_name.blue()),
            ));
            ui_sender.send(UiMsg::Writeln(Line::from(format!(
                " - {}",
                names.join(" ")
            ))));
        }

        // =======================
        // modes
        // =======================
        RPL_UMODEIS => {
            ui_sender.send(UiMsg::Warn(String::from("TODO: RPL_UMODEIS")));
        }

        // =======================
        // fallback
        // =======================
        _ => {
            ui_sender.send(UiMsg::Warn(format!(
                "unhandled numeric {} ({:03})",
                crate::constants::numerics::ALL_NUMERICS
                    .get(&num)
                    .unwrap_or(&"UNKNOWN"),
                num
            )));
        }
    }

    Ok(())
}
