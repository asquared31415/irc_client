#![feature(never_type, never_type_fallback, let_chains, lazy_cell)]

use std::sync::mpsc::Sender;

use clap::Parser;
use color_eyre::eyre::Result;

use crate::{
    client::ExitReason,
    irc_message::{IRCMessage, Message},
    ui::layout::{Direction, Layout, Section, SectionKind},
};

mod client;
mod command;
mod constants;
mod ext;
mod handlers;
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

    #[arg(long)]
    twitch_token: Option<String>,
}

fn main() -> Result<()> {
    color_eyre::install()?;

    if option_env!("RECT_DBG").is_some() {
        let layout = Layout {
            direction: Direction::Vertical,
            sections: vec![
                Section::Leaf {
                    kind: SectionKind::Exact(1),
                },
                Section::Node {
                    direction: Direction::Horizontal,
                    kind: SectionKind::Fill(1),
                    sub_sections: vec![
                        Section::Leaf {
                            kind: SectionKind::Fill(2),
                        },
                        Section::Leaf {
                            kind: SectionKind::Fill(1),
                        },
                    ],
                },
            ],
        };

        let rects = layout.calc((80, 20));
        dbg!(&rects);
        return Ok(());
    }

    let Cli {
        addr,
        tls,
        nick,
        twitch_token,
    } = Cli::parse();

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
