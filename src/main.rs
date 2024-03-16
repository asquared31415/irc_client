#![feature(never_type, never_type_fallback, let_chains)]

use color_eyre::eyre::Result;

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

const ADDR: &str = "localhost:6667";
const NICK: &str = "rust_client";

fn main() -> Result<()> {
    env_logger::init();
    color_eyre::install()?;

    // TODO: probobaly join creation and starting, or at least defer tcp connection until start
    let client = Client::new(ADDR, NICK)?;
    client.start(|sender| {
        // code to run upon starting.
        sender.send(IRCMessage {
            tags: None,
            source: None,
            message: Message::Nick(String::from(NICK)),
        })?;
        sender.send(IRCMessage {
            tags: None,
            source: None,
            message: Message::User(String::from(NICK), String::from(NICK)),
        })?;

        Ok(())
    })?
    // client.start() never returns
    // PROGRAM SHOULD NEVER EXIT EXCEPT BY USER REQUEST
}
