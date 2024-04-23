use core::{
    sync::atomic::{self, AtomicBool},
    time::Duration,
};
use std::{
    io,
    net::TcpStream,
    sync::{
        mpsc,
        mpsc::{Sender, TryRecvError},
        Arc, Mutex,
    },
    thread,
};

use crossterm::style::Stylize as _;
use eyre::{bail, eyre, Context};
use log::*;
use rustls::{pki_types::ServerName, ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use thiserror::Error;

use crate::{
    command::Command,
    ext::*,
    irc_message::{IRCMessage, Message},
    server_io::ServerIo,
    state::{ClientState, ConnectedState, ConnectionState},
    ui::{
        layout::{Direction, Layout, Section, SectionKind},
        term::TerminalUi,
        text::Line,
    },
    util::Target,
};

#[derive(Debug, Error)]
pub enum ExitReason {
    #[error("quit requested")]
    Quit,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Other(#[from] eyre::Report),
}

pub static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// spawns threads for the reading and writing parts of the client and begins processing the
/// connection.
pub fn start(
    addr: &str,
    nick: &str,
    tls: bool,
    init: impl Fn(&Sender<IRCMessage>) -> eyre::Result<()>,
) -> Result<!, ExitReason> {
    let Some((name, _)) = addr.split_once(':') else {
        return Err(eyre!("unable to determine host name for TLS"))?;
    };

    // set non-blocking so that reads and writes can happen on one thread
    // only one thread can be used because TLS has state that's not therad safe
    let stream = TcpStream::connect(addr)?;
    stream.set_nonblocking(true)?;
    let stream: Box<dyn ReadWrite + Send> = if tls {
        // Mozilla's root certificates
        let root_store = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = Arc::new(
            ClientConfig::builder()
                .with_root_certificates(root_store)
                .with_no_client_auth(),
        );
        let server_name =
            ServerName::try_from(name.to_string()).wrap_err("could not parse server name")?;
        let client =
            ClientConnection::new(config, server_name).wrap_err("could not create connection")?;
        Box::new(StreamOwned::new(client, stream))
    } else {
        Box::new(stream)
    };

    // send to this channel to have a message written to the server
    let (write_sender, write_receiver) = mpsc::channel::<IRCMessage>();
    // recv from this channel to get incoming messages from the server
    let (msg_sender, msg_receiver) = mpsc::channel::<IRCMessage>();

    let layout = Layout {
        direction: Direction::Vertical,
        sections: vec![
            Section::Leaf {
                kind: SectionKind::Fill(1),
            },
            Section::Leaf {
                kind: SectionKind::Exact(1),
            },
        ],
    };
    let mut state = ClientState::new(
        write_sender.clone(),
        TerminalUi::new(layout, io::stdout())?,
        nick.to_string(),
    );
    // draw the status page immediately
    state.render()?;
    let state = Arc::new(Mutex::new(state));

    // stream reader and writer thread
    // moves the stream into the thread
    let _ = thread::spawn({
        let state = Arc::clone(&state);
        move || {
            let mut connection = ServerIo::new(stream);

            loop {
                if QUIT_REQUESTED.load(atomic::Ordering::Relaxed) {
                    return;
                }

                let res = || -> eyre::Result<()> {
                    // write any necessary messages
                    match write_receiver.try_recv() {
                        Ok(msg) => {
                            connection.write(&msg)?;
                        }
                        // if empty, move on to try to read
                        Err(TryRecvError::Empty) => {}
                        Err(TryRecvError::Disconnected) => {
                            bail!("connection writer channel disconnected");
                        }
                    }

                    match connection.recv() {
                        Ok(messages) => {
                            for msg in messages {
                                msg_sender.send(msg)?;
                            }
                        }
                        Err(e) => Err(e)?,
                    }

                    Ok(())
                }();

                match res {
                    Ok(()) => {}
                    Err(e) => {
                        state.lock().unwrap().ui.error(e.to_string()).unwrap();
                        return;
                    }
                }
            }
        }
    });

    // user interaction using stdin
    let _ = thread::spawn({
        let state = Arc::clone(&state);
        let queue_sender = write_sender.clone();
        move || {
            loop {
                // NOTE: other threads can sometimes set QUIT_REQUESTED
                if QUIT_REQUESTED.load(atomic::Ordering::Relaxed) {
                    return;
                }

                let input = || -> Result<(), InputErr> {
                    let state = &mut *state.lock().unwrap();
                    // note: this re-renders if needed
                    match state.input() {
                        Ok(Some(input)) => handle_input(state, &queue_sender, input.as_str()),
                        Ok(None) => Ok(()),
                        Err(e) => Err(e.into()),
                    }
                }();

                match input {
                    // no input yet, just loop
                    Ok(()) => {
                        // the delay between input polls. this needs to exist so that this code
                        // isn't constanly locking and unlocking the state
                        // mutex, which was causing other code to never get a
                        // turn on the mutex.
                        const INPUT_POLL_DELAY: Duration = Duration::from_millis(10);
                        thread::sleep(INPUT_POLL_DELAY);
                    }
                    Err(InputErr::Io(e)) => {
                        state.lock().unwrap().ui.error(e.to_string()).unwrap();
                        return;
                    }
                    Err(InputErr::Other(e)) => {
                        state.lock().unwrap().ui.error(e.to_string()).unwrap();
                        return;
                    }
                }
            }
        }
    });

    // call the init function that controls how to register
    init(&write_sender)?;

    // main code that processes state as messages come in
    // TODO: do processing on a thread too
    loop {
        if QUIT_REQUESTED.load(atomic::Ordering::Relaxed) {
            let ui = &mut state.lock().unwrap().ui;
            ui.disable();
            return Err(ExitReason::Quit);
        }

        let msg = match msg_receiver.try_recv() {
            Ok(msg) => msg,
            Err(_) => continue,
        };

        let state = &mut *state.lock().unwrap();
        msg.handle(state)?;
        trace!("state after handling {:#?}", state.conn_state);
    }
}

/// describes **fatal** errors in input handling.
#[derive(Debug, Error)]
enum InputErr {
    #[error(transparent)]
    Io(io::Error),
    #[error(transparent)]
    Other(#[from] eyre::Report),
}

fn handle_input(
    state: &mut ClientState,
    sender: &Sender<IRCMessage>,
    input: &str,
) -> Result<(), InputErr> {
    // ui.debug(format!("input: {}", input))?;

    match input.split_prefix('/') {
        Some((_, input)) => match Command::parse(input) {
            Ok(cmd) => {
                match cmd.handle(state, sender) {
                    Ok(()) => {}
                    Err(report) => {
                        state.error(report.to_string());
                    }
                }
                // even if the command cannot be handled, that's not a fatal error
                Ok(())
            }
            Err(e) => {
                state.error(format!("failed to parse command: {}", e));
                // failure to parse is never fatal
                Ok(())
            }
        },
        None => {
            let ConnectionState::Connected(ConnectedState { nick, channels, .. }) =
                &mut state.conn_state
            else {
                state.error(String::from("cannot send message when not registered"));
                // this is not a fatal error, it likely means that the connection was slow
                return Ok(());
            };
            let nick = nick.clone();

            if channels.len() == 0 {
                state.error(String::from("cannot send a message to 0 channels"));
            } else if channels.len() > 1 {
                state.error(String::from("multiple channels NYI"));
            } else {
                let targets = channels
                    .iter()
                    .map(|(_, c)| Target::Channel(c.name().to_string()))
                    .collect::<Vec<_>>();
                for target in targets.iter() {
                    let line = Line::default()
                        .push_unstyled("<")
                        .push(nick.to_string().magenta().bold())
                        .push_unstyled(">")
                        .push_unstyled(input);
                    state.add_line(target.clone(), line);
                }
                sender
                    .send(IRCMessage {
                        tags: None,
                        source: None,
                        message: Message::Privmsg {
                            targets,
                            msg: input.to_string(),
                        },
                    })
                    .wrap_err("failed to send privmsg to writer thread")?;
            }

            Ok(())
        }
    }
}
