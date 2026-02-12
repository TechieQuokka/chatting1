use aes_gcm::{
    aead::Aead,
    Aes256Gcm, Key, KeyInit, Nonce,
};
use anyhow::{anyhow, bail, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::RngCore;

const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;
const SALT_LEN: usize = 16;
/// Fixed plaintext used to produce the password verification token.
const VERIFY_MAGIC: &str = "chatapp-v1-verification";

/// A symmetric AES-256-GCM key derived from a room password.
pub struct RoomKey {
    key: [u8; KEY_LEN],
}

impl RoomKey {
    /// Derive a room key using Argon2id.
    ///
    /// Salt = room name bytes, zero-padded to `SALT_LEN` (16 bytes).
    /// This ensures the same password produces different keys in different rooms.
    ///
    /// For a password-less room, pass `password = ""`.
    pub fn derive(password: &str, room_name: &str) -> Result<Self> {
        // Build salt from room name (padded / truncated to SALT_LEN).
        let mut salt = [0u8; SALT_LEN];
        let room_bytes = room_name.as_bytes();
        let copy_len = room_bytes.len().min(SALT_LEN);
        salt[..copy_len].copy_from_slice(&room_bytes[..copy_len]);

        // Use conservative parameters compatible with iSH (x86 emulation).
        // m_cost = 8 MiB, t_cost = 2 iterations, p_cost = 1 thread.
        let params = Params::new(8 * 1024, 2, 1, Some(KEY_LEN))
            .map_err(|e| anyhow!("Argon2 params: {}", e))?;
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        let mut key = [0u8; KEY_LEN];
        argon2
            .hash_password_into(password.as_bytes(), &salt, &mut key)
            .map_err(|e| anyhow!("Key derivation failed: {}", e))?;

        Ok(Self { key })
    }

    // ── Encryption ────────────────────────────────────────────────────────────

    /// Encrypt `plaintext` and return `nonce(12) ++ ciphertext+tag`.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let cipher = self.cipher();

        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| anyhow!("Encryption failed"))?;

        let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    /// Decrypt `nonce(12) ++ ciphertext+tag` and return the plaintext.
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < NONCE_LEN + 16 {
            bail!("Ciphertext too short");
        }
        let cipher = self.cipher();
        let nonce = Nonce::from_slice(&data[..NONCE_LEN]);
        let ciphertext = &data[NONCE_LEN..];

        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| anyhow!("Decryption failed — wrong key or corrupted data"))
    }

    // ── Verification token ────────────────────────────────────────────────────

    /// Produce a verification token: encrypt `VERIFY_MAGIC::<room_name>`.
    /// Room members publish this when a new peer joins, so the joiner can
    /// confirm they have the correct password before entering.
    pub fn make_verification_token(&self, room_name: &str) -> Result<Vec<u8>> {
        let payload = format!("{}::{}", VERIFY_MAGIC, room_name);
        self.encrypt(payload.as_bytes())
    }

    /// Return `true` iff `token` decrypts successfully and its plaintext
    /// matches the expected verification string for `room_name`.
    pub fn verify_token(&self, token: &[u8], room_name: &str) -> bool {
        match self.decrypt(token) {
            Ok(plaintext) => {
                let expected = format!("{}::{}", VERIFY_MAGIC, room_name);
                plaintext == expected.as_bytes()
            }
            Err(_) => false,
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn cipher(&self) -> Aes256Gcm {
        let key = Key::<Aes256Gcm>::from_slice(&self.key);
        Aes256Gcm::new(key)
    }
}
