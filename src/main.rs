#![feature(never_type, never_type_fallback, let_chains)]

use std::{
    io::{Read, Write},
    net::TcpStream,
};

use color_eyre::eyre::Result;
use eyre::bail;
use log::*;

use crate::{irc_message::IRCMessage, reader::IrcMessageReader};

mod irc_message;
mod reader;

const ADDR: &str = "localhost:6667";

fn main() -> Result<()> {
    env_logger::init();
    color_eyre::install()?;

    // const TEST_MSG: &str = "PING nya";
    // let x = IRCMessage::parse(TEST_MSG)?;
    // debug!("{:#?}", x);
    // assert_eq!(x.to_irc_string(), format!("{}\r\n", TEST_MSG));

    let mut stream = TcpStream::connect(ADDR)?;

    stream.write_all(b"NICK asquared31415\r\n")?;
    // stream.write_all(b"USER asquared31415 0 * asquared31415")?;
    stream.flush()?;

    let mut reader = IrcMessageReader::new(stream, |msg| {
        debug!("{:#?}", msg);
    });

    debug!("polling reader");
    loop {
        trace!("poll");
        reader.poll()?;
    }

    Ok(())
}
