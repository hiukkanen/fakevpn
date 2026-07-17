use anyhow::Result;
use iroh::endpoint::{SendStream, RecvStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tun::Tun;
use std::sync::Arc;

pub async fn bridge(tap_dev: Arc<Tun>, mut send: SendStream, mut recv: RecvStream) -> Result<()> {
    let mut buf = [0u8; 2048];
    
    loop {
        tokio::select! {
            // Luetaan TAP-laitteesta ja lähetetään Irohiin
            res = tap_dev.recv(&mut buf) => {
                let n = res?;
                send.write_all(&buf[..n]).await?;
            }
            // Luetaan Irohista ja kirjoitetaan TAP-laitteeseen
            res = recv.read_chunk(2048) => {
                if let Some(bytes) = res? {
                    tap_dev.send_all(&bytes).await?;
                }
            }
        }
    }
}
