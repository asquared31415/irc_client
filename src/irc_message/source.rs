use core::fmt::Display;

use crate::channel::Nickname;

#[derive(Debug, Clone)]
pub enum Source {
    Server(String),
    Nick(Nickname, Option<String>, Option<String>),
}

impl Source {
    pub(super) fn parse(s: &str) -> Source {
        match s.split_once('!') {
            Some((nick, rest)) => match rest.split_once('@') {
                Some((user, host)) => {
                    let nick = Nickname::new(nick).unwrap();
                    Source::Nick(nick, Some(user.to_string()), Some(host.to_string()))
                }
                None => {
                    let nick = Nickname::new(nick).unwrap();
                    Source::Nick(nick, Some(rest.to_string()), None)
                }
            },
            // may be only hostname, but it could just be `nick@host`
            None => match s.split_once('@') {
                Some((nick, host)) => {
                    let nick = Nickname::new(nick).unwrap();
                    Source::Nick(nick, None, Some(host.to_string()))
                }
                None => Source::Server(s.to_string()),
            },
        }
    }
}

impl Source {
    pub fn get_name(&self) -> &str {
        match self {
            Source::Server(server) => server.as_str(),
            Source::Nick(nick, _, _) => nick.as_str(),
        }
    }
}

impl Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get_name())
    }
}
