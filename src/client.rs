use std::{
    net::TcpStream,
    sync::{
        mpsc,
        mpsc::{Sender, TryRecvError},
        Arc, Mutex,
    },
    thread,
};

use color_eyre::eyre::Result;
use eyre::{bail, eyre};
use indexmap::IndexSet;
use log::*;
use owo_colors::OwoColorize;
use rustls::{pki_types::ServerName, ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use crate::{
    command::Command,
    ext::*,
    irc_message::{IRCMessage, Message},
    server_io::ServerIo,
    ui::TerminalUi,
};

/// spawns threads for the reading and writing parts of the client and begins processing the
/// connection.
pub fn start(
    addr: &str,
    nick: &str,
    tls: bool,
    init: impl Fn(&Sender<IRCMessage>) -> Result<()>,
) -> Result<!> {
    let Some((name, _)) = addr.split_once(':') else {
        bail!("unable to determine host name for TLS");
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
        let server_name = ServerName::try_from(name.to_string())?;
        let client = ClientConnection::new(config, server_name)?;
        Box::new(StreamOwned::new(client, stream))
    } else {
        Box::new(stream)
    };

    // send to this channel to have a message written to the server
    let (write_sender, write_receiver) = mpsc::channel::<IRCMessage>();
    // recv from this channel to get incoming messages from the server
    let (msg_sender, msg_receiver) = mpsc::channel::<IRCMessage>();

    // stream reader and writer thread
    // moves the stream into the thread
    let _ = thread::spawn({
        move || {
            let mut connection = ServerIo::new(stream);
            let mut inner = || -> eyre::Result<()> {
                // write any necessary messages
                match write_receiver.try_recv() {
                    Ok(msg) => {
                        debug!("write msg {:#?}", msg);
                        connection.write(&msg)?;
                    }
                    // if empty, move on to try to read
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        bail!("connection writer channel disconnected");
                    }
                }

                match connection.recv() {
                    Ok(Some(msg)) => {
                        debug!("recv msg {:#?}", msg);
                        msg_sender.send(msg)?;
                    }
                    // ignore no message found
                    Ok(None) => {}
                    Err(e) => Err(e)?,
                }

                Ok(())
            };

            loop {
                // if an error is encountered, log them and exit the thread
                match inner() {
                    Ok(()) => {}
                    Err(e) => {
                        error!("{}", e);
                        return;
                    }
                }
            }
        }
    });

    let state = Arc::new(Mutex::new(ClientState::Registration(RegistrationState {
        requested_nick: nick.to_string(),
    })));
    let ui = Arc::new(TerminalUi::new());

    // user interaction using stdin
    let _ = thread::spawn({
        let state = Arc::clone(&state);
        let queue_sender = write_sender.clone();
        let ui = Arc::clone(&ui);
        move || {
            let inner = || -> eyre::Result<()> {
                let input = ui.read()?;
                handle_input(&mut *state.lock().unwrap(), &queue_sender, input.trim())?;
                Ok(())
            };

            loop {
                // if an error is encountered, log them and exit the thread
                match inner() {
                    Ok(()) => {}
                    Err(e) => {
                        error!("{}", e);
                        return;
                    }
                }
            }
        }
    });

    // call the init function that controls how to register
    init(&write_sender)?;

    // main code that processes state as messages come in
    loop {
        let msg = msg_receiver.recv()?;
        trace!("handling msg");
        let state = &mut *state
            .try_lock()
            .map_err(|_| eyre!("failed to lock state"))?;
        on_msg(state, &ui, msg, &write_sender)?;
    }
    // only terminates on error
    // TODO: have some way for the ui to request termination
}

fn on_msg(
    state: &mut ClientState,
    ui: &TerminalUi,
    msg: IRCMessage,
    sender: &Sender<IRCMessage>,
) -> Result<()> {
    trace!("state on msg: {:#?}", state);
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
        // REGISTRATION
        // =====================
        Message::Numeric { num: 1, args } => {
            let ClientState::Registration(RegistrationState { requested_nick }) = state else {
                warn!("got a 001 when already registered");
                return Ok(());
            };

            let [nick, ..] = args.as_slice() else {
                bail!("RPL_001 had no nick arg");
            };
            let Some(nick) = nick.as_str() else {
                bail!("nick must be a string argument");
            };

            if requested_nick != nick {
                ui.writeln(format!(
                    "{}: requested nick {}, but got nick {}",
                    "WARNING".bright_yellow(),
                    requested_nick,
                    nick
                ))?;
            }

            *state = ClientState::Connected(ConnectedState {
                nick: nick.to_string(),
                channels: IndexSet::new(),
            });
            ui.writeln("CONNECTED".bright_green().to_string())?;
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
                        ui.writeln(format!("{} {}", "JOINED".bright_blue(), chan))?;
                    }
                    channels.extend(join_channels);
                }
                Some(other) => {
                    for chan in join_channels.iter() {
                        ui.writeln(format!("{} joined {}", other.bright_purple(), chan))?;
                    }
                }
                None => {
                    warn!("JOIN msg without a source");
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
            ui.writeln(format!(
                "{}{}",
                msg.source
                    .as_ref()
                    .map(|source| format!("<{}> ", source.green()))
                    .unwrap_or_default(),
                notice_msg
            ))?
        }
        Message::Notice {
            msg: notice_msg, ..
        } => ui.writeln(format!(
            "{}{} {}",
            msg.source
                .as_ref()
                .map(|source| format!("{} ", source.green()))
                .unwrap_or_default(),
            "NOTICE".bright_yellow(),
            notice_msg
        ))?,

        // =====================
        // UNKNOWN
        // =====================
        unk => {
            warn!("unhandled msg {:?}", unk);
        }
    }
    trace!("state after msg: {:#?}", state);
    Ok(())
}

fn handle_input(state: &mut ClientState, sender: &Sender<IRCMessage>, input: &str) -> Result<()> {
    match state {
        ClientState::Registration(_) => {
            warn!("input during registration NYI");
            Ok(())
        }
        ClientState::Connected(ConnectedState { channels, .. }) => {
            if let Some((_, input)) = input.split_prefix('/') {
                let cmd = Command::parse(input)?;
                trace!("cmd {:?}", cmd);
                cmd.handle(state, sender)?;

                Ok(())
            } else {
                trace!("not a /: {:?}", input);
                trace!("channels: {:?}", channels);
                if channels.len() == 0 {
                    warn!("cannot send a message to 0 channels");
                } else if channels.len() > 1 {
                    warn!("multiple channels NYI");
                } else {
                    sender.send(IRCMessage {
                        tags: None,
                        source: None,
                        message: Message::Privmsg {
                            targets: channels.as_slice().iter().cloned().collect(),
                            msg: input.to_string(),
                        },
                    })?;
                }

                Ok(())
            }
        }
    }
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
