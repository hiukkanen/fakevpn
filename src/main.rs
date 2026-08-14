mod keys;
mod server;
mod tap;
mod vpn;
#[cfg(target_os = "windows")]
mod windows_tap;

use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Context, Result};
use clap::Parser;
use iroh::endpoint::presets;
use iroh::protocol::Router;
use iroh::{Endpoint, PublicKey};
use iroh_base::KeyParsingError;
use crate::server::VpnHandler;

/// Far Cry 4 coop VPN: Layer-2 TAP tunnel over Iroh P2P connection.
#[derive(Parser, Debug)]
struct Cli {
    /// TAP device name (default FC-TAP).
    #[clap(long, default_value = "FC-TAP")]
    device_name: String,

    /// Remote node ID to connect to. If provided, the program is a client;
    /// without it, the program listens as a host.
    /// Accepts a 64-character hex string or a base32-encoded node ID.
    #[clap(long)]
    connect: Option<String>,

    /// IP address to assign to the TAP device. Default 10.0.0.1 (host) / 10.0.0.2 (client).
    #[clap(long)]
    tap_ip: Option<String>,

    /// TAP device MTU. Default 1400 (to reduce QUIC encapsulation overhead).
    #[clap(long, default_value_t = 1400)]
    tap_mtu: u16,

    /// Path to the key file (default %APPDATA%/fakevpn/key).
    #[clap(long)]
    key_file: Option<PathBuf>,
}

impl Cli {
    fn tap_ip(&self) -> String {
        self.tap_ip
            .clone()
            .or_else(|| {
                if self.connect.is_some() {
                    Some("10.0.0.2".to_string())
                } else {
                    Some("10.0.0.1".to_string())
                }
            })
            .unwrap()
    }

    fn parse_connect(&self) -> Result<Option<PublicKey>, KeyParsingError> {
        match &self.connect {
            Some(s) => {
                let key = PublicKey::from_str(s)?;
                Ok(Some(key))
            }
            None => Ok(None),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let secret_key = keys::load_or_generate(cli.key_file.as_deref())?;
    let endpoint = Endpoint::builder(presets::N0)
        .alpns(vec![b"fakevpn/v1".to_vec()])
        .secret_key(secret_key.clone())
        .bind()
        .await?;

    let handler = VpnHandler {
        device_name: cli.device_name.clone(),
        tap_ip: cli.tap_ip(),
        tap_mtu: cli.tap_mtu,
        session_active: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };

    let router = Router::builder(endpoint.clone())
        .accept(b"fakevpn/v1", handler)
        .spawn();

    println!("My Node ID: {}", secret_key.public());
    println!("Iroh endpoint ID: {}", endpoint.id());
    println!("Iroh endpoint addr: {:?}", endpoint.addr());
    println!("Share this Node ID with the other party.");

    let tap_ip = cli.tap_ip();
    let tap_mtu = cli.tap_mtu;
    let device_name = cli.device_name.clone();

    let target_id = cli.parse_connect().map_err(|e| {
        anyhow::anyhow!(
            "Invalid node ID: {}. Expected a 64-character hex string or a base32-encoded node ID.",
            e
        )
    })?;

    if let Some(target_id) = target_id {
        // Client mode: connecting to host.
        println!("Client mode: connecting to node {}...", target_id);
        println!("Client Iroh endpoint ID: {}", endpoint.id());
        println!("Client Iroh endpoint addr: {:?}", endpoint.addr());

        println!("Dialing remote Iroh node {} over ALPN fakevpn/v1...", target_id);
        let conn = tokio::select! {
            conn = router.endpoint().connect(target_id, b"fakevpn/v1") => {
                conn.map_err(|e| {
                    anyhow::anyhow!(
                        "Connection failed: the host may be offline or the Node ID is wrong. Make sure the server is running and you used the correct Node ID. Original error: {}",
                        e
                    )
                })?
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\nCancelled during dial.");
                return Ok(());
            }
        };
        println!("Iroh connection established to {}.", target_id);

        let tap_sync = tap::open_and_configure(&device_name, &tap_ip, tap_mtu)
            .context("TAP device configuration failed. Run as Administrator and ensure FC-TAP is created.")?;
        let (tap_read, tap_write) = tokio::io::split(tap_sync);

        let (send, recv) = conn.open_bi().await.map_err(|e| {
            anyhow::anyhow!(
                "Connection established, but the VPN stream could not be opened. The host may be offline or the protocol version may not match. Original error: {}",
                e
            )
        })?;

        tokio::select! {
            res = vpn::bridge(tap_read, tap_write, send, recv) => {
                if let Err(e) = res {
                    eprintln!("Connection error: {:?}", e);
                }
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\nClosing...");
                conn.close(0u32.into(), b"shutdown");
            }
        }
    } else {
        // Host mode: listening for connections.
        println!("Host mode: waiting for connections... (Ctrl+C to exit)");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\nClosing...");
            }
            _ = std::future::pending::<()>() => {}
        }
        let _ = router.shutdown().await;
    }

    Ok(())
}
