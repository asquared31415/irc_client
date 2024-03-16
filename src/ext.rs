pub trait StrExt {
    fn split_first_matches(&self, c: char) -> Option<(char, &str)>;
}

impl StrExt for str {
    fn split_first_matches(&self, c: char) -> Option<(char, &str)> {
        if self.starts_with(c) {
            Some((c, &self[1..]))
        } else {
            None
        }
    }
}
