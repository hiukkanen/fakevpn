use anyhow::Result;
use iroh::protocol::{AcceptError, ProtocolHandler};
use iroh::endpoint::Connection;
use tun::Configuration;
use crate::vpn;
use std::io;

#[derive(Debug, Clone)]
pub struct VpnHandler {
    // Tallennetaan vain nimi, ei koko konfiguraatiota
    pub device_name: String,
}

impl ProtocolHandler for VpnHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        // Luodaan uusi konfiguraatio jokaisella kutsulla
        let mut config = Configuration::default();
        config.name(&self.device_name);

        let (send, recv) = connection.accept_bi().await
            .map_err(|e| AcceptError::from_err(io::Error::new(io::ErrorKind::Other, e.to_string())))?;
            
        let dev = tun::create_as_async(&config)
            .map_err(|e| AcceptError::from_err(io::Error::new(io::ErrorKind::Other, e.to_string())))?;
        
        println!("Uusi VPN-yhteys hyväksytty!");
        
        if let Err(e) = vpn::bridge(dev, send, recv).await {
            eprintln!("Yhteysvirhe: {:?}", e);
        }
        Ok(())
    }
}
