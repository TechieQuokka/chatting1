# Crate Manifest

## Rationale

Every dependency adds compile time, binary size, and maintenance surface.
Each crate below is included because it solves a hard problem that would
be impractical to reimplement correctly from scratch.

---

## Core Dependencies

### `libp2p`

The P2P networking stack. Used for all network communication.

Selected sub-features (libp2p is feature-gated):

| Feature | Purpose |
|---------|---------|
| `tcp` | Base transport |
| `noise` | Transport-layer authentication and encryption |
| `yamux` | Stream multiplexing |
| `gossipsub` | Topic-based pub/sub messaging (rooms) |
| `kad` | Kademlia DHT for internet peer discovery |
| `mdns` | Local network peer discovery |
| `relay` | Circuit Relay v2 for NAT traversal |
| `dcutr` | Direct Connection Upgrade (hole punching) |
| `identify` | Exchange protocol versions and listen addresses with peers |

Only the features actually used are enabled. This keeps compile times
reasonable.

---

### `tokio`

Async runtime. libp2p's Rust implementation is async-native and requires
Tokio. All application tasks (swarm loop, input reader, render loop) run
as Tokio tasks communicating via channels.

Features used: `full` (simplifies feature selection for a small project;
can be trimmed later if binary size becomes a concern).

---

### `crossterm`

Cross-platform terminal manipulation. Used for:

- Raw mode (capture individual keystrokes, mask password input)
- Cursor positioning (split-pane layout)
- Terminal resize event handling
- ANSI color for message formatting

Chosen over alternatives because it works correctly on Windows, Linux,
macOS, and iSH without requiring ncurses.

---

### `aes-gcm`

AES-256-GCM authenticated encryption. Used to encrypt and decrypt all
message payloads and the room verification token.

Chosen because:
- It is the standard AEAD cipher recommended for new designs.
- The `aes-gcm` crate is maintained by the RustCrypto project, widely
  audited, and has no unsafe code.

---

### `argon2`

Password-based key derivation. Used to derive the room encryption key from
the user-supplied room password.

Argon2id is the current winner of the Password Hashing Competition and is
recommended by OWASP for password hashing and key derivation. The `argon2`
crate is also from the RustCrypto project.

---

### `serde` + `serde_json`

Serialization and deserialization. Used to:

- Encode message payloads (plaintext struct → JSON bytes before encryption)
- Encode/decode the room code
- Read and write the config file (combined with `toml`)

`serde_json` is used for wire payloads because the format is
self-describing and easy to inspect during development.

---

### `toml`

Config file format. Used to read and write `~/.chatrc`.

TOML is chosen because it is human-readable and human-editable, which
matters for a config file users may need to modify by hand (e.g., changing
nickname or log directory on iSH).

---

### `chrono`

Timestamp formatting. Used to add `[HH:MM]` timestamps to displayed
messages and ISO 8601 timestamps to log file entries.

---

### `base58`

Compact encoding for the room code. Base58 avoids visually ambiguous
characters (0, O, I, l) that cause copy-paste errors in terminals.

---

### `rand`

Cryptographically secure random number generation. Used to generate the
12-byte nonce for each AES-256-GCM encryption operation.

The `rand` crate delegates to the OS CSPRNG (`getrandom` under the hood),
which is the correct source of randomness for cryptographic nonces.

---

## Development Dependencies

### `tracing` + `tracing-subscriber`

Structured logging for development and debugging. Logs are written to
stderr so they do not interfere with the crossterm terminal output on
stdout. Disabled or minimized in release builds.

---

## Platform Notes

### iSH (iPhone)

iSH runs an Alpine Linux environment using x86 emulation. All dependencies
are pure Rust or link against standard libc, so they compile and run on
iSH without modification. Compile on a PC and transfer the binary, or
compile directly inside iSH (slower due to emulation).

### Windows

crossterm handles Windows console API differences transparently.
libp2p TCP transport works on Windows. No platform-specific code is
required in the application layer.

---

## What Is Deliberately Excluded

| What | Why excluded |
|------|-------------|
| TLS / HTTPS | Noise protocol at transport layer covers authentication |
| SQLite / any database | Plain text log files are sufficient for v1 |
| Clap / argument parser | Minimal CLI needs only a simple interactive menu |
| Tokio-console | Development tool only; not a runtime dependency |
| Any GUI toolkit | Out of scope by design |
