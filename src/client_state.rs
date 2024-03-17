use std::{
    io::Write,
    net::TcpStream,
    sync::{mpsc, mpsc::Sender, Arc, Mutex},
    thread,
};

use color_eyre::eyre::Result;
use eyre::{bail, eyre};
use indexmap::IndexSet;
use log::*;
use owo_colors::OwoColorize;

use crate::{
    command::Command,
    ext::*,
    irc_message::{IRCMessage, Message},
    reader::{IrcMessageReader, MessagePollErr},
    ui::TerminalUi,
};

pub struct Client {
    stream: TcpStream,
    state: Arc<Mutex<ClientState>>,
    ui: Arc<TerminalUi>,
}

impl Client {
    pub fn new(addr: &str, nick: &str) -> Result<Self> {
        let stream = TcpStream::connect(addr)?;

        let state = ClientState::Registration(RegistrationState {
            requested_nick: nick.to_string(),
        });
        Ok(Self {
            stream,
            state: Arc::new(Mutex::new(state)),
            ui: Arc::new(TerminalUi::new()),
        })
    }

    /// spawns threads for the reading and writing parts of the client and begins processing the
    /// connection.
    pub fn start(mut self, init: impl Fn(&Sender<IRCMessage>) -> Result<()>) -> Result<!> {
        let (queue_sender, queue_receiver) = mpsc::channel::<IRCMessage>();
        let (msg_sender, msg_receiver) = mpsc::channel::<IRCMessage>();

        let _ = thread::spawn({
            let mut writer = self.stream.try_clone()?;
            move || {
                let mut inner = || -> eyre::Result<()> {
                    let msg = queue_receiver.recv()?;
                    let s = msg.to_irc_string();
                    trace!("sending message: {:#?}: {:?}", msg, s);
                    writer.write_all(s?.as_bytes())?;
                    writer.flush()?;
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

        let _ = thread::spawn({
            let reader = self.stream.try_clone()?;
            move || {
                let mut msg_reader = IrcMessageReader::new(reader);

                let mut inner = || -> eyre::Result<()> {
                    trace!("reading");
                    match msg_reader.recv() {
                        Ok(msg) => {
                            debug!("got msg {:#?}", msg);
                            msg_sender.send(msg)?;
                            Ok(())
                        }
                        // IRC parsing errors can just be reported without exiting
                        Err(MessagePollErr::IrcParseErr(e)) => {
                            error!("{}", e);
                            Ok(())
                        }
                        // pass fatal errors up to be reported
                        Err(e) => Err(e)?,
                    }
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

        // user interaction using stdin
        let _ = thread::spawn({
            let state = Arc::clone(&self.state);
            let queue_sender = queue_sender.clone();
            let ui = Arc::clone(&self.ui);
            move || {
                let inner = || -> eyre::Result<()> {
                    let input = ui.read()?;
                    Client::handle_input(&mut *state.lock().unwrap(), &queue_sender, input.trim())?;
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
        init(&queue_sender)?;

        // main code that processes state as messages come in
        loop {
            let msg = msg_receiver.recv()?;
            trace!("handling msg");
            self.on_msg(msg, &queue_sender)?;
        }
        // only terminates on error
        // TODO: have some way for the ui to request termination
    }

    fn on_msg(&mut self, msg: IRCMessage, sender: &Sender<IRCMessage>) -> Result<()> {
        let state = &mut *self.state.try_lock().map_err(|_| eyre!(""))?;
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
                    self.ui.writeln(format!(
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
                self.ui.writeln("CONNECTED".bright_green().to_string())?;
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
                            self.ui
                                .writeln(format!("{} {}", "JOINED".bright_blue(), chan))?;
                        }
                        channels.extend(join_channels);
                    }
                    Some(other) => {
                        for chan in join_channels.iter() {
                            self.ui.writeln(format!(
                                "{} joined {}",
                                other.bright_purple(),
                                chan
                            ))?;
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
                self.ui.writeln(format!(
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
            } => self.ui.writeln(format!(
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

    fn handle_input(
        state: &mut ClientState,
        sender: &Sender<IRCMessage>,
        input: &str,
    ) -> Result<()> {
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
