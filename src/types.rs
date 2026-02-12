use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Display ──────────────────────────────────────────────────────────────────

/// A message ready to render in the terminal.
#[derive(Debug, Clone)]
pub struct DisplayMessage {
    pub timestamp: DateTime<Utc>,
    /// "Nick#disc" for chat messages, empty for system events.
    pub sender: String,
    pub text: String,
    pub is_system: bool,
}

impl DisplayMessage {
    pub fn chat(sender: &str, text: &str) -> Self {
        Self {
            timestamp: Utc::now(),
            sender: sender.to_string(),
            text: text.to_string(),
            is_system: false,
        }
    }

    pub fn system(text: &str) -> Self {
        Self {
            timestamp: Utc::now(),
            sender: String::new(),
            text: text.to_string(),
            is_system: true,
        }
    }

    pub fn render(&self, width: usize) -> String {
        let time = self.timestamp.format("%H:%M");
        if self.is_system {
            let line = format!("[{}] *** {}", time, self.text);
            truncate(&line, width)
        } else {
            let line = format!("[{}] {}: {}", time, self.sender, self.text);
            truncate(&line, width)
        }
    }
}

fn truncate(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        s.to_string()
    } else {
        s.chars().take(width.saturating_sub(1)).collect::<String>() + "…"
    }
}

// ── Wire protocol ─────────────────────────────────────────────────────────────

/// JSON-serialised, then AES-256-GCM encrypted before transmission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireMessage {
    pub msg_type: WireMessageType,
    pub sender_nick: String,
    pub sender_disc: String,
    pub timestamp_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WireMessageType {
    /// Normal chat message.
    Chat,
    /// Encrypted verification token published by room members when a new peer
    /// subscribes to the topic (password check).
    VerificationToken,
}

// ── Inter-task channels ───────────────────────────────────────────────────────

/// Events flowing from the network task → application task.
#[derive(Debug)]
pub enum NetworkEvent {
    /// Raw encrypted payload received on a GossipSub topic.
    MessageReceived { topic: String, payload: Vec<u8> },
    PeerConnected,
    PeerDisconnected(String),
    /// A peer subscribed to one of our GossipSub topics.
    PeerSubscribed { topic: String, peer_id: String },
    ListeningOn(String),
    NewExternalAddr(String),
}

/// Commands flowing from the application task → network task.
#[derive(Debug)]
pub enum NetworkCommand {
    Subscribe(String),
    Unsubscribe(String),
    Publish { topic: String, data: Vec<u8> },
    Dial(String),
    QueryListenAddrs,
}

/// Events flowing from the application task → CLI task (for rendering).
#[derive(Debug, Clone)]
pub enum UiEvent {
    NewMessage(DisplayMessage),
    /// Update the header status line.
    StatusUpdate { room: Option<String>, peers: usize },
    /// Navigate to the main menu.
    ShowMainMenu,
    /// Room was created — show the code to share.
    RoomCreated { name: String, code: String },
    /// Successfully joined a room.
    RoomJoined(String),
    /// Wrong password.
    AccessDenied,
    Error(String),
}

/// Commands flowing from the CLI task → application task.
#[derive(Debug)]
pub enum CliCommand {
    SendMessage(String),
    CreateRoom { name: String, password: String },
    JoinRoom { code: String, password: String },
    LeaveRoom,
    ListPeers,
    Help,
    Quit,
}
