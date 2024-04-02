use crate::{irc_message::Message, ui::term::TerminalUi};

pub fn handle(msg: Message, ui: &mut TerminalUi) -> eyre::Result<()> {
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
            ui.writeln(format!(
                "no MOTD: {}",
                args.get(2).and_then(|p| p.as_str()).unwrap_or("")
            ))?;
        }

        // =======================
        // modes
        // =======================
        RPL_UMODEIS => {
            ui.writeln("TODO: RPL_UMODEIS")?;
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
            ));
        }
    }

    Ok(())
}
