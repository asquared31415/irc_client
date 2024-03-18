#![feature(never_type, never_type_fallback, let_chains)]

use clap::Parser;
use color_eyre::eyre::Result;
use log::debug;

use crate::irc_message::{IRCMessage, Message};

mod client;
mod command;
mod ext;
mod irc_message;
mod server_io;
mod ui;

#[derive(Debug, Parser)]
#[command(version)]
struct Cli {
    #[arg(long)]
    addr: String,

    #[arg(long)]
    tls: bool,

    #[arg(long)]
    nick: String,
}

fn main() -> Result<()> {
    env_logger::init();
    color_eyre::install()?;

    let cli = Cli::parse();
    debug!("{:#?}", cli);

    let Cli { addr, tls, nick } = cli;

    client::start(addr.as_str(), nick.as_str(), tls, |sender| {
        // code to run upon starting.
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
    })?
    // client.start() never returns
    // PROGRAM SHOULD NEVER EXIT EXCEPT BY USER REQUEST
}
