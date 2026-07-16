mod vpn;
use anyhow::{Result, Context};
use iroh::{Endpoint, PublicKey};
use iroh::endpoint::presets;
use iroh::protocol::Router;
use tun::Configuration;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. TAP-kortin avaus asynkronisena laitteena
    let mut config = Configuration::default();
    config.name("FC-TAP");
    // Huom: Käytämme create_as_async, jotta voimme lukea laitetta async-silmukassa
    let dev = tun::create_as_async(&config).context("TAP-kortin avaus epäonnistui. Aja järjestelmänvalvojana.")?;
    println!("FC-TAP kortti avattu.");

    // 2. Iroh-endpointin alustus
    let secret_key = iroh::SecretKey::generate();
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key.clone())
        .bind()
        .await?;

    let router = Router::builder(endpoint).spawn();
    println!("Oma Node ID: {}", secret_key.public());

    // 3. Komentoriviargumenttien käsittely
    let args: Vec<String> = env::args().collect();
    if let Some(target_id_str) = args.get(1) {
        let target_id: PublicKey = target_id_str.parse().context("Virheellinen Node ID")?;
        println!("Yhdistetään kohteeseen: {}", target_id);
        
        // Luodaan yhteys ja avataan bi-directionaalinen streami
        let conn = router.endpoint().connect(target_id, b"fakevpn/v1").await?;
        let (send, recv) = conn.open_bi().await?;
        
        println!("Yhteys muodostettu, aloitetaan siltaus...");
        vpn::bridge(dev, send, recv).await?;
    } else {
        println!("Palvelintila: Odotetaan yhteyksiä (toteuta accept-logiikka vpn.rs:ään)...");
    }

    Ok(())
}
