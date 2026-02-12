use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Identifies a GossipSub topic for a given room.
pub fn topic_for_room(room_name: &str) -> String {
    format!("/chatapp/v1/rooms/{}", room_name)
}

// ── Room code ─────────────────────────────────────────────────────────────────

/// Data embedded in a room code shared out-of-band.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomCodeData {
    /// Human-readable room name (maps to GossipSub topic).
    pub room_name: String,
    /// libp2p Peer ID of the creator as a base58-encoded string.
    pub peer_id: String,
    /// Multiaddr the creator is listening on.
    pub addr: String,
}

impl RoomCodeData {
    /// Encode to a compact Base58 string safe to share over any channel.
    pub fn encode(&self) -> Result<String> {
        let json = serde_json::to_vec(self).context("serialize room code")?;
        Ok(bs58::encode(&json).into_string())
    }

    /// Decode a Base58 room code string.
    pub fn decode(code: &str) -> Result<Self> {
        let json = bs58::decode(code)
            .into_vec()
            .context("base58 decode room code")?;
        let data: Self = serde_json::from_slice(&json).context("deserialize room code")?;
        Ok(data)
    }
}

// ── Active room state ─────────────────────────────────────────────────────────

/// Tracks the state of the currently joined room.
#[derive(Debug, Clone)]
pub struct RoomState {
    pub name: String,
    pub topic: String,
    pub peer_count: usize,
}

impl RoomState {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            topic: topic_for_room(name),
            peer_count: 0,
        }
    }
}
