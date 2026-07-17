use anyhow::Result;
use iroh::protocol::{AcceptError, ProtocolHandler};
use iroh::endpoint::Connection;
use crate::vpn;
use crate::windows_tap;
use std::io;

#[derive(Debug, Clone)]
pub struct VpnHandler {
    pub device_name: String,
}

impl ProtocolHandler for VpnHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let (send, recv) = connection.accept_bi().await
            .map_err(|e| AcceptError::from_err(io::Error::new(io::ErrorKind::Other, e.to_string())))?;

        // Avataan olemassa oleva TAP-laite palvelimella
        let dev = windows_tap::open_tap_device(&self.device_name)
            .map_err(|e| AcceptError::from_err(io::Error::new(io::ErrorKind::Other, format!("TAP avaaminen epäonnistui: {}", e))))?;
        
        println!("Uusi VPN-yhteys hyväksytty!");
        
        if let Err(e) = vpn::bridge(dev, send, recv).await {
            eprintln!("Yhteysvirhe: {:?}", e);
        }
        Ok(())
    }
}
