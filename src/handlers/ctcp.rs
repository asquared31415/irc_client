use core::fmt::Display;

use log::*;

use crate::{
    channel::Nickname,
    irc_message::{IrcMessage, Message},
    targets::Target,
};

pub const CTCP_DELIM: u8 = 0x01;

const CTCP_DELIM_STR: &str = "\u{0001}";
// KEEP THIS IN SYNC WITH THE ENUM BELOW
const IMPLEMENTED_CTCP: &[&str] = ["ACTION", "CLIENTINFO"].as_slice();

#[derive(Debug)]
pub enum CtcpCommand {
    Action(String),
    Clientinfo,
}

impl CtcpCommand {
    pub fn to_msg(self, targets: Vec<Target>) -> IrcMessage {
        IrcMessage {
            tags: None,
            source: None,
            message: Message::Privmsg {
                targets,
                msg: self.irc_string(),
            },
        }
    }

    fn irc_string(&self) -> String {
        let inner = match self {
            CtcpCommand::Action(action) => format!("ACTION {}", action),
            CtcpCommand::Clientinfo => String::from("CLIENTINFO"),
        };

        format!("{}{}{}", CTCP_DELIM_STR, inner, CTCP_DELIM_STR)
    }
}

pub fn parse_ctcp(msg: &str) -> Option<CtcpCommand> {
    let mut bytes = &msg.as_bytes()[1..];

    let mut cmd = Vec::new();
    while let &[head, ref rest @ ..] = bytes
        && is_valid_ctcp_command(head)
    {
        cmd.push(head);
        bytes = rest;
    }

    let params = match bytes {
        [] | [CTCP_DELIM, ..] => String::new(),
        [b' ', rest @ ..] => {
            String::from_utf8(
                rest.iter()
                    .copied()
                    // this will stop at the closing delim or other invalid characters
                    .take_while(|&c| is_valid_ctcp_params(c))
                    .collect::<Vec<_>>(),
            )
            .ok()?
        }
        // UNREACHABLE: the command eats all characters except NUL, delim, CR, LF, and space.
        // CR and LF would terminate the IRC message, but this protocol is based on the contents of
        // an IRC message, so it cannot contain those. NUL is also not permitted in IRC messages.
        // space and delim are handled above.
        [c, ..] => {
            unreachable!("ctcp message impossible char after cmd: {:#04X?}", c)
        }
    };

    match cmd.as_slice() {
        b"ACTION" => Some(CtcpCommand::Action(params)),
        b"CLIENTINFO" => Some(CtcpCommand::Clientinfo),
        unk => {
            warn!("unkown CTCP command {:02X?}", unk);
            None
        }
    }
}

fn is_valid_ctcp_command(c: u8) -> bool {
    matches!(c, 0x02..=0x09 | 0x0B..=0x0C | 0x0E..=0x1F | 0x21..=0xFF)
}

fn is_valid_ctcp_params(c: u8) -> bool {
    matches!(c, 0x02..=0x09 | 0x0B..=0x0C | 0x0E..=0xFF)
}

#[derive(Debug)]
pub enum CtcpReply {
    CLIENTINFO,
}

impl CtcpReply {
    pub fn to_msg(self, nick: &Nickname) -> IrcMessage {
        IrcMessage {
            tags: None,
            source: None,
            message: Message::Notice {
                targets: vec![Target::Nickname(nick.clone())],
                msg: self.as_irc_str(),
            },
        }
    }

    fn as_irc_str(&self) -> String {
        match self {
            CtcpReply::CLIENTINFO => {
                format!(
                    "{}CLIENTINFO {}{}",
                    CTCP_DELIM_STR,
                    IMPLEMENTED_CTCP.join(" "),
                    CTCP_DELIM_STR
                )
            }
        }
    }
}
