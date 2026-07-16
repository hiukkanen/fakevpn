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
        // 1. Tehdään vaaralliset await-kutsut ensin, jolloin 'config' ei ole vielä olemassakaan
        let (send, recv) = connection.accept_bi().await
            .map_err(|e| AcceptError::from_err(io::Error::new(io::ErrorKind::Other, e.to_string())))?;

        // 2. Luodaan 'config' ja 'dev' vasta AWAIT-kutsun JÄLKEEN
        // Nyt config ei elä await-kutsun yli, joten se ei haittaa säieturvallisuutta
        let dev = {
            let mut config = Configuration::default();
            config.name(&self.device_name);

            tun::create_as_async(&config)
        }
        .map_err(|e| AcceptError::from_err(io::Error::new(io::ErrorKind::Other, e.to_string())))?;
        
        println!("Uusi VPN-yhteys hyväksytty!");
        
        // 3. Nyt voidaan aloittaa siltaus
        if let Err(e) = vpn::bridge(dev, send, recv).await {
            eprintln!("Yhteysvirhe: {:?}", e);
        }
        Ok(())
    }
}
