use core::fmt;
use std::{
    collections::{HashMap, VecDeque},
    sync::mpsc::Sender,
};

use crossterm::style::Stylize;
use log::*;

use crate::{
    channel::{Channel, ChannelName, Nickname, UserMessages},
    irc_message::IrcMessage,
    targets::Target,
    ui::{keybinds::Action, term::TerminalUi, text::Line},
};

pub struct ClientState<'a> {
    addr: String,
    pub ui: TerminalUi<'a>,
    pub conn_state: ConnectionState,
    all_targets: Vec<Target>,
    selected_target_idx: usize,
    status_messages: VecDeque<Line<'static>>,
    pub msg_sender: Sender<IrcMessage>,
}

#[derive(Debug)]
pub struct StatusInfo {
    pub addr: String,
    pub registered: bool,
    pub nick: String,
    pub target: Target,
}

impl<'a> ClientState<'a> {
    const TARGET_STATUS_IDX: usize = 0;

    pub fn new(
        addr: impl ToString,
        msg_sender: Sender<IrcMessage>,
        ui: TerminalUi<'a>,
        requested_nick: String,
    ) -> Self {
        Self {
            addr: addr.to_string(),
            ui,
            conn_state: ConnectionState::Registration(RegistrationState { requested_nick }),
            all_targets: vec![Target::Status],
            selected_target_idx: ClientState::TARGET_STATUS_IDX,
            status_messages: VecDeque::new(),
            msg_sender,
        }
    }

    pub fn add_line(&mut self, target: Target, line: Line<'static>) {
        self.ensure_target_exists(target.clone());
        let ConnectionState::Connected(ConnectedState {
            channels,
            user_messages,
            ..
        }) = &mut self.conn_state
        else {
            return;
        };
        match target {
            Target::Channel(channel_name) => {
                // UNWRAP: `ensure_target_exists` called above
                let channel = channels.get_mut(&channel_name).unwrap();
                channel.messages.push_back(line);
            }
            Target::Nickname(nick) => {
                // UNWRAP: `ensure_target_exists` called above
                let user_messages = user_messages.get_mut(&nick).unwrap();
                user_messages.add_line(line);
            }
            Target::Status => {
                self.status_messages.push_back(line);
            }
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

    pub fn warn_in(&mut self, target: &Target, msg: String) {
        let Some(lines) = self.lines_for(target) else {
            warn!("cannot warn {} in unknown target {:?}", msg, target);
            return;
        };
        warn!("{:?} {}", target, msg);
        let line = Line::default().push("WARN: ".yellow()).push(msg.yellow());
        lines.push_back(line);
        self.render();
    }

    pub fn ensure_target_exists(&mut self, target: Target) {
        match &mut self.conn_state {
            ConnectionState::Registration { .. } => {
                unreachable!("should not be joining a channel when not connected")
            }
            ConnectionState::Connected(ConnectedState {
                channels,
                user_messages,
                ..
            }) => match target {
                Target::Status => {}
                Target::Channel(channel_name) => {
                    if !channels.contains_key(&channel_name) {
                        self.all_targets.push(Target::Channel(channel_name.clone()));
                        self.selected_target_idx = self.all_targets.len() - 1;
                        channels.insert(channel_name.clone(), Channel::from_name(channel_name));
                    }
                }
                Target::Nickname(nick) => {
                    if !user_messages.contains_key(&nick) {
                        self.all_targets.push(Target::Nickname(nick.clone()));
                        self.selected_target_idx = self.all_targets.len() - 1;
                        user_messages.insert(nick.clone(), UserMessages::new(nick));
                    }
                }
            },
        }
    }

    pub fn current_target(&self) -> &Target {
        &self.all_targets[self.selected_target_idx]
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

        let (registered, nick) = match &mut self.conn_state {
            ConnectionState::Registration(RegistrationState { requested_nick }) => {
                (false, requested_nick)
            }
            ConnectionState::Connected(ConnectedState { nick, .. }) => (true, nick),
        };

        let status = StatusInfo {
            addr: self.addr.clone(),
            registered,
            nick: nick.clone(),
            target: self.current_target().clone(),
        };

        match target {
            Target::Status => self.ui.render(&status, self.status_messages.iter()),
            Target::Channel(channel_name) => {
                let ConnectionState::Connected(ConnectedState { channels, .. }) =
                    &mut self.conn_state
                else {
                    // just don't render if not connected
                    return Ok(());
                };

                match channels.get(channel_name) {
                    Some(channel) => {
                        self.ui.render(&status, channel.messages.iter())?;
                    }
                    None => {
                        self.selected_target_idx = ClientState::TARGET_STATUS_IDX;
                        self.ui.render(&status, self.status_messages.iter())?;
                    }
                }

                Ok(())
            }
            Target::Nickname(nick) => {
                let ConnectionState::Connected(ConnectedState { user_messages, .. }) =
                    &mut self.conn_state
                else {
                    // just don't render if not connected
                    return Ok(());
                };

                // UNWRAP: the nick cannot be selected if it's not in the target list
                let msgs = user_messages.get(nick).unwrap();
                self.ui.render(&status, msgs.iter_lines())?;

                Ok(())
            }
        }
    }

    /// collects input from the user, re-rendering if necessary
    pub fn input(&mut self) -> eyre::Result<Option<String>> {
        let Some(action) = self.ui.raw_input()? else {
            return Ok(None);
        };

        match action {
            Action::Resize => {
                self.render()?;
                Ok(None)
            }
            Action::Type(c) => {
                self.ui.input_buffer.insert(c);
                self.render()?;
                Ok(None)
            }
            Action::Enter => {
                let s = self.ui.input_buffer.finish();
                self.render()?;
                Ok(Some(s))
            }
            Action::Backspace => {
                self.ui.input_buffer.backspace();
                self.render()?;
                Ok(None)
            }
            Action::Delete => {
                self.ui.input_buffer.delete();
                self.render()?;
                Ok(None)
            }
            Action::PreviousLine => {
                self.ui.scrollback = self.ui.scrollback.saturating_add(1);
                self.render()?;
                Ok(None)
            }
            Action::NextLine => {
                self.ui.scrollback = self.ui.scrollback.saturating_sub(1);
                self.render()?;
                Ok(None)
            }
            Action::PreviousCharacter => {
                self.ui.input_buffer.offset(-1);
                self.render()?;
                Ok(None)
            }
            Action::NextCharacter => {
                self.ui.input_buffer.offset(1);
                self.render()?;
                Ok(None)
            }
            Action::FirstCharacter => {
                self.ui.input_buffer.select(0);
                self.render()?;
                Ok(None)
            }
            Action::LastCharacter => {
                self.ui.input_buffer.select(self.ui.input_buffer.char_len());
                self.render()?;
                Ok(None)
            }
            Action::PreviousWindow => {
                if self.selected_target_idx > 0 {
                    self.selected_target_idx -= 1;
                } else {
                    self.selected_target_idx = self.all_targets.len() - 1;
                }
                self.render()?;
                Ok(None)
            }
            Action::NextWindow => {
                if self.selected_target_idx < self.all_targets.len() - 1 {
                    self.selected_target_idx += 1;
                } else {
                    self.selected_target_idx = 0
                }
                self.render()?;
                Ok(None)
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
            Target::Channel(channel_name) => {
                if let ConnectionState::Connected(ConnectedState { channels, .. }) =
                    &mut self.conn_state
                {
                    channels.get_mut(channel_name).map(|c| &mut c.messages)
                } else {
                    None
                }
            }
            Target::Nickname(nick) => {
                if let ConnectionState::Connected(ConnectedState { user_messages, .. }) =
                    &mut self.conn_state
                {
                    user_messages.get_mut(nick).map(|c| &mut c.messages)
                } else {
                    None
                }
            }
        }
    }

    fn lines_for(&mut self, target: &Target) -> Option<&mut VecDeque<Line<'static>>> {
        match target {
            Target::Status => Some(&mut self.status_messages),
            Target::Channel(channel_name) => {
                if let ConnectionState::Connected(ConnectedState { channels, .. }) =
                    &mut self.conn_state
                {
                    channels.get_mut(channel_name).map(|c| &mut c.messages)
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
    /// currently connected channels
    pub channels: HashMap<ChannelName, Channel>,
    /// all users with which there exists a private message
    pub user_messages: HashMap<Nickname, UserMessages>,
    pub messages_state: MessagesState,
}

impl ConnectedState {
    pub fn new(nick: String) -> Self {
        Self {
            nick,
            channels: HashMap::new(),
            user_messages: HashMap::new(),
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
