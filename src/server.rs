use anyhow::Result;
use iroh::protocol::{AcceptError, ProtocolHandler};
use iroh::endpoint::Connection;
use crate::vpn;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The accepting (host/listen) side of a VPN session.
#[derive(Debug, Clone)]
pub struct VpnHandler {
    pub device_name: String,
    pub tap_ip: String,
    pub tap_mtu: u16,
    pub session_active: Arc<AtomicBool>,
}

impl ProtocolHandler for VpnHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let remote_id = connection.remote_id();
        
        println!("[STATUS] Online with {}", remote_id);
        
        if self.session_active.swap(true, Ordering::SeqCst) {
            return Err(AcceptError::from_err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                "another VPN session is already active",
            )));
        }

        let (send, recv) = connection.accept_bi().await?;

        let result = async {
            let tap_sync = crate::tap::open_and_configure(&self.device_name, &self.tap_ip, self.tap_mtu)
                .map_err(|e| AcceptError::from_err(std::io::Error::other(format!("TAP device configuration failed: {}", e))))?;
            let (tap_read, tap_write) = tokio::io::split(tap_sync);

            println!("New VPN connection accepted!");

            if let Err(e) = vpn::bridge(tap_read, tap_write, send, recv).await {
                eprintln!("Connection error: {:?}", e);
            }
            
            println!("[STATUS] Offline from {}", remote_id);
            Ok(())
        }
        .await;

        self.session_active.store(false, Ordering::SeqCst);
        result
    }
}
