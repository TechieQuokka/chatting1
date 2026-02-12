use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::{
    config::Config,
    crypto::RoomKey,
    identity::Identity,
    logger::Logger,
    room::{topic_for_room, RoomCodeData, RoomState},
    types::{
        CliCommand, DisplayMessage, NetworkCommand, NetworkEvent, UiEvent, WireMessage,
        WireMessageType,
    },
};

pub struct App {
    identity: Identity,
    config: Config,

    // Active room state (None when in menu)
    room: Option<RoomState>,
    room_key: Option<RoomKey>,
    logger: Option<Logger>,

    // Peer tracking: gossipsub peer_id string → display name (if known)
    peers: HashMap<String, String>,

    // Listen addresses gathered from the network layer
    listen_addrs: Vec<String>,

    // Pending password verification: waiting for a VerificationToken message
    pending_verify: Option<PendingVerify>,

    // Channels
    net_event_rx: mpsc::UnboundedReceiver<NetworkEvent>,
    net_cmd_tx: mpsc::UnboundedSender<NetworkCommand>,
    cli_cmd_rx: mpsc::UnboundedReceiver<CliCommand>,
    ui_event_tx: mpsc::UnboundedSender<UiEvent>,
}

struct PendingVerify {
    room_name: String,
    room_key: RoomKey,
    deadline: tokio::time::Instant,
}

impl App {
    pub fn new(
        identity: Identity,
        config: Config,
        net_event_rx: mpsc::UnboundedReceiver<NetworkEvent>,
        net_cmd_tx: mpsc::UnboundedSender<NetworkCommand>,
        cli_cmd_rx: mpsc::UnboundedReceiver<CliCommand>,
        ui_event_tx: mpsc::UnboundedSender<UiEvent>,
    ) -> Self {
        Self {
            identity,
            config,
            room: None,
            room_key: None,
            logger: None,
            peers: HashMap::new(),
            listen_addrs: Vec::new(),
            pending_verify: None,
            net_event_rx,
            net_cmd_tx,
            cli_cmd_rx,
            ui_event_tx,
        }
    }

    /// Main event loop — runs until the CLI sends `Quit`.
    pub async fn run(mut self) -> Result<()> {
        // Ask network layer to report its listen addresses.
        let _ = self.net_cmd_tx.send(NetworkCommand::QueryListenAddrs);

        loop {
            // Verification timeout check interval
            let timeout = tokio::time::sleep(Duration::from_millis(500));

            tokio::select! {
                // CLI command from the user
                Some(cmd) = self.cli_cmd_rx.recv() => {
                    match self.handle_cli_command(cmd).await {
                        Ok(true) => break,   // Quit requested
                        Ok(false) => {}
                        Err(e) => {
                            let _ = self.ui_event_tx.send(UiEvent::Error(e.to_string()));
                        }
                    }
                }

                // Event from network layer
                Some(event) = self.net_event_rx.recv() => {
                    if let Err(e) = self.handle_network_event(event).await {
                        warn!("Network event error: {e}");
                    }
                }

                // Verification timeout
                _ = timeout => {
                    self.check_verify_timeout();
                }
            }
        }
        Ok(())
    }

    // ── CLI commands ──────────────────────────────────────────────────────────

    /// Returns `Ok(true)` to signal quit.
    async fn handle_cli_command(&mut self, cmd: CliCommand) -> Result<bool> {
        match cmd {
            CliCommand::Quit => return Ok(true),

            CliCommand::SendMessage(text) => {
                self.send_message(text).await?;
            }

            CliCommand::CreateRoom { name, password } => {
                self.create_room(name, password).await?;
            }

            CliCommand::JoinRoom { code, password } => {
                self.join_room(code, password).await?;
            }

            CliCommand::LeaveRoom => {
                self.leave_room().await?;
            }

            CliCommand::ListPeers => {
                let list = if self.peers.is_empty() {
                    "No peers connected.".to_string()
                } else {
                    self.peers
                        .values()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let msg = DisplayMessage::system(&format!("Peers: {}", list));
                let _ = self.ui_event_tx.send(UiEvent::NewMessage(msg));
            }

            CliCommand::Help => {
                let help = concat!(
                    "/quit   — leave room / exit\n",
                    "/peers  — list connected peers\n",
                    "/help   — show this message"
                );
                for line in help.lines() {
                    let msg = DisplayMessage::system(line);
                    let _ = self.ui_event_tx.send(UiEvent::NewMessage(msg));
                }
            }
        }
        Ok(false)
    }

    // ── Room operations ───────────────────────────────────────────────────────

    async fn create_room(&mut self, name: String, password: String) -> Result<()> {
        self.leave_room().await?;

        let room_key = RoomKey::derive(&password, &name)?;
        let topic = topic_for_room(&name);

        // Subscribe to the GossipSub topic.
        let _ = self.net_cmd_tx.send(NetworkCommand::Subscribe(topic.clone()));

        // Open log file.
        self.config.ensure_log_dir()?;
        let logger = Logger::open(&self.config.log_dir, &name)?;

        // Build room code (include first available listen address).
        let addr = self
            .listen_addrs
            .first()
            .cloned()
            .unwrap_or_default();

        let code_data = RoomCodeData {
            room_name: name.clone(),
            peer_id: self.identity.peer_id.to_string(),
            addr,
        };
        let code = code_data.encode().unwrap_or_default();

        // Update state.
        let mut room_state = RoomState::new(&name);
        room_state.peer_count = 1;
        self.room = Some(room_state);
        self.room_key = Some(room_key);
        self.logger = Some(logger);

        let _ = self
            .ui_event_tx
            .send(UiEvent::RoomCreated { name, code });

        self.emit_status();
        Ok(())
    }

    async fn join_room(&mut self, code: String, password: String) -> Result<()> {
        self.leave_room().await?;

        let code_data = RoomCodeData::decode(&code)?;
        let room_name = code_data.room_name.clone();
        let room_key = RoomKey::derive(&password, &room_name)?;
        let topic = topic_for_room(&room_name);

        // Dial the room creator if we have their address.
        if !code_data.addr.is_empty() {
            let _ = self
                .net_cmd_tx
                .send(NetworkCommand::Dial(code_data.addr.clone()));
        }

        // Subscribe to the GossipSub topic.
        let _ = self.net_cmd_tx.send(NetworkCommand::Subscribe(topic));

        // Open log file.
        self.config.ensure_log_dir()?;
        let logger = Logger::open(&self.config.log_dir, &room_name)?;

        // Record pending verification state (5-second timeout).
        self.pending_verify = Some(PendingVerify {
            room_name: room_name.clone(),
            room_key,
            deadline: tokio::time::Instant::now() + Duration::from_secs(5),
        });

        self.logger = Some(logger);

        let msg = DisplayMessage::system(&format!(
            "Connecting to room '{}' — waiting for verification…",
            room_name
        ));
        let _ = self.ui_event_tx.send(UiEvent::NewMessage(msg));

        Ok(())
    }

    async fn leave_room(&mut self) -> Result<()> {
        if let Some(room) = self.room.take() {
            let _ = self
                .net_cmd_tx
                .send(NetworkCommand::Unsubscribe(room.topic.clone()));
            if let Some(ref mut log) = self.logger {
                let _ = log.log_event("session ended");
            }
            info!("Left room '{}'", room.name);
        }
        self.room_key = None;
        self.logger = None;
        self.pending_verify = None;
        self.peers.clear();

        let _ = self.ui_event_tx.send(UiEvent::ShowMainMenu);
        self.emit_status();
        Ok(())
    }

    // ── Message sending ───────────────────────────────────────────────────────

    async fn send_message(&mut self, text: String) -> Result<()> {
        let (room, key) = match (&self.room, &self.room_key) {
            (Some(r), Some(k)) => (r.clone(), k),
            _ => {
                let _ = self
                    .ui_event_tx
                    .send(UiEvent::Error("Not in a room.".to_string()));
                return Ok(());
            }
        };

        let wire = WireMessage {
            msg_type: WireMessageType::Chat,
            sender_nick: self.identity.nickname.clone(),
            sender_disc: self.identity.discriminator.clone(),
            timestamp_ms: Utc::now().timestamp_millis(),
            text: text.clone(),
        };

        let json = serde_json::to_vec(&wire)?;
        let encrypted = key.encrypt(&json)?;

        let _ = self.net_cmd_tx.send(NetworkCommand::Publish {
            topic: room.topic.clone(),
            data: encrypted,
        });

        // Show our own message locally immediately.
        let display = DisplayMessage::chat(&self.identity.display_name(), &text);
        if let Some(ref mut log) = self.logger {
            let _ = log.log(&display);
        }
        let _ = self.ui_event_tx.send(UiEvent::NewMessage(display));

        Ok(())
    }

    // ── Network events ────────────────────────────────────────────────────────

    async fn handle_network_event(&mut self, event: NetworkEvent) -> Result<()> {
        match event {
            NetworkEvent::MessageReceived { topic, payload } => {
                self.handle_message(topic, payload).await?;
            }

            NetworkEvent::PeerSubscribed { topic, peer_id } => {
                // A new peer joined our topic — publish verification token so they
                // can confirm the password.
                if let Some(room) = &self.room {
                    if topic == room.topic {
                        tracing::debug!("Peer {peer_id} subscribed to room '{}'", room.name);
                        if let Some(key) = &self.room_key {
                            if let Ok(token) = key.make_verification_token(&room.name) {
                                let _ = self.net_cmd_tx.send(NetworkCommand::Publish {
                                    topic: topic.clone(),
                                    data: self.wrap_verification_token(token)?,
                                });
                            }
                        }
                    }
                }
                // Track peer count.
                if let Some(ref mut room) = self.room {
                    if topic == room.topic {
                        room.peer_count += 1;
                        self.emit_status();
                    }
                }
            }

            NetworkEvent::PeerDisconnected(peer_id) => {
                if let Some(name) = self.peers.remove(&peer_id) {
                    let msg = DisplayMessage::system(&format!("{} disconnected", name));
                    if let Some(ref mut log) = self.logger {
                        let _ = log.log(&msg);
                    }
                    let _ = self.ui_event_tx.send(UiEvent::NewMessage(msg));
                    if let Some(ref mut room) = self.room {
                        room.peer_count = room.peer_count.saturating_sub(1);
                    }
                    self.emit_status();
                }
            }

            NetworkEvent::ListeningOn(addr) => {
                if !self.listen_addrs.contains(&addr) {
                    self.listen_addrs.push(addr);
                }
            }

            NetworkEvent::NewExternalAddr(addr) => {
                info!("External address: {addr}");
                if !self.listen_addrs.contains(&addr) {
                    self.listen_addrs.insert(0, addr);
                }
            }

            NetworkEvent::PeerConnected => {}
        }
        Ok(())
    }

    async fn handle_message(&mut self, topic: String, payload: Vec<u8>) -> Result<()> {
        // ── Pending verification ──────────────────────────────────────────────
        if let Some(ref pv) = self.pending_verify {
            // Try to decrypt with the pending key.
            if let Ok(plaintext) = pv.room_key.decrypt(&payload) {
                if let Ok(wire) = serde_json::from_slice::<WireMessage>(&plaintext) {
                    if wire.msg_type == WireMessageType::VerificationToken {
                        let token: Vec<u8> = serde_json::from_str(&wire.text)
                            .unwrap_or_default();
                        let room_name = pv.room_name.clone();
                        if pv.room_key.verify_token(&token, &room_name) {
                            self.confirm_join(room_name).await;
                        } else {
                            self.deny_join().await;
                        }
                        return Ok(());
                    }
                }
            }
        }

        // ── Normal message for the active room ────────────────────────────────
        let (room_name, key) = match (&self.room, &self.room_key) {
            (Some(r), Some(k)) => (r.name.clone(), k),
            _ => return Ok(()),
        };

        if !topic.ends_with(&room_name) {
            return Ok(());
        }

        let plaintext = match key.decrypt(&payload) {
            Ok(p) => p,
            Err(_) => return Ok(()), // Silently discard — wrong key or noise.
        };

        let wire: WireMessage = match serde_json::from_slice(&plaintext) {
            Ok(w) => w,
            Err(_) => return Ok(()),
        };

        if wire.msg_type == WireMessageType::VerificationToken {
            return Ok(()); // Already handled above.
        }

        let sender = format!("{}#{}", wire.sender_nick, wire.sender_disc);

        // Skip echo of our own messages (we display them immediately on send).
        if wire.sender_nick == self.identity.nickname
            && wire.sender_disc == self.identity.discriminator
        {
            return Ok(());
        }

        // Track peer display name.
        let peer_key = format!("{}#{}", wire.sender_nick, wire.sender_disc);
        self.peers.entry(peer_key.clone()).or_insert_with(|| {
            let msg = DisplayMessage::system(&format!("{} joined the room", peer_key));
            let _ = self.ui_event_tx.send(UiEvent::NewMessage(msg.clone()));
            if let Some(ref mut log) = self.logger {
                let _ = log.log(&msg);
            }
            peer_key.clone()
        });

        let display = DisplayMessage::chat(&sender, &wire.text);
        if let Some(ref mut log) = self.logger {
            let _ = log.log(&display);
        }
        let _ = self.ui_event_tx.send(UiEvent::NewMessage(display));

        Ok(())
    }

    // ── Verification flow ─────────────────────────────────────────────────────

    async fn confirm_join(&mut self, room_name: String) {
        if let Some(pv) = self.pending_verify.take() {
            self.room_key = Some(pv.room_key);
        }
        let room_state = RoomState::new(&room_name);
        self.room = Some(room_state);
        let _ = self.ui_event_tx.send(UiEvent::RoomJoined(room_name));
        self.emit_status();
    }

    async fn deny_join(&mut self) {
        self.pending_verify = None;
        if let Some(room) = self.room.take() {
            let _ = self
                .net_cmd_tx
                .send(NetworkCommand::Unsubscribe(room.topic));
        }
        self.logger = None;
        let _ = self.ui_event_tx.send(UiEvent::AccessDenied);
        let _ = self.ui_event_tx.send(UiEvent::ShowMainMenu);
    }

    fn check_verify_timeout(&mut self) {
        let timed_out = self
            .pending_verify
            .as_ref()
            .map(|pv| tokio::time::Instant::now() >= pv.deadline)
            .unwrap_or(false);

        if timed_out {
            // No verification token received → assume empty room / creator offline.
            // Let the user in with the key they provided.
            if let Some(pv) = self.pending_verify.take() {
                let room_name = pv.room_name.clone();
                self.room_key = Some(pv.room_key);
                let room_state = RoomState::new(&room_name);
                self.room = Some(room_state);
                let _ = self.ui_event_tx.send(UiEvent::RoomJoined(room_name));
                self.emit_status();
            }
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Wrap a raw verification token bytes in an encrypted WireMessage envelope.
    fn wrap_verification_token(&self, token: Vec<u8>) -> Result<Vec<u8>> {
        let key = self.room_key.as_ref().expect("room key present");
        let wire = WireMessage {
            msg_type: WireMessageType::VerificationToken,
            sender_nick: self.identity.nickname.clone(),
            sender_disc: self.identity.discriminator.clone(),
            timestamp_ms: Utc::now().timestamp_millis(),
            text: serde_json::to_string(&token)?,
        };
        let json = serde_json::to_vec(&wire)?;
        key.encrypt(&json)
    }

    fn emit_status(&self) {
        let _ = self.ui_event_tx.send(UiEvent::StatusUpdate {
            room: self.room.as_ref().map(|r| r.name.clone()),
            peers: self.room.as_ref().map(|r| r.peer_count).unwrap_or(0),
        });
    }
}
