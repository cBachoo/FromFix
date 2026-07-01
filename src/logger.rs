//! Minimal file logger that writes FromFix.log next to the game executable.

use std::fs::OpenOptions;
use std::fs::File;
use std::io::Write;
use std::sync::{Mutex, OnceLock};

static LOG: OnceLock<Mutex<File>> = OnceLock::new();

/// Open (truncating) the log file. Silently no-ops if the file can't be opened.
pub fn init(path: &str) {
    if let Ok(f) = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
    {
        let _ = LOG.set(Mutex::new(f));
    }
}

/// Write a single log line. Called through the [`log!`] macro.
pub fn write(args: core::fmt::Arguments) {
    if let Some(m) = LOG.get() {
        if let Ok(mut f) = m.lock() {
            let _ = writeln!(f, "{}", args);
            let _ = f.flush();
        }
    }
}

/// Log a formatted line to the FromFix log file.
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => { $crate::logger::write(format_args!($($arg)*)) };
}
