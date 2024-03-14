#![feature(never_type, never_type_fallback, let_chains)]

use core::{
    any::Any,
    sync::{atomic, atomic::AtomicBool},
    time::Duration,
};
use std::{io::Write, net::TcpStream, sync::mpsc, thread};

use color_eyre::eyre::Result;
use eyre::{bail, WrapErr};
use log::*;

use crate::{
    irc_message::{IRCMessage, Message},
    reader::IrcMessageReader,
};

mod irc_message;
mod reader;

const ADDR: &str = "localhost:6667";

fn main() -> Result<()> {
    env_logger::init();
    color_eyre::install()?;

    let (reader, writer) = {
        let stream = TcpStream::connect(ADDR)?;
        (stream.try_clone()?, stream)
    };

    let (sender, receiver) = mpsc::channel::<IRCMessage>();

    static FIRST_PONG: AtomicBool = AtomicBool::new(false);

    let reader_handle = thread::spawn({
        let sender = sender.clone();
        move || -> Result<()> {
            let mut reader = IrcMessageReader::new(reader);

            loop {
                trace!("reading");
                let msg = reader.recv()?;
                trace!("got msg {:#?}", msg);
                match msg.message {
                    Message::Ping(token) => {
                        FIRST_PONG.store(true, atomic::Ordering::SeqCst);
                        sender.send(IRCMessage {
                            tags: None,
                            source: None,
                            message: Message::Pong(token),
                        })?;
                    }
                    _ => {}
                }
            }
        }
    });

    let writer_handle = thread::spawn(|| -> Result<()> {
        let mut writer = writer;
        let receiver = receiver;

        loop {
            let msg = receiver.recv()?;
            let s = msg.to_irc_string();
            trace!("sending message: {:#?}: {:?}", msg, s);
            writer.write_all(s.as_bytes())?;
            writer.flush()?;
        }
    });

    sender.send(IRCMessage {
        tags: None,
        source: None,
        message: Message::Nick(String::from("rust_client")),
    })?;
    sender.send(IRCMessage {
        tags: None,
        source: None,
        message: Message::User(String::from("rust_client"), String::from("rust_client")),
    })?;

    while !FIRST_PONG.load(atomic::Ordering::SeqCst) {}

    sender.send(IRCMessage {
        tags: None,
        source: None,
        message: Message::Join(vec![(String::from("#testing"), None)]),
    })?;
    sender.send(IRCMessage {
        tags: None,
        source: None,
        message: Message::Privmsg(
            vec![String::from("#testing")],
            String::from("this is a test msg"),
        ),
    })?;

    let mut ping_idx = 0;
    loop {
        thread::sleep(Duration::from_secs(1));
        sender.send(IRCMessage {
            tags: None,
            source: None,
            message: Message::Ping(ping_idx.to_string()),
        })?;
        ping_idx += 1;
    }

    match writer_handle.join() {
        Ok(res) => res.wrap_err("writer thread errored")?,
        Err(payload) => bail!(
            "writer thread panicked: {}",
            get_panic_payload_msg(&*payload)
        ),
    }

    match reader_handle.join() {
        Ok(res) => res.wrap_err("reader thread errored")?,
        Err(payload) => bail!(
            "reader thread panicked: {}",
            get_panic_payload_msg(&*payload)
        ),
    }

    Ok(())
}

fn get_panic_payload_msg(payload: &dyn Any) -> &str {
    match payload.downcast_ref::<&'static str>() {
        Some(&s) => s,
        None => match payload.downcast_ref::<String>() {
            Some(s) => s.as_str(),
            None => "Box<dyn Any>",
        },
    }
}
