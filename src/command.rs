use thiserror::Error;

#[derive(Debug)]
pub enum Command {
    Join(String),
}

#[derive(Debug, Error)]
pub enum CommandParseErr {
    #[error("missing a command")]
    MissingCommand,
}

impl Command {
    pub fn parse<S: AsRef<str>>(s: S) -> Result<Self, CommandParseErr> {
        let s = s.as_ref().to_lowercase();
        let parts = s.split(' ').filter(|s| !s.is_empty()).collect::<Vec<_>>();
        let Some(cmd) = parts.first() else {
            return Err(CommandParseErr::MissingCommand);
        };

        match *cmd {
            "join" => {}
            _ => todo!(),
        }

        todo!()
    }
}
