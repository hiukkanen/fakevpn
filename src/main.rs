mod vpn;
use anyhow::{Result, Context};
use iroh::{Endpoint, SecretKey, PublicKey};
use iroh::endpoint::presets;
use tun::Configuration;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. TAP-kortin avaus
    let mut config = Configuration::default();
    config.name("FC-TAP");
    let _dev = tun::create(&config).context("Virhe: FC-TAP korttia ei löytynyt.")?;
    println!("FC-TAP kortti avattu.");

    // 2. Iroh-endpointin alustus
    let secret_key = SecretKey::generate();
    let _endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key.clone())
        .bind()
        .await?;

    let my_id: PublicKey = secret_key.public();
    println!("Oma Node ID: {}", my_id);

    // 3. Komentoriviargumenttien käsittely
    let args: Vec<String> = env::args().collect();
    if let Some(target_id_str) = args.get(1) {
        let target_id: PublicKey = target_id_str.parse().context("Virheellinen Node ID")?;
        println!("Yhdistetään kohteeseen: {}", target_id);
    } else {
        println!("Palvelintila: Odotetaan yhteyksiä...");
    }

    Ok(())
}
