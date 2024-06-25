use thiserror::Error;

use crate::irc::{client::command::ClientIrcCommand, Tags};

#[derive(Debug)]
/// an IRC message sent by the client. does not have a source.
pub struct ClientMessage {
    tags: Tags,
    cmd: ClientIrcCommand,
}

impl ClientMessage {
    pub fn from_command(cmd: ClientIrcCommand) -> Self {
        Self {
            tags: Tags::empty(),
            cmd,
        }
    }

    pub fn irc_str(&self) -> Result<String, ClientMessageToStringErr> {
        let mut s = String::new();
        // FIXME: send tags
        s.push_str(self.cmd.irc_str()?.as_str());
        s.push_str("\r\n");
        Ok(s)
    }
}

#[derive(Debug, Error)]
pub enum ClientMessageToStringErr {
    #[error("message had invalid params")]
    InvalidParams,
}
