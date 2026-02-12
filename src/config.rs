use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Display nickname chosen by the user.
    pub nickname: Option<String>,
    /// Ed25519 keypair encoded as protobuf then base64.
    pub private_key_b64: Option<String>,
    /// Directory for per-room chat logs.
    #[serde(default = "default_log_dir")]
    pub log_dir: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            nickname: None,
            private_key_b64: None,
            log_dir: default_log_dir(),
        }
    }
}

fn default_log_dir() -> String {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".chat_logs")
        .to_string_lossy()
        .into_owned()
}

impl Config {
    /// Path to `~/.chatrc`.
    pub fn path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".chatrc")
    }

    /// Load from disk, or return `Default` if missing / unreadable.
    pub fn load_or_default() -> Self {
        let path = Self::path();
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => toml::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Persist the current config to `~/.chatrc`.
    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Ensure the log directory exists.
    pub fn ensure_log_dir(&self) -> Result<()> {
        std::fs::create_dir_all(&self.log_dir)?;
        Ok(())
    }
}
