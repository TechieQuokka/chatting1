# Room Design

## Room Identity

A room is identified solely by its **name**. The name is a free-form UTF-8
string chosen by the creator. The name maps directly to a GossipSub topic:

```
room name "rust-chat"  →  topic "/chatapp/v1/rooms/rust-chat"
```

There is no room registry. Any peer that knows the room name (and password,
if one is set) can subscribe to the topic and participate.

---

## Room Lifecycle

### Creating a Room

1. User selects "Create room" from the main menu.
2. User enters a room name.
3. User optionally enters a password (empty = no password).
4. Application derives the encryption key from the password (see `05-encryption.md`).
5. Application subscribes to the corresponding GossipSub topic.
6. Application generates a **room code** (see `02-networking.md`).
7. The room code is displayed so the creator can share it with others.

### Joining a Room

1. User selects "Join room" from the main menu.
2. User enters the room code shared by the creator.
3. Application decodes the room code to extract the creator's Peer ID,
   address, and room name.
4. Application dials the creator.
5. User enters the room password (or leaves blank if none).
6. Application attempts to verify the password using the **verification token**
   protocol (described below).
7. On success: subscribe to the topic and begin receiving messages.
8. On failure: display "Access denied — wrong password." and return to menu.

### Leaving a Room

The user types `/quit` or presses Ctrl-C. The application unsubscribes from
the GossipSub topic. No notification is broadcast to other peers; they will
naturally stop receiving messages from this peer as the mesh re-gossips.

---

## Password Verification Protocol

Passwords are never transmitted over the network in any form.
Verification is done **locally** using a shared verification token.

### How It Works

When a room is created with a password:

1. The creator derives a symmetric key `K` from the password using Argon2id.
2. The creator encrypts a fixed, well-known plaintext string
   (e.g., `"chatapp-v1-verification"`) using `K` and AES-256-GCM.
3. This ciphertext is the **verification token**. It is broadcast to the
   GossipSub topic as the first message (a special `JOIN_VERIFY` message type).

When a peer joins:

1. The joining peer subscribes to the topic and receives the verification token.
2. The peer derives key `K'` from the password the user typed.
3. The peer attempts to decrypt the verification token using `K'`.
4. If decryption succeeds (AEAD tag validates) → password is correct → proceed.
5. If decryption fails (AEAD tag mismatch) → password is wrong →
   display "Access denied." → unsubscribe from topic → return to menu.

### Why This Is Secure

- AES-256-GCM authentication tags make it computationally infeasible to
  accept a wrong password by coincidence.
- The password itself never leaves the local machine.
- An observer on the network sees only the ciphertext, not the plaintext or
  the key.

### Rooms Without Passwords

If no password is set, all messages are encrypted with a key derived from
the room name itself (as a deterministic empty-password derivation). This
means all subscribers can read messages, but the wire format is still
encrypted. The verification token check is skipped.

---

## Room Code Format

The room code is a Base58-encoded string. It encodes:

| Field | Description |
|-------|-------------|
| Room name | UTF-8 string, the GossipSub topic identifier |
| Creator Peer ID | The libp2p Peer ID of the room creator |
| Creator multiaddress | IP address and port the creator is listening on |

The code is compact enough to share via text message or paste into a terminal.
Example (illustrative, not real):

```
7xKpQm3NvBsRtYdEfGhJ2cLwAoP9uXiZ
```

---

## Constraints and Limitations

- Room names are case-sensitive.
- There is no room list browsable from the outside. You must know the room
  code to join.
- Room history is not available to late joiners. Only messages received
  after subscription are visible (see `07-persistence.md` for local log
  behavior).
- There is no room admin or moderation concept. Any peer who knows the
  room code and password is a full participant.
