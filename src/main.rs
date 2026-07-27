mod vpn;
mod server;

#[cfg(target_os = "windows")]
mod windows_tap;
#[cfg(not(target_os = "windows"))]
mod tap_stub;

use anyhow::{Result, Context};
use iroh::{Endpoint, PublicKey};
use iroh::endpoint::presets;
use iroh::protocol::Router;
use std::env;
use crate::server::VpnHandler;

#[cfg(target_os = "windows")]
use crate::windows_tap;
#[cfg(not(target_os = "windows"))]
use crate::tap_stub;

#[tokio::main]
async fn main() -> Result<()> {
    let device_name = "FC-TAP";

    let secret_key = iroh::SecretKey::generate();
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key.clone())
        .bind()
        .await?;

    let router = Router::builder(endpoint)
        .accept(b"fakevpn/v1", VpnHandler { device_name: device_name.to_string() })
        .spawn();

    println!("Oma Node ID: {}", secret_key.public());

    let args: Vec<String> = env::args().collect();
    if let Some(target_id_str) = args.get(1) {
        let target_id: PublicKey = target_id_str.parse().context("Virheellinen Node ID")?;

        println!("Odotetaan verkkoyhteyttä...");
        router.endpoint().online().await;

        println!("Avataan olemassa oleva Windows TAP-laite: {}...", device_name);
        let dev = windows_tap::open_tap_device(device_name)
            .context("TAP-laitteen avaaminen epäonnistui. Aja ohjelma Administrator-oikeuksilla ja varmista, että FC-TAP on luotu.")?;

        let conn = router.endpoint().connect(target_id, b"fakevpn/v1").await?;
        let (mut send, recv) = conn.open_bi().await?;
        vpn::open_stream(&mut send).await?;

        vpn::bridge(dev, send, recv).await?;
    } else {
        println!("Palvelintila: Odotetaan yhteyksiä...");
        router.endpoint().online().await;
        println!("Valmis vastaanottamaan yhteyksiä.");

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    }
    Ok(())
}
