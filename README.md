# P2P CLI Chat

A fully peer-to-peer, end-to-end encrypted chat application for the terminal, built in Rust.

No central server. No accounts. No sign-up.

## Features

- **Serverless P2P** — peers connect directly over the internet using [libp2p](https://libp2p.io/)
- **End-to-end encryption** — all messages are encrypted with AES-256-GCM; room passwords never leave your machine
- **NAT traversal** — works behind home routers via Circuit Relay v2 and DCUtR hole punching
- **Local network discovery** — mDNS automatically finds peers on the same Wi-Fi, no configuration needed
- **Split-pane TUI** — header + scrollable message area + input bar, built with crossterm
- **Password masking** — password input is hidden behind `•` characters
- **Message persistence** — every session is appended to a plain-text log file in `~/.chat_logs/`
- **Cross-platform** — Linux, macOS, Windows, iSH (iPhone), Termux (Android)

## Quick Start

### Prerequisites

- Rust toolchain (stable, edition 2024) — install via [rustup](https://rustup.rs/)

### Build and Run

```bash
git clone <repo-url>
cd chatting1
cargo build --release
./target/release/chat
```

On first run you will be prompted for a nickname. It is saved to `~/.chatrc` and reused on every subsequent launch.

### Create a Room

```
=== P2P Chat ===
[1] Create room
[2] Join room
[Q] Quit
> 1

Room name: rust-chat
Password (leave blank for none): ••••••••

Room 'rust-chat' created. Share this code: 7xKpQm3NvBsRtYdEfGhJ2cLwAoP9uXiZ
```

Share the room code with others via any out-of-band channel (text message, email, etc.).

### Join a Room

```
> 2

Room code: 7xKpQm3NvBsRtYdEfGhJ2cLwAoP9uXiZ
Password (leave blank for none): ••••••••

Joined room 'rust-chat'
```

### In-Room Commands

| Command  | Action                                       |
|----------|----------------------------------------------|
| `/quit`  | Leave the room and return to the main menu   |
| `/peers` | List currently connected peer nicknames      |
| `/help`  | Print the command list                       |

Any input that does not start with `/` is sent as a chat message.

**Keyboard shortcuts:**

| Key        | Behavior                          |
|------------|-----------------------------------|
| `Enter`    | Send message / confirm input      |
| `Ctrl-C`   | Quit current context              |
| `Backspace`| Delete last character             |

## Terminal Layout

```
┌──────────────────────────────────────────────────────────────┐
│  Room: rust-chat                      3 peers online         │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  [14:30] Seung#3f2a: hello everyone                          │
│  [14:31] Mike#7b1c: hey!                                     │
│  [14:33] *** Alice#9d4e joined the room                      │
│  [14:34] Mike#7b1c: welcome Alice                            │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│  > type your message here...                                 │
└──────────────────────────────────────────────────────────────┘
```

## Architecture

The application is divided into four layers, each with a single responsibility:

```
┌──────────────────────────────────────────────────────────────┐
│  CLI Layer (crossterm)                                       │
│  Render output, capture keystrokes                           │
├──────────────────────────────────────────────────────────────┤
│  Application Layer                                           │
│  Room management, message routing, state orchestration       │
├──────────────────────────────────────────────────────────────┤
│  Encryption Layer                                            │
│  Argon2id key derivation · AES-256-GCM encrypt/decrypt       │
├──────────────────────────────────────────────────────────────┤
│  Networking Layer (libp2p)                                   │
│  TCP · Noise · Yamux · GossipSub · Kademlia · mDNS           │
│  Circuit Relay v2 · DCUtR hole punching                      │
└──────────────────────────────────────────────────────────────┘
```

Three Tokio tasks run concurrently and communicate through channels:

| Task            | Role                                   |
|-----------------|----------------------------------------|
| Network task    | Drives the libp2p swarm event loop     |
| Application task| Owns all state, routes messages        |
| CLI task        | Owns the terminal, renders the TUI     |

No shared mutable state crosses task boundaries.

## Identity

On first run an Ed25519 key pair is generated and saved to `~/.chatrc`. The public key is hashed to produce a **Peer ID** — a self-certifying, globally unique identifier requiring no registration.

Nicknames are displayed with a 4-character hex **discriminator** derived from the Peer ID:

```
Seung#3f2a
```

Two users with the same nickname are distinguished at a glance.

## Encryption

| Step              | Algorithm        | Details                                        |
|-------------------|-----------------|------------------------------------------------|
| Key derivation    | Argon2id         | password + room name as salt → 256-bit key     |
| Message encryption| AES-256-GCM      | random 12-byte nonce per message (OS CSPRNG)   |
| Wire format       | —                | `nonce (12 B) ++ ciphertext ++ tag`            |

Passwords are never transmitted. Each peer derives the room key independently from the password they enter locally.

**Password verification** — when a new peer joins, existing room members publish an encrypted verification token. The joiner attempts to decrypt it:
- Decryption succeeds → correct password → enter room
- Decryption fails → wrong password → "Access denied" → return to menu

**Security scope (v1):**
- Provides message confidentiality and integrity
- Does not provide forward secrecy, anonymity at the IP layer, or access revocation

## Networking

Peer discovery uses two complementary mechanisms:

- **mDNS** — zero-configuration discovery on the local subnet
- **Kademlia DHT** — internet-wide discovery via IPFS bootstrap nodes

NAT traversal:

- **Circuit Relay v2** — traffic is forwarded through a public relay when a direct connection is not possible
- **DCUtR** — after a relayed connection is established, UDP hole punching attempts to upgrade it to a direct connection

Room codes encode the creator's Peer ID, multiaddress, and room name in a compact Base58 string safe to share over any channel.

## File Layout

```
~/.chatrc          — config (nickname, private key, log dir)
~/.chat_logs/      — per-room message logs
  rust-chat.log
  general.log
```

Logs are plain UTF-8 text, one event per line:

```
[2026-02-12T14:32:05Z] Seung#3f2a: hello everyone
[2026-02-12T14:33:00Z] *** Alice#9d4e joined the room
```

## Source Layout

```
src/
  main.rs       — entry point, task wiring
  app.rs        — application state and event loop
  cli.rs        — TUI rendering and input handling (crossterm)
  network.rs    — libp2p swarm setup and event dispatch
  crypto.rs     — Argon2id key derivation, AES-256-GCM encrypt/decrypt
  identity.rs   — Ed25519 keypair, Peer ID, discriminator
  config.rs     — ~/.chatrc load/save (TOML)
  room.rs       — room state, topic naming, room code encode/decode
  logger.rs     — append-only per-room log files
  types.rs      — shared types (WireMessage, NetworkEvent, UiEvent, CliCommand)
```

## Dependencies

| Crate                     | Purpose                                       |
|---------------------------|-----------------------------------------------|
| `libp2p`                  | P2P transport, discovery, pub/sub, NAT        |
| `tokio`                   | Async runtime                                 |
| `crossterm`               | Cross-platform terminal manipulation          |
| `aes-gcm`                 | AES-256-GCM authenticated encryption          |
| `argon2`                  | Password-based key derivation (Argon2id)      |
| `serde` / `serde_json`    | Message serialization                         |
| `toml`                    | Config file format                            |
| `chrono`                  | Timestamp formatting                          |
| `bs58`                    | Base58 room code encoding                     |
| `rand`                    | OS CSPRNG for nonce generation                |
| `tracing`                 | Structured logging to stderr                  |

## Platform Notes

| Platform       | Environment                          |
|----------------|--------------------------------------|
| Linux / macOS  | Native terminal                      |
| Windows        | CMD / PowerShell (crossterm handles Win32 console) |
| iPhone         | [iSH](https://ish.app/) — Alpine Linux x86 emulator |
| Android        | Termux                               |

All dependencies are pure Rust or link against standard libc. Cross-compilation to iSH is possible; alternatively, compile directly inside iSH (slower due to x86 emulation).

## Limitations (v1)

- No message history for late joiners — only messages received after joining are visible
- No in-session nickname change — edit `~/.chatrc` and restart
- No log rotation — log files grow indefinitely
- No forward secrecy — the same room key is used for the full session
- No access revocation — any peer with the password stays able to join
- Integration tests (cross-device, NAT traversal) pending
