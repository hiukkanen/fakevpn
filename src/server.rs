use anyhow::Result;
use iroh::protocol::{AcceptError, ProtocolHandler};
use iroh::endpoint::Connection;
use tun::Configuration;
use crate::vpn;
use std::io;

#[derive(Debug, Clone)]
pub struct VpnHandler {
    pub config: Configuration,
}

impl ProtocolHandler for VpnHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        // Muunnetaan virheet std::io::Erroriksi, jonka Iroh hyväksyy
        let (send, recv) = connection.accept_bi().await
            .map_err(|e| AcceptError::from_err(io::Error::new(io::ErrorKind::Other, e.to_string())))?;
            
        let dev = tun::create_as_async(&self.config)
            .map_err(|e| AcceptError::from_err(io::Error::new(io::ErrorKind::Other, e.to_string())))?;
        
        println!("Uusi VPN-yhteys hyväksytty!");
        
        if let Err(e) = vpn::bridge(dev, send, recv).await {
            eprintln!("Yhteysvirhe: {:?}", e);
        }
        Ok(())
    }
}
