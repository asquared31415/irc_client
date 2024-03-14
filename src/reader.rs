use std::{io, io::Read, net::TcpStream};

use thiserror::Error;

use crate::irc_message::{IRCMessage, IrcParseErr};

// the size of the receive buffer to allocate, in bytes.
const BUFFER_SIZE: usize = 4096;

pub struct IrcMessageReader {
    connection: TcpStream,
    buffer: Box<[u8; BUFFER_SIZE]>,
    /// if a message is truncated by the buffer, place the beginning of the message here to be
    /// concatenated with the end.
    message_fragment: Option<String>,
}

impl IrcMessageReader {
    pub fn new(connection: TcpStream) -> Self {
        Self {
            connection,
            buffer: Box::new([0_u8; BUFFER_SIZE]),
            message_fragment: None,
        }
    }

    pub fn recv(&mut self) -> Result<IRCMessage, MessagePollErr> {
        // read from the stream until there's a full message
        loop {
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
                        if e.kind() != io::ErrorKind::Interrupted {
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

            // parse out the messages from the stream
            let mut remaining = s;
            loop {
                match remaining.find("\r\n") {
                    Some(idx) => {
                        let (msg_str, rest) = remaining.split_at(idx);
                        // remove the CRLF from the remaining text
                        remaining = &rest[2..];

                        let mut msg_str = msg_str.to_string();
                        // if we found the end of a message, prepend the existing fragment to the
                        // start
                        if let Some(fragment) = self.message_fragment.take() {
                            msg_str.insert_str(0, fragment.as_str());
                        }

                        // clients should ignore 0 length messages
                        if msg_str.len() == 0 {
                            continue;
                        }

                        let msg = IRCMessage::parse(msg_str.as_str())?;
                        return Ok(msg);
                    }
                    // there was not a CRLF, add it to the buffer
                    None => {
                        self.message_fragment
                            .get_or_insert(String::new())
                            .push_str(remaining);
                        // no more can be parsed out of this message
                        break;
                    }
                }
            }
        }
    }
}

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
