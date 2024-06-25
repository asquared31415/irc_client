use core::str::Utf8Chunks;
use std::io;

use log::debug;
use thiserror::Error;

use crate::{
    ext::{ReadWrite, WriteExt},
    irc::{
        client::{ClientMessage, ClientMessageToStringErr},
        IrcMessage, IrcParseErr,
    },
};

// the size of the receive buffer to allocate, in bytes.
const BUFFER_SIZE: usize = 16 * 1024;

#[derive(Debug, Error)]
pub enum MessagePollErr {
    #[error("the connection was closed")]
    Closed,
    #[error("polling was unsuccessful after {} retries", .0)]
    TooManyRetries(u8),
    #[error(transparent)]
    IrcParseErr(#[from] IrcParseErr),
    #[error(transparent)]
    Io(#[from] io::Error),
}

#[derive(Debug, Error)]
pub enum MsgWriteErr {
    #[error(transparent)]
    MessageToStrErr(#[from] ClientMessageToStringErr),
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub struct ServerIo {
    connection: Box<dyn ReadWrite + Send>,
    buffer: Box<[u8; BUFFER_SIZE]>,
    message_buffer: String,
    // sometimes, a UTF8 character can be split across the end of a buffer during a receive call.
    // when this happens, the remaining bytes of the character are copied to the start of the
    // buffer, and this value updated to reflect the index into the buffer that the next recv call
    // should start receiving at
    recv_idx: u8,
}

impl ServerIo {
    pub fn new(connection: Box<dyn ReadWrite + Send>) -> Self {
        Self {
            connection,
            buffer: Box::new([0_u8; BUFFER_SIZE]),
            message_buffer: String::new(),
            recv_idx: 0,
        }
    }

    pub fn write(&mut self, msg: &ClientMessage) -> Result<(), MsgWriteErr> {
        let msg = msg.irc_str()?;
        // remove the \r\n when writing to the log file
        debug!("<- {:?}", &msg[..(msg.len() - 2)]);
        self.connection.write_all_blocking(msg.as_bytes())?;
        Ok(())
    }

    // returns Ok([msg, ...]) if a message was read and Err(e) if an error occurred
    pub fn recv(&mut self) -> Result<Vec<IrcMessage>, MessagePollErr> {
        const MAX_RETRIES: u8 = 5;
        let mut retry_count = 0;
        let count = loop {
            match self
                .connection
                .read(&mut self.buffer[usize::from(self.recv_idx)..])
            {
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

        let (decoded, invalid) = from_utf8_lossy_split(&self.buffer[..count]);
        let invalid_len = invalid.len();
        debug_assert!(invalid_len < 3);
        if invalid_len == 0 {
            self.message_buffer.push_str(decoded.as_str());
            self.recv_idx = 0;
        } else {
            self.message_buffer.push_str(decoded.as_str());

            // copy the truncated bytes to the start of the buffer
            let end = invalid.to_vec();
            self.buffer[..invalid_len].copy_from_slice(end.as_slice());
            self.recv_idx = invalid_len as u8;
        }

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

            let msg = IrcMessage::parse(msg_str.as_str())?;
            msgs.push(msg);
            debug!("-> {:?}", msg_str);
        }

        return Ok(msgs);
    }
}

// decodes a byte slice into a UTF8 string, but if the end of the slice is not valid UTF8,
// keep it to prepend to the next recv.
fn from_utf8_lossy_split<'slice>(b: &'slice [u8]) -> (String, &'slice [u8]) {
    let mut s = String::new();
    let mut chunks = b.utf8_chunks().peekable();
    while let Some(chunk) = chunks.next() {
        s.push_str(chunk.valid());
        // if there's invalid data, either replace it if it's not at the end of the chunk,
        // or split it if it is
        if chunk.invalid().len() > 0 {
            if chunks.peek().is_some() {
                // UTF8 replacement char
                s.push_str("\u{FFFD}");
            } else {
                // error due to end of input
                return (s, chunk.invalid());
            }
        }
    }

    // no end of input errors were found
    (s, [].as_slice())
}
