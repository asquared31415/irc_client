use crossterm::style::Stylize;
use eyre::bail;
use log::debug;

use crate::{
    irc_message::Message,
    state::{ClientState, ConnectedState, ConnectionState, NamesState},
    ui::text::Line,
};

pub fn handle(msg: Message, state: &mut ClientState) -> eyre::Result<()> {
    let Message::Numeric { num, args } = msg else {
        unreachable!()
    };

    let ClientState {
        ui,
        conn_state:
            ConnectionState::Connected(ConnectedState {
                channels,
                messages_state,
                ..
            }),
    } = state
    else {
        bail!("cannot handle messages when not yet connected");
    };

    use crate::constants::numerics::*;
    match num {
        // =======================
        // LUSERS responses
        // =======================
        RPL_LUSERCLIENT => {
            if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                ui.writeln(msg.to_string())?;
            }
        }
        RPL_LUSEROP => {
            let [_, ops, msg, ..] = args.as_slice() else {
                // just ignore malformed replies here
                return Ok(());
            };
            let Some(ops): Option<u16> = ops.as_str().and_then(|s| s.parse().ok()) else {
                ui.warn("RPL_USEROP was not a u16")?;
                return Ok(());
            };
            let Some(msg) = msg.as_str() else {
                return Ok(());
            };

            ui.writeln(format!("{} {}", ops, msg))?;
        }
        RPL_LUSERUNKNOWN => {
            let [_, ops, msg, ..] = args.as_slice() else {
                // just ignore malformed replies here
                return Ok(());
            };
            let Some(ops): Option<u16> = ops.as_str().and_then(|s| s.parse().ok()) else {
                ui.warn("RPL_LUSERUNKNOWN was not a u16")?;
                return Ok(());
            };
            let Some(msg) = msg.as_str() else {
                return Ok(());
            };

            ui.writeln(format!("{} {}", ops, msg))?;
        }
        RPL_LUSERCHANNELS => {
            let [_, ops, msg, ..] = args.as_slice() else {
                // just ignore malformed replies here
                return Ok(());
            };
            let Some(ops): Option<u16> = ops.as_str().and_then(|s| s.parse().ok()) else {
                ui.warn("RPL_LUSERCHANNELS was not a u16")?;
                return Ok(());
            };
            let Some(msg) = msg.as_str() else {
                return Ok(());
            };

            ui.writeln(format!("{} {}", ops, msg))?;
        }
        RPL_LUSERME => {
            if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                ui.writeln(msg.to_string())?;
            }
        }
        RPL_LOCALUSERS => {
            if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                ui.writeln(msg.to_string())?;
            }
        }
        RPL_GLOBALUSERS => {
            if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                ui.writeln(msg.to_string())?;
            }
        }

        // =======================
        // MOTD
        // =======================
        ERR_NOMOTD => {
            ui.writeln(
                Line::default().push(
                    format!(
                        "no MOTD: {}",
                        args.get(2).and_then(|p| p.as_str()).unwrap_or("<MISSING>")
                    )
                    .yellow(),
                ),
            )?;
        }

        RPL_MOTDSTART | RPL_MOTD | RPL_ENDOFMOTD => {
            // display the MOTD to the user
            if let Some(msg) = args.last().and_then(|p| p.as_str()) {
                ui.writeln(msg.to_string())?;
            }
        }

        // =======================
        // names
        // =======================
        RPL_NAMREPLY => {
            let [_, _, channel, names_list @ ..] = args.as_slice() else {
                ui.warn(format!("RPL_NAMREPLY missing params"))?;
                return Ok(());
            };
            let Some(channel) = channel.as_str() else {
                ui.warn("RPL_NAMEREPLY malformed args")?;
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
                ui.warn("RPL_ENDOFNAMES missing args")?;
                return Ok(());
            };
            let Some(channel_name) = channel.as_str() else {
                ui.warn("RPL_ENDOFNAMES malformed args")?;
                return Ok(());
            };

            let Some(NamesState { names }) = messages_state.active_names.remove(channel_name)
            else {
                ui.warn(format!("unexpected RPL_NAMEREPLY for {}", channel_name))?;
                return Ok(());
            };

            let Some(channel) = channels.iter_mut().find(|c| c.name() == channel_name) else {
                ui.warn(format!(
                    "cannot update names for channel not joined: {}",
                    channel_name
                ))?;
                return Ok(());
            };

            channel.users.extend(names.iter().cloned());
            debug!("{:#?}", channel);

            ui.writeln(
                Line::default()
                    .push("NAMES".green())
                    .push_unstyled(" for ")
                    .push(channel_name.blue()),
            )?;
            ui.writeln(format!(" - {}", names.join(" ")))?;
        }

        // =======================
        // modes
        // =======================
        RPL_UMODEIS => {
            ui.warn("TODO: RPL_UMODEIS")?;
        }

        // =======================
        // fallback
        // =======================
        _ => {
            ui.warn(format!(
                "unhandled numeric {} ({:03})",
                crate::constants::numerics::ALL_NUMERICS
                    .get(&num)
                    .unwrap_or(&"UNKNOWN"),
                num
            ))?;
        }
    }

    Ok(())
}
