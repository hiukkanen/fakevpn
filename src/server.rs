use anyhow::Result;
use iroh::protocol::{AcceptError, ProtocolHandler};
use iroh::endpoint::Connection;
use tun::Configuration;
use crate::vpn;
use std::io;

#[derive(Debug, Clone)]
pub struct VpnHandler {
    pub device_name: String,
}

impl ProtocolHandler for VpnHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        // 1. Tehdään asynkroninen odotus yhteydelle ensin
        let (send, recv) = connection.accept_bi().await
            .map_err(|e| AcceptError::from_err(io::Error::new(io::ErrorKind::Other, e.to_string())))?;

        // 2. Avataan olemassa oleva TAP-kortti vasta odotuksen jälkeen
        let dev = {
            let mut config = Configuration::default();
            config.name(&self.device_name);
            // Määritetään Layer 2 (TAP), jotta palvelinkin osaa tarttua oikeaan korttiin
            config.layer(tun::Layer::L2);

            tun::create_as_async(&config)
        }
        .map_err(|e| AcceptError::from_err(io::Error::new(io::ErrorKind::Other, e.to_string())))?;
        
        println!("Uusi VPN-yhteys hyväksytty!");
        
        // 3. Aloitetaan siltaus
        if let Err(e) = vpn::bridge(dev, send, recv).await {
            eprintln!("Yhteysvirhe: {:?}", e);
        }
        Ok(())
    }
}
