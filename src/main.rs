#![feature(
    never_type,
    never_type_fallback,
    let_chains,
    lazy_cell,
    thread_id_value,
    utf8_chunks
)]

use std::{
    panic::{catch_unwind, resume_unwind},
    sync::mpsc::Sender,
};

use clap::Parser;
use crossterm::terminal;
use eyre::{bail, eyre};
use log::LevelFilter;

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
    color_eyre::install()?;

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

    match catch_unwind(|| client::start(addr.as_str(), nick.as_str(), tls, client_on_start)) {
        Ok(res) => {
            match res {
                // client.start() never returns Ok
                Ok(_) => unreachable!(),
                // no need to report anything on a requsted quit
                Err(ExitReason::Quit) => return Ok(()),
                Err(e) => return Err(e.into()),
            }
        }
        Err(payload) => {
            terminal::disable_raw_mode()?;
            resume_unwind(payload)
        }
    }
}
