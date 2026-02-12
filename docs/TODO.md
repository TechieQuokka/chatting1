# Implementation TODO

## Phase 1 — Project Setup
- [x] Initialize Cargo.toml with all dependencies
- [x] Define workspace structure (`src/` modules)
- [x] Set up tracing/logging for development

## Phase 2 — Identity & Config
- [x] Config file reader/writer (`~/.chatrc`, TOML)
- [x] Ed25519 key pair generation and persistence
- [x] Nickname prompt on first run + save to config
- [x] Discriminator derivation from Peer ID (first 4 hex chars)

## Phase 3 — Encryption Layer
- [x] Argon2id key derivation (password + room name as salt)
- [x] AES-256-GCM encrypt function
- [x] AES-256-GCM decrypt function
- [x] Verification token generation
- [x] Verification token validation

## Phase 4 — Networking Layer
- [x] libp2p swarm setup (TCP + Noise + Yamux)
- [x] GossipSub configuration and topic subscription
- [x] Kademlia DHT setup + IPFS bootstrap nodes
- [x] mDNS local discovery
- [x] Circuit Relay v2 setup
- [x] DCUtR hole punching setup
- [x] Room code encode (Base58)
- [x] Room code decode (Base58)

## Phase 5 — Application Layer
- [x] Message payload struct (sender, discriminator, timestamp, text)
- [x] Outgoing message pipeline (serialize → encrypt → publish)
- [x] Incoming message pipeline (receive → decrypt → deserialize)
- [x] Room create flow
- [x] Room join flow (with password verification)
- [x] Room leave flow
- [x] Peer join/leave event handling
- [x] Tokio channel wiring (swarm ↔ app ↔ cli)

## Phase 6 — CLI Layer
- [x] crossterm raw mode setup and teardown
- [x] Split-pane layout (header + message pane + input bar)
- [x] Message pane rendering and scroll
- [x] Input bar with keystroke capture
- [x] Password input masking (`•`)
- [x] Terminal resize handling
- [x] Main menu rendering
- [x] In-room command parsing (`/quit`, `/peers`, `/help`)

## Phase 7 — Persistence
- [x] Log directory creation (`~/.chat_logs/`)
- [x] Log file open/close per room session
- [x] Append chat messages to log
- [x] Append system events to log

## Phase 8 — Integration & Testing
- [ ] PC ↔ PC local network test (mDNS)
- [ ] PC ↔ PC internet test (DHT + room code)
- [ ] PC ↔ iPhone (iSH) test
- [ ] Wrong password → access denied test
- [ ] NAT traversal test (DCUtR)
- [ ] Graceful shutdown test (Ctrl-C)
