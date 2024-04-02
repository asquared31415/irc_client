#![feature(never_type, never_type_fallback, let_chains, lazy_cell)]

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
}

fn main() -> Result<()> {
    color_eyre::install()?;

    if option_env!("RECT_DBG").is_some() {
        let layout = Layout {
            direction: Direction::Horizontal,
            sections: vec![
                Section {
                    direction: Direction::Vertical,
                    kind: SectionKind::Exact(1),
                    sub_sections: vec![],
                },
                Section {
                    direction: Direction::Horizontal,
                    kind: SectionKind::Fill(1),
                    sub_sections: vec![
                        Section {
                            direction: Direction::Vertical,
                            kind: SectionKind::Fill(2),
                            sub_sections: vec![],
                        },
                        Section {
                            direction: Direction::Vertical,
                            kind: SectionKind::Fill(1),
                            sub_sections: vec![],
                        },
                    ],
                },
            ],
        };

        let rects = layout.calc((80, 20));
        dbg!(&rects);
        return Ok(());
    }

    let Cli { addr, tls, nick } = Cli::parse();

    match client::start(addr.as_str(), nick.as_str(), tls, |sender| {
        //code to run upon starting.
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
    }) {
        // client.start() never returns Ok
        Ok(_) => unreachable!(),
        // no need to report anything on a requsted quit
        Err(ExitReason::Quit) => return Ok(()),
        Err(e) => return Err(e.into()),
    }
}
