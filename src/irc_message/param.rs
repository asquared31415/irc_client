#[derive(Debug, Clone)]
pub enum Param {
    String(String),
    List(Vec<String>),
}

impl Param {
    /// returns the param as a &str, if it was a normal string param
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Param::String(s) => Some(s),
            Param::List(_) => None,
        }
    }

    pub fn as_list(&self) -> Option<&[String]> {
        match self {
            Param::String(_) => None,
            Param::List(list) => Some(list),
        }
    }

    /// returns a vec containing the single string parameter, if the param is a string, or returns
    /// the list parameter if the param is a list. this is useful for places where a list is
    /// optional, like JOIN.
    pub fn optional_list(&self) -> Vec<String> {
        match self {
            Param::String(s) => vec![s.to_string()],
            Param::List(list) => list.to_owned(),
        }
    }

    pub fn to_irc_string(&self) -> String {
        match self {
            Param::String(s) => s.to_owned(),
            Param::List(args) => args.join(","),
        }
    }
}

pub(super) fn parse_params(s: &str) -> Vec<Param> {
    let mut params = vec![];

    let mut s = s.trim_start_matches(' ');
    while s.len() > 0 {
        let end_idx = s.find(' ').unwrap_or(s.len());
        let param = &s[..end_idx];

        // NOTE: if a parameter starts with a `:`, the rest of the message is a parameter. the last
        // parameter may omit the `:` if it's not necessary to disambiguate.
        if param.starts_with(':') {
            params.push(Param::String(s[1..].to_string()));
            // ate the rest of the params, return early
            return params;
        } else if param.contains(',') {
            let parts = param
                .split(',')
                .filter_map(|s| {
                    if s.is_empty() {
                        None
                    } else {
                        Some(s.to_string())
                    }
                })
                .collect::<Vec<_>>();
            params.push(Param::List(parts));
        } else {
            params.push(Param::String(param.to_string()));
        }

        s = s[end_idx..].trim_start_matches(' ');
    }

    params
}
