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

        // Open and configure the TAP device on the server.
        let tap_sync = crate::tap::open_and_configure(&self.device_name, &self.tap_ip, self.tap_mtu)
            .map_err(|e| AcceptError::from_err(io::Error::other(format!("TAP device configuration failed: {}", e))))?;
        let (tap_read, tap_write) = tokio::io::split(tap_sync);

        println!("New VPN connection accepted!");

        if let Err(e) = vpn::bridge(tap_read, tap_write, send, recv).await {
            eprintln!("Connection error: {:?}", e);
        }
        Ok(())
    }
}
