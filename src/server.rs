use anyhow::Result;
use iroh::protocol::{AcceptError, ProtocolHandler};
use iroh::endpoint::Connection;
use crate::vpn;
use std::io;
use std::sync::Arc;
use tokio_tun::Tun;

#[derive(Debug, Clone)]
pub struct VpnHandler {
    pub device_name: String,
}

impl ProtocolHandler for VpnHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let (send, recv) = connection.accept_bi().await
            .map_err(|e| AcceptError::from_err(io::Error::new(io::ErrorKind::Other, e.to_string())))?;

        // Avataan olemassa oleva TAP-laite palvelimella
        let dev = Tun::builder()
            .name(&self.device_name)
            .tap()
            .packet_info()
            .build()
            .map_err(|e| AcceptError::from_err(io::Error::new(io::ErrorKind::Other, format!("TAP avaaminen epäonnistui: {}", e))))?
            .pop()
            .ok_or_else(|| AcceptError::from_err(io::Error::new(io::ErrorKind::Other, "Ei saatu laiteosoitinta")))?;
        
        let dev = Arc::new(dev);
        println!("Uusi VPN-yhteys hyväksytty!");
        
        if let Err(e) = vpn::bridge(dev, send, recv).await {
            eprintln!("Yhteysvirhe: {:?}", e);
        }
        Ok(())
    }
}
