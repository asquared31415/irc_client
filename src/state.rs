use core::fmt;
use std::{
    collections::{HashMap, VecDeque},
    sync::mpsc::Sender,
};

use crossterm::style::Stylize;
use log::*;

use crate::{
    channel::channel::Channel,
    irc_message::IRCMessage,
    ui::{term::TerminalUi, text::Line},
    util::Target,
};

pub struct ClientState<'a> {
    pub ui: TerminalUi<'a>,
    pub conn_state: ConnectionState,
    all_targets: Vec<Target>,
    selected_target_idx: usize,
    status_messages: VecDeque<Line<'static>>,
    pub msg_sender: Sender<IRCMessage>,
}

impl<'a> ClientState<'a> {
    const TARGET_STATUS_IDX: usize = 0;

    pub fn new(msg_sender: Sender<IRCMessage>, ui: TerminalUi<'a>, requested_nick: String) -> Self {
        Self {
            ui,
            conn_state: ConnectionState::Registration(RegistrationState { requested_nick }),
            all_targets: vec![Target::Status],
            selected_target_idx: ClientState::TARGET_STATUS_IDX,
            status_messages: VecDeque::new(),
            msg_sender,
        }
    }

    pub fn add_line(&mut self, target: Target, line: Line<'static>) {
        let ConnectionState::Connected(ConnectedState { channels, .. }) = &mut self.conn_state
        else {
            return;
        };
        if target == Target::Status {
            self.status_messages.push_back(line);
        } else {
            let Some(channel) = channels.get_mut(&target) else {
                warn!("could not add line to missing channel {:?}", target);
                return;
            };

            channel.messages.push_back(line);
        }

        self.render();
    }

    /// report a non-fatal error to the current target window.
    /// this should be used for things like UI, as opposed to parsing.
    pub fn error(&mut self, error: String) {
        error!("{}", error);
        let Some(lines) = self.current_lines() else {
            return;
        };
        let line = Line::default().push("ERROR: ".red()).push(error.red());
        lines.push_back(line);
        self.render();
    }

    pub fn warn(&mut self, msg: String) {
        warn!("{}", msg);
        let Some(lines) = self.current_lines() else {
            return;
        };
        let line = Line::default().push("WARN: ".yellow()).push(msg.yellow());
        lines.push_back(line);
        self.render();
    }

    pub fn join_channel(&mut self, channel: Channel) {
        match &mut self.conn_state {
            ConnectionState::Registration(_) => todo!(),
            ConnectionState::Connected(ConnectedState { channels, .. }) => {
                let target = channel.target();
                if channels.contains_key(&target) {
                    self.warn(format!("already joined channel {}", channel.name()));
                } else {
                    self.all_targets.push(target.clone());
                    self.selected_target_idx = self.all_targets.len() - 1;
                    channels.insert(target, channel);
                }
            }
        }
    }

    pub fn render(&mut self) -> eyre::Result<()> {
        let target = match self.all_targets.get(self.selected_target_idx) {
            Some(target) => target,
            None => {
                self.selected_target_idx = ClientState::TARGET_STATUS_IDX;
                &Target::Status
            }
        };
        trace!("rendering for {:?}", target);
        if target == &Target::Status {
            self.ui.render(self.status_messages.iter())?;
            return Ok(());
        }

        match &mut self.conn_state {
            ConnectionState::Registration(_) => Ok(()),
            ConnectionState::Connected(ConnectedState { channels, .. }) => {
                match channels.get(target) {
                    Some(channel) => {
                        self.ui.render(channel.messages.iter())?;
                    }
                    None => {
                        self.selected_target_idx = ClientState::TARGET_STATUS_IDX;
                        // render the status page
                        self.ui.render(self.status_messages.iter())?;
                    }
                }

                Ok(())
            }
        }
    }

    fn current_lines(&mut self) -> Option<&mut VecDeque<Line<'static>>> {
        let target = match self.all_targets.get(self.selected_target_idx) {
            Some(target) => target,
            None => {
                self.selected_target_idx = ClientState::TARGET_STATUS_IDX;
                &Target::Status
            }
        };
        match target {
            Target::Status => Some(&mut self.status_messages),
            Target::Channel(_) => {
                if let ConnectionState::Connected(ConnectedState { channels, .. }) =
                    &mut self.conn_state
                {
                    channels.get_mut(target).map(|c| &mut c.messages)
                } else {
                    None
                }
            }
            Target::Nickname(_) => todo!(),
        }
    }
}

impl<'a> fmt::Debug for ClientState<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientState")
            .field("conn_state", &self.conn_state)
            .finish_non_exhaustive()
    }
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
    // currently connected channels
    pub channels: HashMap<Target, Channel>,
    pub messages_state: MessagesState,
}

impl ConnectedState {
    pub fn new(nick: String) -> Self {
        Self {
            nick,
            channels: HashMap::new(),
            messages_state: MessagesState {
                active_names: HashMap::new(),
            },
        }
    }
}

/// state for messages that are in-flight or handled across multiple messages
#[derive(Debug)]
pub struct MessagesState {
    // a list of channels with active NAMES replies
    pub active_names: HashMap<String, NamesState>,
}

#[derive(Debug)]
pub struct NamesState {
    pub names: Vec<String>,
}
