use std::io::{self, BufRead, Stdin, Stdout, Write};

pub struct TerminalUi {
    stdin: Stdin,
    stdout: Stdout,
}

impl TerminalUi {
    pub fn new() -> Self {
        Self {
            stdin: io::stdin(),
            stdout: io::stdout(),
        }
    }
    pub fn read(&self) -> io::Result<String> {
        let mut stdin = self.stdin.lock();
        let mut buf = String::new();
        stdin.read_line(&mut buf)?;
        Ok(buf)
    }

    pub fn writeln(&self, msg: impl AsRef<str>) -> io::Result<()> {
        let mut stdout = self.stdout.lock();
        let msg = msg.as_ref();
        stdout.write_all(msg.as_bytes())?;
        stdout.write_all(b"\n")?;
        stdout.flush()
    }
}

impl Write for TerminalUi {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stdout.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.stdout.flush()
    }
}
