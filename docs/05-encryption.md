# Encryption Design

## Goals

1. Only peers who know the room password can read messages.
2. The password is never transmitted over the network.
3. A wrong password produces an unambiguous, immediate failure — not garbled
   text.
4. The design requires no key exchange protocol between peers. The shared
   secret is derived independently by each peer from the same password.

---

## Cryptographic Primitives

| Primitive | Algorithm | Purpose |
|-----------|-----------|---------|
| Key derivation | Argon2id | Derive a symmetric key from a password string |
| Symmetric encryption | AES-256-GCM | Encrypt and authenticate message payloads |
| Random nonce | OS CSPRNG | Ensure each ciphertext is unique |

---

## Key Derivation

When a user creates or joins a room with a password, the application derives
a 256-bit symmetric key using **Argon2id**:

- **Input**: the room password (UTF-8 string)
- **Salt**: the room name (UTF-8 string, padded/truncated to 16 bytes)
- **Output**: 32-byte key

Using the room name as the salt means that the same password produces a
different key for each differently named room. This prevents cross-room
replay of ciphertexts.

Argon2id parameters are chosen to be memory-hard (resistant to GPU cracking)
while remaining fast enough for interactive use (sub-second on typical
hardware and on iSH).

For rooms without a password, the key is derived from the empty string with
the same salt. This provides consistent wire encryption without requiring the
user to enter a password.

---

## Message Encryption

Every outgoing message payload is encrypted as follows:

```
plaintext = { sender_nickname, sender_discriminator, timestamp, message_text }
nonce     = 12 random bytes from the OS CSPRNG
ciphertext, tag = AES-256-GCM.encrypt(key, nonce, plaintext)
wire_payload = nonce ++ ciphertext ++ tag
```

- The **nonce** is randomly generated per message, prepended to the payload.
- AES-256-GCM produces an **authentication tag** that covers both the
  ciphertext and the nonce. Any tampering (including a wrong decryption key)
  causes tag verification to fail.
- The `wire_payload` is what GossipSub transmits. It is opaque bytes.

---

## Message Decryption

On receipt of a GossipSub message:

```
wire_payload = nonce ++ ciphertext ++ tag
result = AES-256-GCM.decrypt(key, nonce, ciphertext, tag)

if result == Ok(plaintext):
    display message
else:
    silently discard (wrong key / tampered message)
```

Decryption failures are silently discarded. A peer with the wrong password
will receive GossipSub payloads but cannot read any of them.

---

## Password Verification Token

To give the user immediate feedback when they enter the wrong password, the
application uses a **verification token** — a special message published to
the topic by the room creator (or any existing member) when a new peer joins.

```
verification_plaintext = "chatapp-v1-verification::{room-name}"
verification_token = AES-256-GCM.encrypt(key, fixed_nonce, verification_plaintext)
```

When the joining peer receives this token and attempts decryption:

- Success → tag validates → password is correct → enter room
- Failure → tag fails → "Access denied — wrong password." → leave topic

The verification token is a regular GossipSub message with a special
`msg_type` field set to `JOIN_VERIFY`. Application layer filters these
out from the chat display.

---

## Security Scope

This design provides **confidentiality** (unreadable without the key) and
**integrity** (tampered messages are discarded) for message content.

It does **not** provide:

- **Anonymity**: Peer IDs and IP addresses are visible at the libp2p transport
  layer to connected peers (standard for P2P networks).
- **Forward secrecy**: The same room key is used for the lifetime of the
  session. Compromise of the password retroactively exposes all stored logs.
- **Access revocation**: There is no mechanism to remove a peer from a room
  once they have the password.

These limitations are acceptable for a v1 application.
