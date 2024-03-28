use std::io;

use eyre::bail;
use ratatui::backend::Backend;

use crate::{irc_message::Message, ui::TerminalUi};

pub fn handle<B: Backend + io::Write>(msg: Message, ui: &mut TerminalUi<B>) -> eyre::Result<()> {
    let Message::Numeric { num, args } = msg else {
        unreachable!()
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
                ui.warn("RPL_USEROP was not a u16");
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
                ui.warn("RPL_LUSERUNKNOWN was not a u16");
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
                ui.warn("RPL_LUSERCHANNELS was not a u16");
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
        RPL_LOCALUSERS => {}
        RPL_GLOBALUSERS => {}

        // =======================
        // MOTD
        // =======================
        ERR_NOMOTD => {
            ui.writeln(format!(
                "no MOTD: {}",
                args.first().and_then(|p| p.as_str()).unwrap_or("")
            ))?;
        }

        _ => {
            ui.warn(format!("unhandled numeric {:03}", num));
        }
    }

    Ok(())
}
