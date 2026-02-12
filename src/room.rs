use anyhow::{bail, Context, Result};

/// Identifies a GossipSub topic for a given room.
pub fn topic_for_room(room_name: &str) -> String {
    format!("/chatapp/v1/rooms/{}", room_name)
}

// ── Room code ─────────────────────────────────────────────────────────────────

/// Data embedded in a room code shared out-of-band.
///
/// Encoded as `room_name\0peer_id\0addr` → Base58, which is notably shorter
/// than the previous JSON → Base58 encoding.
#[derive(Debug, Clone)]
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
        // NUL-delimited: room_name\0peer_id\0addr — no JSON overhead.
        let raw = format!("{}\0{}\0{}", self.room_name, self.peer_id, self.addr);
        Ok(bs58::encode(raw.as_bytes()).into_string())
    }

    /// Decode a Base58 room code string.
    pub fn decode(code: &str) -> Result<Self> {
        let bytes = bs58::decode(code)
            .into_vec()
            .context("base58 decode room code")?;
        let s = std::str::from_utf8(&bytes).context("room code is not valid UTF-8")?;
        let parts: Vec<&str> = s.splitn(3, '\0').collect();
        if parts.len() != 3 {
            bail!("invalid room code format");
        }
        Ok(Self {
            room_name: parts[0].to_string(),
            peer_id: parts[1].to_string(),
            addr: parts[2].to_string(),
        })
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
