use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::PathBuf,
};

use anyhow::Result;
use chrono::Utc;

use crate::types::DisplayMessage;

pub struct Logger {
    writer: BufWriter<File>,
}

impl Logger {
    /// Open (or create) the log file for `room_name` inside `log_dir`.
    pub fn open(log_dir: &str, room_name: &str) -> Result<Self> {
        // Sanitise room name for use as a filename.
        let safe_name: String = room_name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();

        let path = PathBuf::from(log_dir).join(format!("{}.log", safe_name));

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;

        Ok(Self {
            writer: BufWriter::new(file),
        })
    }

    /// Append a chat message or system event line.
    pub fn log(&mut self, msg: &DisplayMessage) -> Result<()> {
        let ts = msg.timestamp.to_rfc3339();
        let line = if msg.is_system {
            format!("[{}] *** {}\n", ts, msg.text)
        } else {
            format!("[{}] {}: {}\n", ts, msg.sender, msg.text)
        };
        self.writer.write_all(line.as_bytes())?;
        self.writer.flush()?;
        Ok(())
    }

    /// Append a plain system string (e.g. "session started").
    pub fn log_event(&mut self, text: &str) -> Result<()> {
        let ts = Utc::now().to_rfc3339();
        let line = format!("[{}] *** {}\n", ts, text);
        self.writer.write_all(line.as_bytes())?;
        self.writer.flush()?;
        Ok(())
    }
}
