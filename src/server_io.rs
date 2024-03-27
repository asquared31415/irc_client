use std::io;

use thiserror::Error;

use crate::{
    ext::{ReadWrite, WriteExt},
    irc_message::{IRCMessage, IrcParseErr, MessageToStringErr},
};

// the size of the receive buffer to allocate, in bytes.
const BUFFER_SIZE: usize = 4096;

#[derive(Debug, Error)]
pub enum MessagePollErr {
    #[error("the connection was closed")]
    Closed,
    #[error("polling was unsuccessful after {} retries", .0)]
    TooManyRetries(u8),
    #[error("server sent invalid UTF-8")]
    InvalidUTF8,
    #[error(transparent)]
    IrcParseErr(#[from] IrcParseErr),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug, Error)]
pub enum MsgWriteErr {
    #[error(transparent)]
    MessageToStrErr(#[from] MessageToStringErr),
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub struct ServerIo {
    connection: Box<dyn ReadWrite + Send>,
    buffer: Box<[u8; BUFFER_SIZE]>,
    message_buffer: String,
}

impl ServerIo {
    pub fn new(connection: Box<dyn ReadWrite + Send>) -> Self {
        Self {
            connection,
            buffer: Box::new([0_u8; BUFFER_SIZE]),
            message_buffer: String::new(),
        }
    }

    pub fn write(&mut self, msg: &IRCMessage) -> Result<(), MsgWriteErr> {
        self.connection
            .write_all_blocking(msg.to_irc_string()?.as_bytes())?;
        Ok(())
    }

    // returns Ok([msg, ...]) if a message was read and Err(e) if an error occurred
    pub fn recv(&mut self) -> Result<Vec<IRCMessage>, MessagePollErr> {
        const MAX_RETRIES: u8 = 5;
        let mut retry_count = 0;
        let count = loop {
            match self.connection.read(&mut *self.buffer) {
                Ok(count) => {
                    // TCP streams return Ok(0) when they have been gracefully closed by the
                    // other side
                    if count == 0 {
                        return Err(MessagePollErr::Closed);
                    }

                    break count;
                }
                Err(e) => {
                    if e.kind() == io::ErrorKind::WouldBlock {
                        return Ok(vec![]);
                    } else if e.kind() != io::ErrorKind::Interrupted {
                        return Err(e.into());
                    } else if retry_count > MAX_RETRIES {
                        return Err(MessagePollErr::TooManyRetries(retry_count));
                    } else {
                        // retry on Interrupted
                        retry_count += 1;
                        continue;
                    }
                }
            }
        };

        let Ok(s) = core::str::from_utf8(&self.buffer[..count]) else {
            return Err(MessagePollErr::InvalidUTF8);
        };
        self.message_buffer.push_str(s);

        let mut msgs = Vec::new();
        // parse out the messages from the buffer
        while let Some(idx) = self.message_buffer.find("\r\n") {
            let (msg_str, rest) = self.message_buffer.split_at(idx);
            let msg_str = msg_str.to_string();
            // remove the CRLF from the text
            self.message_buffer = rest[2..].to_string();

            // clients should ignore 0 length messages
            if msg_str.len() == 0 {
                continue;
            }

            let msg = IRCMessage::parse(msg_str.as_str())?;
            msgs.push(msg);
        }

        return Ok(msgs);
    }
}
