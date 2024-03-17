#![feature(never_type, never_type_fallback, let_chains)]

use clap::Parser;
use color_eyre::eyre::Result;
use log::info;

use crate::{
    client_state::Client,
    irc_message::{IRCMessage, Message},
};

mod client_state;
mod command;
mod ext;
mod irc_message;
mod reader;
mod ui;

#[derive(Debug, Parser)]
#[command(version)]
struct Cli {
    #[arg(long)]
    addr: String,

    #[arg(long)]
    nick: String,
}

fn main() -> Result<()> {
    env_logger::init();
    color_eyre::install()?;

    let cli = Cli::parse();
    info!("{:#?}", cli);

    let Cli { addr, nick } = cli;

    // TODO: probobaly join creation and starting, or at least defer tcp connection until start
    let client = Client::new(addr.as_str(), nick.as_str())?;
    client.start(|sender| {
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
