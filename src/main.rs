mod vpn;
use anyhow::{Result, Context};
use iroh::{Endpoint, SecretKey};
use tun::Configuration;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Käynnistetään fakevpn...");

    let mut config = Configuration::default();
    config.name("FC-TAP");

    let dev = tun::create(&config)
        .context("Virhe: FC-TAP korttia ei löytynyt. Aja ohjelma järjestelmänvalvojana.")?;

    println!("Yhteys FC-TAP -korttiin onnistui!");

    let secret_key = SecretKey::generate();
    let endpoint = Endpoint::builder(iroh::endpoint::presets::N0::default())
        .secret_key(secret_key.clone())
        .bind()
        .await?;

    println!("Node ID: {}", secret_key.public());
    println!("Odota yhteyksiä tai käytä --connect <ID> (toteuta argumenttien käsittely seuraavaksi)");

    Ok(())
}
