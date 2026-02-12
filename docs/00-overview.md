# P2P CLI Chat Application — Design Overview

## Project Summary

A fully peer-to-peer command-line chat application built in Rust.
No central message broker. No persistent server. Peers communicate directly
over the internet using libp2p as the networking foundation.

## Core Design Philosophy

- **Serverless by nature**: No machine is privileged. Any peer can create or
  join a room without registering with a central authority.
- **Encryption first**: Room passwords are never transmitted. They are used
  locally to derive symmetric keys for message encryption and decryption.
- **Identity without accounts**: A peer's identity is a cryptographic key pair
  generated locally. No sign-up, no email, no server-side user record.
- **Minimal surface area**: The application does exactly what a chat app needs
  to do — discover peers, join rooms, send and receive encrypted messages.

## Target Platforms

| Platform | Runtime Environment |
|----------|---------------------|
| Linux / macOS | Native terminal |
| Windows | Native terminal (CMD / PowerShell) |
| iPhone | iSH (Alpine Linux emulator, free App Store app) |
| Android | Termux |

## High-Level Architecture

```
┌─────────────────────────────────────────────────────┐
│                   CLI Layer                         │
│  (crossterm split-pane: message area + input bar)   │
└───────────────────┬─────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────┐
│                Application Layer                    │
│  (room management, nickname, password verification, │
│   message formatting, local log persistence)        │
└───────────────────┬─────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────┐
│               Encryption Layer                      │
│  (Argon2 key derivation, AES-256-GCM encrypt/       │
│   decrypt, verification token logic)                │
└───────────────────┬─────────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────────┐
│              Networking Layer (libp2p)               │
│  Transport → Noise → Yamux → GossipSub              │
│  Kademlia DHT + mDNS + Circuit Relay + DCUtR        │
└─────────────────────────────────────────────────────┘
```

## Document Index

| File | Contents |
|------|----------|
| `01-architecture.md` | Layer-by-layer architecture detail |
| `02-networking.md` | libp2p component selection and peer lifecycle |
| `03-rooms.md` | Room creation, discovery, and password design |
| `04-identity.md` | Peer identity, nickname, and discriminator system |
| `05-encryption.md` | Cryptographic design for message confidentiality |
| `06-cli-ux.md` | Terminal layout and user interaction flow |
| `07-persistence.md` | Local message log design |
| `08-crate-manifest.md` | Dependency rationale and version strategy |
