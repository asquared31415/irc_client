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

use crossterm::style::Stylize;
use eyre::{bail, eyre, Context};
use indexmap::IndexSet;
use log::{debug, trace};
use rustls::{pki_types::ServerName, ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use thiserror::Error;

use crate::{
    command::Command,
    ext::*,
    handlers,
    irc_message::{IRCMessage, Message, Param, Source},
    server_io::ServerIo,
    ui::{
        layout::{Direction, Layout, Section, SectionKind},
        term::{InputStatus, TerminalUi},
        text::Line,
    },
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
    let state = Arc::new(Mutex::new(ClientState {
        ui: TerminalUi::new(layout, io::stdout())?,
        conn_state: ConnectionState::Registration(RegistrationState {
            requested_nick: nick.to_string(),
        }),
    }));

    // send to this channel to have a message written to the server
    let (write_sender, write_receiver) = mpsc::channel::<IRCMessage>();
    // recv from this channel to get incoming messages from the server
    let (msg_sender, msg_receiver) = mpsc::channel::<IRCMessage>();

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
                            // debug!("write msg {:#?}", msg);
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
                    let ret = match state.ui.input() {
                        InputStatus::Complete(input) => {
                            handle_input(state, &queue_sender, input.trim())
                        }
                        // incomplete input, loop again
                        InputStatus::Incomplete => Ok(()),
                        InputStatus::Quit => Err(InputErr::Quit),
                        InputStatus::IoErr(e) => Err(InputErr::Io(e)),
                    };
                    ret
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
                    Err(InputErr::Quit) => {
                        QUIT_REQUESTED.store(true, atomic::Ordering::Relaxed);
                        return;
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
            let _ = ui.writeln("exiting");
            ui.disable();
            return Err(ExitReason::Quit);
        }

        let msg = match msg_receiver.try_recv() {
            Ok(msg) => msg,
            Err(_) => continue,
        };

        let state = &mut *state.lock().unwrap();
        if let Err(report) = on_msg(state, msg, &write_sender) {
            // panic!("MEOW");
            let _ = state
                .ui
                .writeln(format!("ERROR unable to handle message: {}", report));
            QUIT_REQUESTED.store(true, atomic::Ordering::Relaxed);
        }
    }
}

fn on_msg(
    state: &mut ClientState,
    msg: IRCMessage,
    sender: &Sender<IRCMessage>,
) -> eyre::Result<()> {
    let ui = &mut state.ui;
    // ui.writeln(
    //     (SystemTime::now().duration_since(SystemTime::UNIX_EPOCH))
    //         .unwrap()
    //         .as_secs()
    //         .to_string(),
    // )?;
    match msg.message {
        // =====================
        // PING
        // =====================
        Message::Ping(token) => sender.send(IRCMessage {
            tags: None,
            source: None,
            message: Message::Pong(token.to_string()),
        })?,

        // =====================
        // ERROR
        // =====================
        Message::Error(reason) => {
            ui.error(reason.as_str())?;
            // technically not a requested quit, but a requested quit exits silently
            QUIT_REQUESTED.store(true, atomic::Ordering::Relaxed);
        }

        // =====================
        // REGISTRATION
        // =====================
        Message::Numeric { num: 1, args } => {
            let ClientState {
                conn_state: ConnectionState::Registration(RegistrationState { requested_nick }),
                ..
            } = state
            else {
                ui.warn("001 when already registered")?;
                return Ok(());
            };

            let [nick, msg, ..] = args.as_slice() else {
                bail!("RPL_001 had no nick and msg arg");
            };
            let (Some(nick), Some(msg)) = (nick.as_str(), msg.as_str()) else {
                bail!("nick must be a string argument");
            };

            if requested_nick != nick {
                ui.writeln(format!(
                    "WARNING: requested nick {}, but got nick {}",
                    requested_nick, nick
                ))?;
            }

            state.conn_state = ConnectionState::Connected(ConnectedState {
                nick: nick.to_string(),
                channels: IndexSet::new(),
            });
            ui.writeln(msg.to_string())?;
        }

        // =====================
        // GREETING
        // =====================
        Message::Numeric { num: 2, args } => {
            let [_, msg, ..] = args.as_slice() else {
                bail!("RPL_YOURHOST missing msg");
            };
            let Some(msg) = msg.as_str() else {
                bail!("RPL_YOURHOST msg not a string");
            };
            ui.writeln(msg.to_string())?;
        }
        Message::Numeric { num: 3, args } => {
            let [_, msg, ..] = args.as_slice() else {
                bail!("RPL_CREATED missing msg");
            };
            let Some(msg) = msg.as_str() else {
                bail!("RPL_CREATED msg not a string");
            };
            ui.writeln(msg.to_string())?;
        }
        Message::Numeric { num: 4, args } => {
            let [_, rest @ ..] = args.as_slice() else {
                bail!("RPL_NUMERIC missing client arg");
            };
            let msg = rest
                .iter()
                .filter_map(Param::as_str)
                .collect::<Vec<&str>>()
                .join(" ");
            ui.writeln(msg)?;
        }
        Message::Numeric { num: 5, args: _ } => {
            //TODO: do we care about this?
        }

        // =====================
        // CHANNEL STATE
        // =====================
        Message::Join(join_channels) => {
            let ClientState {
                conn_state: ConnectionState::Connected(ConnectedState { nick, channels }),
                ..
            } = state
            else {
                bail!("JOIN messages can only be processed when connected to a server");
            };
            let join_channels = join_channels
                .into_iter()
                .map(|(channel, _)| channel)
                .collect::<Vec<_>>();

            // if the source of the join is ourself, update the list of joined channels,
            // otherwise announce a join
            match msg.source.as_ref().map(|source| source.get_name()) {
                Some(source) if source == nick => {
                    for chan in join_channels.iter() {
                        ui.writeln(
                            Line::default()
                                .push("joined ".green())
                                .push(chan.clone().dark_blue()),
                        )?;
                    }
                    channels.extend(join_channels);
                }
                Some(other) => {
                    for chan in join_channels.iter() {
                        ui.writeln(
                            Line::default()
                                .push(other.magenta())
                                .push(" joined ".green())
                                .push(chan.clone().dark_blue()),
                        )?;
                    }
                }
                None => {
                    ui.warn("JOIN msg without a source")?;
                }
            }
        }

        Message::Quit(reason) => {
            let Some(name) = msg.source.as_ref().map(Source::get_name) else {
                bail!("QUIT msg had no source");
            };
            // NOTE: servers SHOULD always send a reason, but make sure
            let reason = reason.unwrap_or(String::from("disconnected"));
            ui.writeln(format!("{} quit: {}", name, reason))?;
        }

        // =====================
        // MESSAGES
        // =====================
        Message::Privmsg { msg: privmsg, .. } => {
            // TODO: check whether target is channel vs user
            write_msg(
                ui,
                msg.source.as_ref(),
                Line::new_without_style(privmsg).unwrap(),
            )?;
        }
        Message::Notice {
            msg: notice_msg, ..
        } => {
            write_msg(
                ui,
                msg.source.as_ref(),
                Line::default()
                    .push("NOTICE ".green())
                    .push_unstyled(notice_msg),
            )?;
        }

        // =====================
        // OTHER NUMERIC REPLIES
        // =====================
        msg @ Message::Numeric { .. } => {
            handlers::numeric::handle(msg, ui)?;
        }

        // =====================
        // UNKNOWN
        // =====================
        unk => {
            ui.warn(format!("unhandled msg {:?}", unk));
        }
    }
    // ui.writeln(
    //     (SystemTime::now().duration_since(SystemTime::UNIX_EPOCH))
    //         .unwrap()
    //         .as_secs()
    //         .to_string(),
    // )?;
    Ok(())
}

#[derive(Debug, Error)]
enum InputErr {
    // client requested a quit
    #[error("client requested a quit")]
    Quit,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Other(#[from] eyre::Report),
}

fn handle_input(
    state: &mut ClientState,
    sender: &Sender<IRCMessage>,
    input: &str,
) -> Result<(), InputErr> {
    let ui = &mut state.ui;
    ui.debug(format!("input: {}", input));
    match state {
        ClientState {
            conn_state: ConnectionState::Registration(..),
            ..
        } => {
            ui.warn("input during registration NYI");
            Ok(())
        }
        ClientState {
            conn_state: ConnectionState::Connected(ConnectedState { nick, channels }),
            ..
        } => {
            if let Some((_, input)) = input.split_prefix('/') {
                let cmd = Command::parse(input)
                    .wrap_err_with(|| format!("failed to parse command {:?}", input))?;
                cmd.handle(state, sender)?;

                Ok(())
            } else {
                if channels.len() == 0 {
                    ui.warn("cannot send a message to 0 channels");
                } else if channels.len() > 1 {
                    ui.warn("multiple channels NYI");
                } else {
                    sender
                        .send(IRCMessage {
                            tags: None,
                            source: None,
                            message: Message::Privmsg {
                                targets: channels.as_slice().iter().cloned().collect(),
                                msg: input.to_string(),
                            },
                        })
                        .wrap_err("failed to send privmsg to writer thread")?;
                    write_msg(
                        ui,
                        Some(&Source::new(nick.to_string())),
                        Line::default().push_unstyled(input),
                    )?;
                }

                Ok(())
            }
        }
    }
}

fn write_msg<'a>(
    ui: &mut TerminalUi<'a>,
    source: Option<&Source>,
    line: Line<'a>,
) -> eyre::Result<()> {
    let mut composed = Line::default();
    if let Some(source) = source {
        composed = composed
            .push_unstyled("<")
            .push(source.to_string().magenta())
            .push_unstyled(">");
    }
    composed.extend(line.into_iter());
    ui.writeln(composed)?;
    Ok(())
}

pub struct ClientState<'a> {
    pub ui: TerminalUi<'a>,
    pub conn_state: ConnectionState,
}

#[derive(Debug)]
pub enum ConnectionState {
    Registration(RegistrationState),
    Connected(ConnectedState),
}

#[derive(Debug)]
pub struct RegistrationState {
    /// the nick that the user requested. the server will respond with the actual nick in the
    /// RPL_WELCOME message.
    pub requested_nick: String,
}

#[derive(Debug)]
pub struct ConnectedState {
    pub nick: String,
    // list of connected channel names. each name includes the prefix.
    pub channels: IndexSet<String>,
}
