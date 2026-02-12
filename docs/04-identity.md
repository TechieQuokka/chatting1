# Identity Design

## Peer Identity

Every instance of the application generates an **Ed25519 key pair** on first
run. The private key is stored in the local config file. The public key is
hashed to produce a **Peer ID** — a self-certifying, globally unique
identifier that requires no central authority to issue.

The Peer ID serves two roles:
1. **Network identity**: used by libp2p for routing and authentication.
2. **Discriminator source**: the first 4 hex characters of the Peer ID are
   displayed next to the nickname to distinguish users with the same name.

---

## Nickname System

### First Run

On first launch, the application prompts the user for a nickname:

```
Welcome! Enter your nickname: Seung
```

The nickname is saved to the config file and reused on every subsequent run.

### Display Format

Nicknames are shown with a 4-character discriminator derived from the Peer ID:

```
Seung#3f2a
```

If two users in the same room both choose the nickname "Seung", they will
appear as `Seung#3f2a` and `Seung#7b1c` — visually distinct at a glance.

### Rules

- Nicknames may contain any printable characters.
- There is no uniqueness constraint. Duplicates are allowed and resolved
  visually by the discriminator.
- Nickname length is capped at 32 characters to keep the display clean.
- Nicknames are not verified by any authority. A peer may choose any nickname.

### Changing the Nickname

The user can change their nickname by editing the config file directly,
or by deleting the config entry and restarting the application.
In-session nickname changes are not supported in v1.

---

## Config File

The config file is stored at `~/.chatrc` (or the platform equivalent).
It is a TOML file containing:

| Key | Description |
|-----|-------------|
| `nickname` | The user's chosen display name |
| `private_key` | Base64-encoded Ed25519 private key |
| `log_dir` | Directory for message logs (default: `~/.chat_logs/`) |

The private key must be kept safe. If it is lost, the peer's identity
changes on the next run (a new key pair is generated). This has no effect
on the ability to join rooms, but past encrypted messages cannot be
re-decrypted with a new key (they are encrypted with room keys, not peer
keys, so this is not a practical issue).

---

## Message Attribution

Each message payload includes the sender's nickname and discriminator.
The application layer embeds these fields in the plaintext before encryption.
When a message is decrypted successfully, the sender's display name is
extracted from the plaintext and rendered in the message pane.

This means the sender's identity is protected by the same AES-256-GCM
encryption as the message content. A passive observer cannot determine
who sent a message in a password-protected room.
