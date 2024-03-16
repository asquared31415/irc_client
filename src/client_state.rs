use std::{
    io::Write,
    net::TcpStream,
    sync::{mpsc, mpsc::Sender, Arc, Mutex},
    thread,
};

use color_eyre::eyre::Result;
use eyre::eyre;
use log::*;
use owo_colors::OwoColorize;

use crate::{
    command::Command,
    ext::*,
    irc_message::{IRCMessage, Message},
    reader::IrcMessageReader,
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
            move || -> Result<!> {
                loop {
                    let msg = queue_receiver.recv()?;
                    let s = msg.to_irc_string();
                    trace!("sending message: {:#?}: {:?}", msg, s);
                    writer.write_all(s.as_bytes())?;
                    writer.flush()?;
                }
            }
        });

        let _ = thread::spawn({
            let reader = self.stream.try_clone()?;
            move || -> Result<!> {
                let mut msg_reader = IrcMessageReader::new(reader);

                loop {
                    trace!("reading");
                    let msg = msg_reader.recv()?;
                    trace!("got msg {:#?}", msg);
                    msg_sender.send(msg)?;
                }
            }
        });

        // user interaction using stdin
        let _ = thread::spawn({
            let state = Arc::clone(&self.state);
            let ui = Arc::clone(&self.ui);
            move || -> Result<!> {
                loop {
                    let input = ui.read()?;
                    Client::handle_input(&mut *state.lock().unwrap(), input.trim());
                }
            }
        });

        // call the init function that controls how to register
        init(&queue_sender)?;

        // main code that processes state as messages come in
        loop {
            let msg = msg_receiver.recv()?;
            self.on_msg(msg, &queue_sender)?;
        }
        // only terminates on error
        // TODO: have some way for the ui to request termination
    }

    fn on_msg(&mut self, msg: IRCMessage, sender: &Sender<IRCMessage>) -> Result<()> {
        let state = &mut *self.state.try_lock().map_err(|_| eyre!(""))?;
        match state {
            ClientState::Registration(RegistrationState { requested_nick }) => {
                match msg.message {
                    Message::Ping(token) => {
                        // TODO: when you do this state better, read the real nick from 001 and
                        // compare/notify user
                        *state = ClientState::Connected(ConnectedState {
                            nick: requested_nick.to_string(),
                        });

                        sender.send(IRCMessage {
                            tags: None,
                            source: None,
                            message: Message::Pong(token),
                        })?;
                    }
                    Message::Notice { targets: _, msg } => {
                        self.ui.writeln(format!("{} {}", "NOTICE".yellow(), msg))?;
                    }
                    _ => {
                        warn!("unexpected message in registration state {:#?}", msg);
                    }
                }
            }
            ClientState::Connected(_) => {}
        }

        Ok(())
    }

    fn handle_input(state: &mut ClientState, input: &str) {
        debug!("{:#?}", state);
        if let Some((_, input)) = input.split_first_matches('/') {
            let cmd = Command::parse(input);
            debug!("cmd {:?}", cmd);
        } else {
            debug!("not a /: {:?}", input);
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
    nick: String,
}
