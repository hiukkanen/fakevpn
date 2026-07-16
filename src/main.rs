mod vpn;
mod server;

use anyhow::{Result, Context};
use iroh::{Endpoint, PublicKey};
use iroh::endpoint::presets;
use iroh::protocol::Router;
use tun::Configuration;
use std::env;
use crate::server::VpnHandler;

#[tokio::main]
async fn main() -> Result<()> {
    let mut config = Configuration::default();
    config.name("FC-TAP");

    let secret_key = iroh::SecretKey::generate();
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key.clone())
        .bind()
        .await?;

    let router = Router::builder(endpoint)
        .accept(b"fakevpn/v1", VpnHandler { config: config.clone() })
        .spawn();

    println!("Oma Node ID: {}", secret_key.public());

    let args: Vec<String> = env::args().collect();
    if let Some(target_id_str) = args.get(1) {
        let target_id: PublicKey = target_id_str.parse().context("Virheellinen Node ID")?;
        let dev = tun::create_as_async(&config)?;
        let conn = router.endpoint().connect(target_id, b"fakevpn/v1").await?;
        let (send, recv) = conn.open_bi().await?;
        vpn::bridge(dev, send, recv).await?;
    } else {
        println!("Palvelintila: Odotetaan yhteyksiä...");
        // Router on käynnissä, pidetään pääohjelma elossa
        loop { tokio::time::sleep(tokio::time::Duration::from_secs(60)).await; }
    }
    Ok(())
}
