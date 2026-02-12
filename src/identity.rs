use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use libp2p::{
    identity::{self, Keypair},
    PeerId,
};

use crate::config::Config;

pub struct Identity {
    pub keypair: Keypair,
    pub peer_id: PeerId,
    pub nickname: String,
    /// First 4 hex chars derived from the Peer ID bytes — e.g. "3f2a".
    pub discriminator: String,
}

impl Identity {
    /// Load or generate an Ed25519 keypair from `config`, then build the identity.
    /// Saves updated config if a new keypair was generated.
    pub fn load_or_create(config: &mut Config) -> Result<Self> {
        let keypair = match &config.private_key_b64 {
            Some(b64) => {
                let bytes = B64.decode(b64).context("decode private key base64")?;
                Keypair::from_protobuf_encoding(&bytes).context("decode keypair from protobuf")?
            }
            None => {
                let kp = identity::Keypair::generate_ed25519();
                let bytes = kp
                    .to_protobuf_encoding()
                    .context("encode keypair to protobuf")?;
                config.private_key_b64 = Some(B64.encode(&bytes));
                kp
            }
        };

        let peer_id = PeerId::from(keypair.public());
        let discriminator = discriminator_from_peer_id(&peer_id);

        let nickname = config
            .nickname
            .clone()
            .unwrap_or_else(|| format!("Peer{}", &discriminator));

        Ok(Self {
            keypair,
            peer_id,
            nickname,
            discriminator,
        })
    }

    /// Returns the formatted display name, e.g. `"Seung#3f2a"`.
    pub fn display_name(&self) -> String {
        format!("{}#{}", self.nickname, self.discriminator)
    }
}

/// Derive a 4-character hex discriminator from the first two bytes of the
/// multihash bytes of a Peer ID.
pub fn discriminator_from_peer_id(peer_id: &PeerId) -> String {
    let bytes = peer_id.to_bytes();
    // bytes[0] and bytes[1] are the multihash code + length prefix.
    // Take bytes 2..4 which are the start of the actual key hash.
    let a = bytes.get(2).copied().unwrap_or(bytes[0]);
    let b = bytes.get(3).copied().unwrap_or(bytes[1]);
    format!("{:02x}{:02x}", a, b)
}
