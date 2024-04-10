#![feature(
    never_type,
    never_type_fallback,
    let_chains,
    lazy_cell,
    thread_id_value,
    utf8_chunks,
    round_char_boundary
)]

use std::{panic::set_hook, sync::mpsc::Sender};

use clap::Parser;
use crossterm::{execute, terminal};
use eyre::{bail, eyre};
use log::*;

use crate::{
    client::ExitReason,
    irc_message::{IRCMessage, Message},
};

mod channel;
mod client;
mod command;
mod constants;
mod ext;
mod handlers;
mod irc_message;
mod logging;
mod server_io;
mod state;
mod ui;
mod util;

#[derive(Debug, Parser)]
#[command(version)]
struct Cli {
    #[arg(long)]
    addr: String,

    #[arg(long)]
    tls: bool,

    #[arg(long)]
    nick: String,

    #[arg(long)]
    twitch_token: Option<String>,
}

fn main() -> eyre::Result<()> {
    let (panic_hook, eyre_hook) = color_eyre::config::HookBuilder::new().into_hooks();
    eyre_hook.install()?;
    set_hook(Box::new({
        let panic_hook = panic_hook.into_panic_hook();
        move |panic_info| {
            let _ = terminal::disable_raw_mode();
            let _ = execute!(std::io::stdout(), terminal::LeaveAlternateScreen);
            panic_hook(panic_info);
            std::process::abort()
        }
    }));

    let Cli {
        addr,
        tls,
        nick,
        twitch_token,
    } = Cli::parse();

    let Some((name, _)) = addr.split_once(':') else {
        bail!("unable to parse hostname");
    };

    logging::init(name, LevelFilter::Debug).map_err(|_| eyre!("failed to init logger"))?;

    //code to run upon starting.
    let client_on_start = |sender: &Sender<IRCMessage>| {
        if let Some(token) = twitch_token.as_ref() {
            sender.send(IRCMessage {
                tags: None,
                source: None,
                message: Message::Pass(token.to_string()),
            })?;
        }

        sender.send(IRCMessage {
            tags: None,
            source: None,
            message: Message::Nick(nick.clone()),
        })?;
        sender.send(IRCMessage {
            tags: None,
            source: None,
            message: Message::User(nick.clone(), nick.clone()),
        })?;

        Ok(())
    };

    match client::start(addr.as_str(), nick.as_str(), tls, client_on_start) {
        // client.start() never returns Ok
        Ok(_) => unreachable!(),
        // no need to report anything on a requsted quit
        Err(ExitReason::Quit) => return Ok(()),
        Err(e) => return Err(e.into()),
    }
}
