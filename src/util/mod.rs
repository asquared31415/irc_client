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
