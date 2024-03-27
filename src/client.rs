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

use eyre::{bail, eyre, Context};
use indexmap::IndexSet;
use ratatui::backend::{Backend, CrosstermBackend};
use rustls::{pki_types::ServerName, ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use thiserror::Error;

use crate::{
    command::Command,
    ext::*,
    irc_message::{IRCMessage, Message, Param, Source},
    server_io::ServerIo,
    ui::{InputStatus, TerminalUi},
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

static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

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

    let state = Arc::new(Mutex::new(ClientState::Registration(RegistrationState {
        requested_nick: nick.to_string(),
    })));
    let ui = Arc::new(Mutex::new(TerminalUi::new(CrosstermBackend::new(
        io::stdout(),
    ))?));

    // send to this channel to have a message written to the server
    let (write_sender, write_receiver) = mpsc::channel::<IRCMessage>();
    // recv from this channel to get incoming messages from the server
    let (msg_sender, msg_receiver) = mpsc::channel::<IRCMessage>();

    // stream reader and writer thread
    // moves the stream into the thread
    let _ = thread::spawn({
        let ui = Arc::clone(&ui);
        move || {
            let mut connection = ServerIo::new(stream);
            let mut inner = || -> eyre::Result<()> {
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
                            // debug!("recv msg {:#?}", msg);
                            msg_sender.send(msg)?;
                        }
                    }
                    Err(e) => Err(e)?,
                }

                Ok(())
            };

            loop {
                if QUIT_REQUESTED.load(atomic::Ordering::Relaxed) {
                    return;
                }

                // if an error is encountered, log them and exit the thread
                match inner() {
                    Ok(()) => {}
                    Err(e) => {
                        ui.lock().unwrap().error(e.to_string());
                        return;
                    }
                }
            }
        }
    });

    // user interaction using stdin
    let _ = thread::spawn({
        const INPUT_POLL_DELAY: Duration = Duration::from_millis(5);
        let state = Arc::clone(&state);
        let queue_sender = write_sender.clone();
        let ui = Arc::clone(&ui);
        move || {
            #[derive(Debug)]
            enum UiErr {
                Quit,
                Io(io::Error),
                Other(eyre::Report),
            }

            let inner = || -> Result<(), UiErr> {
                let input = match ui.lock().unwrap().input() {
                    InputStatus::Complete(input) => input,
                    // incomplete input, loop again
                    InputStatus::Incomplete => return Ok(()),
                    InputStatus::Quit => return Err(UiErr::Quit),
                    InputStatus::IoErr(e) => return Err(UiErr::Io(e)),
                };
                handle_input(
                    &mut *state.lock().unwrap(),
                    &mut *ui.lock().unwrap(),
                    &queue_sender,
                    input.trim(),
                )
                .map_err(|r| UiErr::Other(r))?;
                Ok(())
            };

            loop {
                // if an error is encountered, log them and exit the thread
                match inner() {
                    Ok(()) => thread::sleep(INPUT_POLL_DELAY),
                    Err(UiErr::Quit) => {
                        QUIT_REQUESTED.store(true, atomic::Ordering::Relaxed);
                        return;
                    }
                    Err(UiErr::Io(e)) => {
                        ui.lock().unwrap().error(e.to_string());
                        return;
                    }
                    Err(UiErr::Other(e)) => {
                        ui.lock().unwrap().error(e.to_string());
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
            let mut ui = ui.lock().unwrap();
            ui.writeln("exiting")?;
            ui.disable();
            return Err(ExitReason::Quit);
        }

        let msg = match msg_receiver.try_recv() {
            Ok(msg) => msg,
            Err(_) => continue,
        };
        on_msg(
            &mut *state.lock().unwrap(),
            &mut *ui.lock().unwrap(),
            msg,
            &write_sender,
        )?;
    }
    // only terminates on error
    // TODO: have some way for the ui to request termination
}

fn on_msg<B: Backend + io::Write>(
    state: &mut ClientState,
    ui: &mut TerminalUi<B>,
    msg: IRCMessage,
    sender: &Sender<IRCMessage>,
) -> eyre::Result<()> {
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
            ui.error(reason.as_str());
            // technically not a requested quit, but a requested quit exits silently
            QUIT_REQUESTED.store(true, atomic::Ordering::Relaxed);
        }

        // =====================
        // REGISTRATION
        // =====================
        Message::Numeric { num: 1, args } => {
            let ClientState::Registration(RegistrationState { requested_nick }) = state else {
                ui.warn("001 when already registered");
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

            *state = ClientState::Connected(ConnectedState {
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
            let ClientState::Connected(ConnectedState { nick, channels }) = state else {
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
                        ui.writeln(format!("JOINED {}", chan))?;
                    }
                    channels.extend(join_channels);
                }
                Some(other) => {
                    for chan in join_channels.iter() {
                        ui.writeln(format!("{} joined {}", other, chan))?;
                    }
                }
                None => {
                    ui.warn("JOIN msg without a source");
                }
            }
        }

        // =====================
        // MESSAGES
        // =====================
        Message::Privmsg {
            msg: notice_msg, ..
        } => {
            // TODO: check whether target is channel vs user
            write_msg(ui, msg.source.as_ref(), notice_msg.as_str())?;
        }
        Message::Notice {
            msg: notice_msg, ..
        } => {
            write_msg(
                ui,
                msg.source.as_ref(),
                format!("NOTICE {}", notice_msg).as_str(),
            )?;
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

fn handle_input<B: Backend + io::Write>(
    state: &mut ClientState,
    ui: &mut TerminalUi<B>,
    sender: &Sender<IRCMessage>,
    input: &str,
) -> eyre::Result<()> {
    ui.debug(format!("input: {}", input));
    match state {
        ClientState::Registration(_) => {
            ui.warn("input during registration NYI");
            Ok(())
        }
        ClientState::Connected(ConnectedState { nick, channels, .. }) => {
            if let Some((_, input)) = input.split_prefix('/') {
                let cmd = Command::parse(input)?;
                cmd.handle(state, sender)?;

                Ok(())
            } else {
                if channels.len() == 0 {
                    ui.warn("cannot send a message to 0 channels");
                } else if channels.len() > 1 {
                    ui.warn("multiple channels NYI");
                } else {
                    sender.send(IRCMessage {
                        tags: None,
                        source: None,
                        message: Message::Privmsg {
                            targets: channels.as_slice().iter().cloned().collect(),
                            msg: input.to_string(),
                        },
                    })?;
                    write_msg(ui, Some(&Source::new(nick.to_string())), input)?;
                }

                Ok(())
            }
        }
    }
}

fn write_msg<B: Backend + io::Write>(
    ui: &mut TerminalUi<B>,
    source: Option<&Source>,
    msg: &str,
) -> eyre::Result<()> {
    ui.writeln(format!(
        "{}{}",
        source
            .as_ref()
            .map(|source| format!("<{}> ", source))
            .unwrap_or_default(),
        msg
    ))?;

    Ok(())
}

#[derive(Debug)]
pub enum ClientState {
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
