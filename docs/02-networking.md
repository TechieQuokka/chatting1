# Networking Design

## Why libp2p

libp2p is a modular peer-to-peer networking framework originally built for
IPFS. It solves the three hard problems of internet P2P in one integrated
stack: transport abstraction, NAT traversal, and peer discovery.
BitTorrent solves the same problems with a similar architecture (tracker +
DHT + hole punching). libp2p provides all of this as composable Rust crates.

---

## Transport Stack

Connections between peers are established through a layered transport stack:

```
TCP (raw bytes)
  └─► Noise Protocol (authenticated encryption of the connection)
        └─► Yamux (multiplexing multiple logical streams over one connection)
```

- **TCP**: The base transport. Reliable, universally supported.
- **Noise**: Performs a cryptographic handshake so both peers authenticate
  each other's Peer ID. Prevents man-in-the-middle attacks at the transport
  level. This is distinct from message-level encryption (see `05-encryption.md`).
- **Yamux**: Allows many independent logical streams (e.g., one for Kademlia,
  one for GossipSub) to share a single TCP connection.

---

## Peer Identity

Each peer owns an Ed25519 key pair. The public key is hashed to produce a
**Peer ID** — a globally unique, self-certifying identifier. No registration
server is required.

When two peers connect, the Noise handshake proves that each peer controls
the private key corresponding to their Peer ID.

---

## Peer Discovery

Two complementary mechanisms run simultaneously:

### mDNS (Local Network Discovery)

mDNS broadcasts a query on the local subnet. Any peer running the same
application responds with its multiaddress. This works without internet
access and requires no configuration. Useful for same-WiFi testing.

### Kademlia DHT (Internet Discovery)

Kademlia is a distributed hash table that allows peers to locate other peers
without a central server.

**Bootstrap process:**
1. On startup, the peer connects to a set of well-known public bootstrap nodes.
   The application ships with the same bootstrap node addresses used by IPFS,
   which are stable and globally distributed.
2. The peer announces itself by inserting its Peer ID into the DHT.
3. The peer can then look up other peers subscribed to a given GossipSub topic.

Bootstrap nodes serve only as entry points into the DHT. They do not relay
chat messages and have no knowledge of room contents.

---

## NAT Traversal

Most devices sit behind a NAT router. Direct TCP connections from the internet
are blocked by default. libp2p solves this with two complementary mechanisms:

### Circuit Relay v2

When a direct connection cannot be established, a third peer (relay) forwards
traffic between the two parties. Public libp2p nodes act as relays.

```
Peer A ──► Relay Node ──► Peer B
           (public IP)
```

This always works but is slower than a direct connection.

### DCUtR — Direct Connection Upgrade through Relay

After establishing a relayed connection, DCUtR attempts to upgrade it to
a direct connection using **UDP hole punching**:

1. Both peers exchange their observed public addresses through the relay.
2. Both peers send packets to each other simultaneously, punching holes
   in their respective NAT tables.
3. If successful, the relay connection is dropped and peers communicate
   directly.

```
Phase 1 (relay):  A ──► Relay ──► B
Phase 2 (upgrade): A ─────────────► B (direct)
```

This mirrors exactly how BitTorrent achieves direct peer connections.

---

## GossipSub — Room Messaging

GossipSub is a publish-subscribe protocol designed for libp2p. It provides
efficient message propagation across a mesh of peers.

**Topic naming convention:**

```
/chatapp/v1/rooms/{room-name}
```

Each room corresponds to one GossipSub topic. Subscribing to a topic is
equivalent to entering a room. Publishing to a topic sends a message to
all subscribers currently in the mesh for that topic.

**Message flow:**

```
Sender:    plaintext → encrypt → publish to topic
Receiver:  receive from topic → decrypt → display
```

GossipSub does not guarantee message ordering or delivery to offline peers.
Messages are live-only (persistence is handled by the application layer).

---

## Room Code

When a peer creates a room, the application generates a **room code** that
encodes enough information for another peer to bootstrap a connection:

- The creator's Peer ID
- The creator's current multiaddress (IP + port)
- The room name

The code is a compact Base58-encoded string. The user shares it out-of-band
(e.g., via a messaging app). The joining peer decodes the room code, dials
the creator directly, and then discovers additional room members through
GossipSub's mesh gossip.

---

## Peer Lifecycle

```
Start
  │
  ├─► Generate / load key pair
  ├─► Start TCP listener on a random port
  ├─► Connect to IPFS bootstrap nodes
  ├─► Start mDNS
  │
  ├─[Create room]─► Subscribe to topic → generate room code → wait
  │
  ├─[Join room]──► Decode room code → dial creator → subscribe to topic
  │                └─► attempt DCUtR upgrade if behind NAT
  │
  ├─[In room]────► Publish encrypted messages / receive and decrypt
  │
  └─[Quit]───────► Unsubscribe → close swarm → exit
```
