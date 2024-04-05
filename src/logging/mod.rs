use std::{
    fs,
    fs::File,
    io,
    io::prelude::Write as _,
    path::Path,
    sync::{Mutex, MutexGuard},
    thread,
    time::SystemTime,
};

use log::{LevelFilter, Log, SetLoggerError};

const LOG_PATH: &str = "./logs/";

pub fn init(hostname: impl AsRef<str>, max_level: LevelFilter) -> Result<(), SetLoggerError> {
    log::set_boxed_logger(Box::new(
        Logger::new(LOG_PATH, hostname, max_level).expect("unable to create logger"),
    ))?;
    log::set_max_level(max_level);
    Ok(())
}

pub struct Logger {
    max_level: LevelFilter,
    log_file: Mutex<File>,
}

impl Logger {
    pub fn new(
        log_folder: impl AsRef<Path>,
        hostname: impl AsRef<str>,
        max_level: LevelFilter,
    ) -> io::Result<Self> {
        let folder = log_folder.as_ref();
        fs::create_dir_all(folder)?;
        let path = folder.join(format!(
            "{}-{}.txt",
            hostname.as_ref(),
            humantime::format_rfc3339_seconds(SystemTime::now())
        ));
        Ok(Self {
            log_file: Mutex::new(File::options().create(true).append(true).open(path)?),
            max_level,
        })
    }

    fn file(&self) -> MutexGuard<'_, File> {
        self.log_file.lock().unwrap()
    }
}

impl Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() >= self.max_level
    }

    fn log(&self, record: &log::Record) {
        // if record.level() > log::max_level() {
        //     return;
        // }

        let _ = self.file().write_fmt(format_args!(
            "[{}] [{:<5}] [{:016X}] {}\n",
            humantime::format_rfc3339_millis(SystemTime::now()),
            record.level(),
            thread::current().id().as_u64(),
            record.args(),
        ));
    }

    fn flush(&self) {
        let _ = self.file().flush();
    }
}
