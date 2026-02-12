# Architecture Design

## Layer Overview

The application is divided into four clearly separated layers.
Each layer has a single responsibility and communicates only with adjacent layers.

```
┌──────────────────────────────────────────────────────────────┐
│  Layer 4 — CLI Layer                                         │
│  Responsibility: Render output, capture input                │
│  Knows about: Application Layer events and commands         │
│  Does not know about: Encryption, Networking                 │
├──────────────────────────────────────────────────────────────┤
│  Layer 3 — Application Layer                                 │
│  Responsibility: Orchestrate all domain logic                │
│  Knows about: All layers below                               │
│  Does not know about: Terminal rendering specifics           │
├──────────────────────────────────────────────────────────────┤
│  Layer 2 — Encryption Layer                                  │
│  Responsibility: Derive keys, encrypt, decrypt               │
│  Knows about: Cryptographic primitives only                  │
│  Does not know about: Networking, CLI                        │
├──────────────────────────────────────────────────────────────┤
│  Layer 1 — Networking Layer                                  │
│  Responsibility: Send and receive raw byte payloads          │
│  Knows about: libp2p protocols                               │
│  Does not know about: Message content or encryption          │
└──────────────────────────────────────────────────────────────┘
```

---

## Layer 1 — Networking Layer

Wraps libp2p and exposes two simple abstractions to the layer above:

- **Publish**: send a byte payload to a named topic (room)
- **Subscribe**: receive byte payloads from a named topic (room)
- **Peer events**: notify when peers join or leave the local mesh

The layer is responsible for:
- Starting the libp2p swarm
- Bootstrapping into the Kademlia DHT via IPFS public nodes
- Performing mDNS discovery on the local network
- Negotiating NAT traversal via Circuit Relay and DCUtR hole punching
- Managing GossipSub topic subscriptions

The layer is **not** responsible for what the payloads contain.

---

## Layer 2 — Encryption Layer

All message confidentiality lives here. This layer is a pure transformation:
plaintext + key → ciphertext, or ciphertext + key → plaintext (or error).

Responsibilities:
- Derive a symmetric key from a room password using Argon2id
- Encrypt outgoing message payloads with AES-256-GCM
- Decrypt incoming payloads; surface a clear error on authentication failure
- Produce and verify the room's **verification token** (used for password
  checking at join time — see `03-rooms.md`)

This layer has no I/O. It is a stateless set of pure functions.

---

## Layer 3 — Application Layer

The brain of the application. It owns all mutable state and coordinates
the other layers.

State owned here:
- Local peer identity (key pair, derived Peer ID)
- Active room name and derived encryption key
- Nickname and discriminator
- In-memory message buffer (for redrawing the terminal)
- Log file handle

Responsibilities:
- Read the config file on startup; prompt for nickname on first run
- Handle user commands (create room, join room, send message, quit)
- Route outgoing messages: encrypt → publish via networking layer
- Route incoming messages: decrypt → validate → append to buffer and log
- Emit display events to the CLI layer

---

## Layer 4 — CLI Layer

Renders the split-pane terminal interface and forwards raw keystrokes to
the application layer as structured commands.

Responsibilities:
- Draw the message pane (scrollable, newest at bottom)
- Draw the input bar (fixed at the bottom of the terminal)
- Handle terminal resize events and redraw accordingly
- Map keystrokes to application commands (Enter to send, Ctrl-C to quit, etc.)

This layer contains **no business logic**. It only renders state provided
by the application layer and forwards input.

---

## Concurrency Model

The application uses Tokio for async I/O. Three concurrent tasks run
throughout the session:

| Task | Role |
|------|------|
| Swarm task | Drives the libp2p event loop |
| Input task | Reads keystrokes from stdin without blocking |
| Render task | Redraws the terminal when the message buffer changes |

Tasks communicate through Tokio channels (mpsc and broadcast).
No shared mutable state crosses task boundaries — all state mutation
happens inside the application layer, serialized through channel messages.

---

## Startup Sequence

```
1. Load or create config file (~/.chatrc)
2. Load or generate Ed25519 key pair (stored in config)
3. If no nickname exists → prompt user → save to config
4. Start libp2p swarm (transport + protocols)
5. Bootstrap into Kademlia DHT
6. Start mDNS listener
7. Launch CLI render loop
8. Present main menu: [C]reate room / [J]oin room / [Q]uit
```

---

## Shutdown Sequence

```
1. User presses Ctrl-C or types /quit
2. Unsubscribe from GossipSub topic
3. Flush and close log file
4. Gracefully stop the swarm
5. Restore terminal state (crossterm cleanup)
6. Exit process
```
