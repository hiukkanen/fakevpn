use anyhow::Result;
use iroh::protocol::{AcceptError, ProtocolHandler};
use iroh::endpoint::Connection;
use crate::vpn;
use std::io;

/// The accepting (host/listen) side of a VPN session.
#[derive(Debug, Clone)]
pub struct VpnHandler {
    pub device_name: String,
    pub tap_ip: String,
    pub tap_mtu: u16,
}

impl ProtocolHandler for VpnHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let (send, recv) = connection.accept_bi().await
            .map_err(|e| AcceptError::from_err(io::Error::other(e.to_string())))?;

        // Avataan ja määritellään TAP-laite palvelimella
        let dev = crate::tap::open_and_configure(&self.device_name, &self.tap_ip, self.tap_mtu)
            .map_err(|e| AcceptError::from_err(io::Error::other(format!("TAP-laitteen määritys epäonnistui: {}", e))))?;

        println!("Uusi VPN-yhteys hyväksytty!");

        let (tap_read, tap_write) = tokio::io::split(dev);
        if let Err(e) = vpn::bridge(tap_read, tap_write, send, recv).await {
            eprintln!("Yhteysvirhe: {:?}", e);
        }
        Ok(())
    }
}
