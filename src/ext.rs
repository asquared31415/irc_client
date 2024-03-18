use std::io::{self, Read, Write};

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

pub trait ReadWrite: Read + Write {}

impl<T: Read + Write> ReadWrite for T {}

pub trait WriteExt {
    fn write_all_blocking(&mut self, buf: &[u8]) -> io::Result<()>;
}

impl<T: Write> WriteExt for T {
    fn write_all_blocking(&mut self, mut buf: &[u8]) -> io::Result<()> {
        while !buf.is_empty() {
            match self.write(buf) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "failed to write whole buffer",
                    ));
                }
                Ok(n) => buf = &buf[n..],
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}
