pub trait StrExt {
    fn split_prefix(&self, c: char) -> Option<(char, &str)>;
}

impl StrExt for str {
    fn split_prefix(&self, c: char) -> Option<(char, &str)> {
        if self.starts_with(c) {
            Some((c, &self[1..]))
        } else {
            None
        }
    }
}
