mod app;
mod cli;
mod config;
mod crypto;
mod identity;
mod logger;
mod network;
mod room;
mod types;

use anyhow::Result;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::{
    app::App,
    config::Config,
    identity::Identity,
    network::NetworkService,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialise tracing (write to stderr so it doesn't pollute the TUI).
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::from_default_env())
        .init();

    // ── Config & identity ─────────────────────────────────────────────────────
    let mut config = Config::load_or_default();
    let mut identity = Identity::load_or_create(&mut config)?;

    // Prompt for nickname on first run (before TUI takes over).
    if config.nickname.is_none() {
        let nick = prompt_nickname()?;
        identity.nickname = nick.clone();
        config.nickname = Some(nick);
    }
    config.save()?;

    // ── Network service ───────────────────────────────────────────────────────
    let (net_service, net_event_rx, net_cmd_tx) =
        NetworkService::new(identity.keypair.clone())?;

    // ── Inter-task channels ───────────────────────────────────────────────────
    let (cli_cmd_tx, cli_cmd_rx) = tokio::sync::mpsc::unbounded_channel();
    let (ui_event_tx, ui_event_rx) = tokio::sync::mpsc::unbounded_channel();

    // ── Spawn tasks ───────────────────────────────────────────────────────────

    // Network task — drives the libp2p swarm.
    tokio::spawn(async move {
        net_service.run().await;
    });

    // Application task — owns state and orchestrates everything.
    let app = App::new(
        identity,
        config,
        net_event_rx,
        net_cmd_tx,
        cli_cmd_rx,
        ui_event_tx,
    );
    let app_handle = tokio::spawn(async move {
        if let Err(e) = app.run().await {
            tracing::error!("App error: {e}");
        }
    });

    // CLI task — owns the terminal (runs until the user quits).
    cli::run_cli(cli_cmd_tx, ui_event_rx).await?;

    // Give the app a moment to clean up.
    let _ = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        app_handle,
    )
    .await;

    Ok(())
}

/// Blocking stdin prompt for the nickname.
/// Called before the crossterm TUI starts, so plain I/O is fine.
fn prompt_nickname() -> Result<String> {
    use std::io::{self, BufRead, Write};
    print!("Welcome! Enter your nickname: ");
    io::stdout().flush()?;
    let nick = io::stdin()
        .lock()
        .lines()
        .next()
        .transpose()?
        .unwrap_or_default()
        .trim()
        .to_string();
    Ok(if nick.is_empty() {
        "Anonymous".to_string()
    } else {
        nick.chars().take(32).collect()
    })
}
