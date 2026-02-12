use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    time::Duration,
};

use anyhow::{Context, Result};
use libp2p::{
    dcutr, gossipsub, identify, kad, mdns, noise, relay, swarm::NetworkBehaviour, tcp, yamux,
    Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::types::{NetworkCommand, NetworkEvent};

// ── Bootstrap peers (IPFS public nodes) ──────────────────────────────────────

const BOOTSTRAP_PEERS: &[(&str, &str)] = &[
    (
        "/dnsaddr/bootstrap.libp2p.io",
        "QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    ),
    (
        "/dnsaddr/bootstrap.libp2p.io",
        "QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
    ),
    (
        "/dnsaddr/bootstrap.libp2p.io",
        "QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
    ),
    (
        "/dnsaddr/bootstrap.libp2p.io",
        "QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
    ),
];

// ── Combined NetworkBehaviour ─────────────────────────────────────────────────

#[derive(NetworkBehaviour)]
struct ChatBehaviour {
    gossipsub: gossipsub::Behaviour,
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
    mdns: mdns::tokio::Behaviour,
    relay_client: relay::client::Behaviour,
    dcutr: dcutr::Behaviour,
    identify: identify::Behaviour,
}

// ── NetworkService ────────────────────────────────────────────────────────────

pub struct NetworkService {
    swarm: Swarm<ChatBehaviour>,
    event_tx: mpsc::UnboundedSender<NetworkEvent>,
    cmd_rx: mpsc::UnboundedReceiver<NetworkCommand>,
}

impl NetworkService {
    /// Build the swarm and return:
    /// * the `NetworkService` (to be driven via `run()`)
    /// * a receiver for network events
    /// * a sender for network commands
    pub fn new(
        keypair: libp2p::identity::Keypair,
    ) -> Result<(
        Self,
        mpsc::UnboundedReceiver<NetworkEvent>,
        mpsc::UnboundedSender<NetworkCommand>,
    )> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let local_peer_id = PeerId::from(keypair.public());
        info!("Local peer id: {local_peer_id}");

        let swarm = SwarmBuilder::with_existing_identity(keypair.clone())
            .with_tokio()
            .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)
            .context("TCP transport setup")?
            .with_dns()
            .context("DNS transport setup")?
            .with_relay_client(noise::Config::new, yamux::Config::default)
            .context("Relay client setup")?
            .with_behaviour(|key, relay_client| {
                // ── GossipSub ──────────────────────────────────────────
                let msg_id_fn = |msg: &gossipsub::Message| {
                    let mut hasher = DefaultHasher::new();
                    msg.data.hash(&mut hasher);
                    gossipsub::MessageId::from(hasher.finish().to_string())
                };
                let gossipsub_config = gossipsub::ConfigBuilder::default()
                    .heartbeat_interval(Duration::from_secs(10))
                    .validation_mode(gossipsub::ValidationMode::Strict)
                    .message_id_fn(msg_id_fn)
                    .build()
                    .expect("valid gossipsub config");

                let gossipsub = gossipsub::Behaviour::new(
                    gossipsub::MessageAuthenticity::Signed(key.clone()),
                    gossipsub_config,
                )
                .expect("valid gossipsub behaviour");

                // ── Kademlia ───────────────────────────────────────────
                let mut kademlia = kad::Behaviour::new(
                    local_peer_id,
                    kad::store::MemoryStore::new(local_peer_id),
                );
                kademlia.set_mode(Some(kad::Mode::Server));
                for (addr_str, pid_str) in BOOTSTRAP_PEERS {
                    if let (Ok(addr), Ok(pid)) = (
                        addr_str.parse::<Multiaddr>(),
                        pid_str.parse::<PeerId>(),
                    ) {
                        kademlia.add_address(&pid, addr);
                    }
                }

                // ── mDNS ───────────────────────────────────────────────
                let mdns = mdns::tokio::Behaviour::new(
                    mdns::Config::default(),
                    local_peer_id,
                )
                .expect("valid mdns behaviour");

                // ── DCUtR & Identify ───────────────────────────────────
                let dcutr = dcutr::Behaviour::new(local_peer_id);
                let identify = identify::Behaviour::new(identify::Config::new(
                    "/chatapp/0.1.0".to_string(),
                    key.public(),
                ));

                Ok(ChatBehaviour {
                    gossipsub,
                    kademlia,
                    mdns,
                    relay_client,
                    dcutr,
                    identify,
                })
            })
            .context("Behaviour setup")?
            .with_swarm_config(|c| {
                c.with_idle_connection_timeout(Duration::from_secs(60))
            })
            .build();

        Ok((
            Self { swarm, event_tx, cmd_rx },
            event_rx,
            cmd_tx,
        ))
    }

    /// Drive the swarm — call this inside a dedicated Tokio task.
    pub async fn run(mut self) {
        // Start listening on a random TCP port.
        self.swarm
            .listen_on("/ip4/0.0.0.0/tcp/0".parse().expect("valid multiaddr"))
            .expect("listen_on succeeded");

        // Kick off DHT bootstrap.
        let _ = self.swarm.behaviour_mut().kademlia.bootstrap();

        loop {
            tokio::select! {
                // ── Inbound swarm event ───────────────────────────────
                event = self.swarm.next() => {
                    match event {
                        Some(e) => self.handle_swarm_event(e),
                        None => break,
                    }
                }

                // ── Outbound command from app ─────────────────────────
                Some(cmd) = self.cmd_rx.recv() => {
                    self.handle_command(cmd);
                }
            }
        }
    }

    fn handle_swarm_event(&mut self, event: libp2p::swarm::SwarmEvent<ChatBehaviourEvent>) {
        use libp2p::swarm::SwarmEvent;
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("Listening on {address}");
                let _ = self
                    .event_tx
                    .send(NetworkEvent::ListeningOn(address.to_string()));
            }

            SwarmEvent::ExternalAddrConfirmed { address } => {
                info!("External address confirmed: {address}");
                let _ = self
                    .event_tx
                    .send(NetworkEvent::NewExternalAddr(address.to_string()));
            }

            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                debug!("Connected: {peer_id}");
                let _ = self.event_tx.send(NetworkEvent::PeerConnected);
            }

            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                debug!("Disconnected: {peer_id}");
                let _ = self
                    .event_tx
                    .send(NetworkEvent::PeerDisconnected(peer_id.to_string()));
            }

            SwarmEvent::Behaviour(behaviour_event) => {
                self.handle_behaviour_event(behaviour_event);
            }

            _ => {}
        }
    }

    fn handle_behaviour_event(&mut self, event: ChatBehaviourEvent) {
        match event {
            // ── GossipSub ─────────────────────────────────────────────
            ChatBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                message, ..
            }) => {
                let _ = self.event_tx.send(NetworkEvent::MessageReceived {
                    topic: message.topic.to_string(),
                    payload: message.data,
                });
            }

            ChatBehaviourEvent::Gossipsub(gossipsub::Event::Subscribed { peer_id, topic }) => {
                let _ = self.event_tx.send(NetworkEvent::PeerSubscribed {
                    topic: topic.to_string(),
                    peer_id: peer_id.to_string(),
                });
            }

            ChatBehaviourEvent::Gossipsub(gossipsub::Event::Unsubscribed { peer_id, .. }) => {
                let _ = self
                    .event_tx
                    .send(NetworkEvent::PeerDisconnected(peer_id.to_string()));
            }

            // ── Kademlia ──────────────────────────────────────────────
            ChatBehaviourEvent::Kademlia(kad::Event::OutboundQueryProgressed {
                result: kad::QueryResult::Bootstrap(Ok(_)),
                ..
            }) => {
                info!("Kademlia bootstrap complete");
            }

            // ── mDNS ──────────────────────────────────────────────────
            ChatBehaviourEvent::Mdns(mdns::Event::Discovered(peers)) => {
                for (peer_id, addr) in peers {
                    debug!("mDNS discovered: {peer_id} @ {addr}");
                    self.swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(&peer_id, addr);
                    self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .add_explicit_peer(&peer_id);
                }
            }

            ChatBehaviourEvent::Mdns(mdns::Event::Expired(peers)) => {
                for (peer_id, _) in peers {
                    self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .remove_explicit_peer(&peer_id);
                }
            }

            // ── Identify ──────────────────────────────────────────────
            ChatBehaviourEvent::Identify(identify::Event::Received {
                peer_id, info, ..
            }) => {
                for addr in info.listen_addrs {
                    self.swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(&peer_id, addr);
                }
            }

            // ── DCUtR ─────────────────────────────────────────────────
            ChatBehaviourEvent::Dcutr(e) => {
                info!("DCUtR event: {:?}", e);
            }

            _ => {}
        }
    }

    fn handle_command(&mut self, cmd: NetworkCommand) {
        match cmd {
            NetworkCommand::Subscribe(topic_str) => {
                let topic = gossipsub::IdentTopic::new(&topic_str);
                if let Err(e) = self.swarm.behaviour_mut().gossipsub.subscribe(&topic) {
                    warn!("Subscribe error: {e}");
                }
            }

            NetworkCommand::Unsubscribe(topic_str) => {
                let topic = gossipsub::IdentTopic::new(&topic_str);
                let _ = self.swarm.behaviour_mut().gossipsub.unsubscribe(&topic);
            }

            NetworkCommand::Publish { topic: topic_str, data } => {
                let topic = gossipsub::IdentTopic::new(&topic_str);
                if let Err(e) = self.swarm.behaviour_mut().gossipsub.publish(topic, data) {
                    warn!("Publish error: {e}");
                }
            }

            NetworkCommand::Dial(addr_str) => {
                if let Ok(addr) = addr_str.parse::<Multiaddr>() {
                    if let Err(e) = self.swarm.dial(addr) {
                        warn!("Dial error: {e}");
                    }
                } else {
                    warn!("Invalid multiaddr: {addr_str}");
                }
            }

            NetworkCommand::QueryListenAddrs => {
                for addr in self.swarm.listeners() {
                    let _ = self
                        .event_tx
                        .send(NetworkEvent::ListeningOn(addr.to_string()));
                }
            }
        }
    }
}

// Needed to drive the swarm in a loop (from `futures::StreamExt`).
use futures::StreamExt;
