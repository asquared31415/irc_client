use crossterm::style::Stylize;
use log::*;

use crate::{
    handlers::ctcp::{self, CtcpCommand, CtcpReply, CTCP_DELIM},
    irc_message::Source,
    state::ClientState,
    targets::Target,
    ui::text::Line,
    util,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Notice,
    Privmsg,
}

pub(super) fn handle_message(
    state: &mut ClientState,
    kind: MessageKind,
    source: &Option<Source>,
    target: &Target,
    msg: &str,
) {
    let mut target = target.clone();
    // adjust nickname targets to be the *sender* of the message instead of the
    // receiver (which is always the current user)
    if matches!(target, Target::Nickname(_)) {
        let Some(Source::Nick(nick, _, _)) = source else {
            return;
        };
        target = Target::Nickname(nick.clone());
    }

    // handle CTCP messages specially
    if matches!(msg.as_bytes().get(0), Some(&CTCP_DELIM)) {
        let Some(ctcp) = ctcp::parse_ctcp(msg) else {
            return;
        };
        debug!("{:#?}", ctcp);
        match ctcp {
            CtcpCommand::Action(action) => {
                let mut line = util::line_now().push("* ".magenta());
                if let Some(source) = source.as_ref() {
                    line = line.join(util::nick_line(source.get_name(), false));
                }
                let line = line.push_unstyled(" ").push_unstyled(action);
                state.add_line(target.clone(), line);
            }
            CtcpCommand::Clientinfo => {
                let Some(Source::Nick(nick, _, _)) = source else {
                    warn!("CTCP CLIENTINFO without nick source");
                    return;
                };
                state.send_msg(CtcpReply::CLIENTINFO.to_msg(nick));
            }
        }
    } else {
        let mut line = util::line_now();
        if let Some(source) = source.as_ref() {
            line = line.join(util::message_nick_line(source.get_name(), false));
        }
        match kind {
            MessageKind::Notice => line = line.join(Line::default().push("NOTICE ".green())),
            MessageKind::Privmsg => {}
        }
        line = line.join(Line::from(msg.to_string()));

        state.add_line(target.clone(), line);
    }
}
