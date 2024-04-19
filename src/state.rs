use core::fmt;
use std::{collections::HashMap, sync::mpsc::Sender};

use crate::{
    channel::channel::Channel,
    irc_message::IRCMessage,
    ui::term::{TerminalUi, UiMsg},
    util::Target,
};

pub struct ClientState<'a> {
    pub ui: TerminalUi<'a>,
    pub target_messages: HashMap<Target, Vec<IRCMessage>>,
    pub conn_state: ConnectionState,
    pub msg_sender: Sender<IRCMessage>,
    pub ui_sender: Sender<UiMsg<'a>>,
}

impl<'a> ClientState<'a> {
    pub fn new(
        msg_sender: Sender<IRCMessage>,
        ui_sender: Sender<UiMsg<'a>>,
        ui: TerminalUi<'a>,
        requested_nick: String,
    ) -> Self {
        Self {
            ui,
            target_messages: HashMap::new(),
            conn_state: ConnectionState::Registration(RegistrationState { requested_nick }),
            msg_sender,
            ui_sender,
        }
    }
}

impl<'a> fmt::Debug for ClientState<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            f.debug_struct("ClientState")
                .field("target_messages", &self.target_messages)
                .field("conn_state", &self.conn_state)
                .finish_non_exhaustive()
        } else {
            f.debug_struct("ClientState")
                .field("conn_state", &self.conn_state)
                .finish_non_exhaustive()
        }
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
    pub channels: Vec<Channel>,
    pub messages_state: MessagesState,
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
