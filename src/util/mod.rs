use crossterm::style::Stylize;

use crate::ui::text::Line;

pub mod unicode_width;

pub fn line_now() -> Line<'static> {
    const FMT: &str = "%H:%M:%S";
    let now = chrono::Local::now();

    Line::default()
        .push_unstyled("[")
        .push(now.format(FMT).to_string().red())
        .push_unstyled("]")
}

pub fn message_nick_line(nick: &str, me: bool) -> Line<'static> {
    Line::default()
        .push_unstyled("<")
        .join(nick_line(nick, me))
        .push_unstyled(">")
}

pub fn nick_line(nick: &str, me: bool) -> Line<'static> {
    if me {
        Line::default().push(nick.to_string().cyan())
    } else {
        Line::default().push(nick.to_string().magenta())
    }
}
