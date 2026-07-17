use anyhow::Result;
use iroh::endpoint::{SendStream, RecvStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::fs::File;

pub async fn bridge(mut tap_dev: File, mut send: SendStream, mut recv: RecvStream) -> Result<()> {
    let mut buf = [0u8; 2048];
    
    loop {
        tokio::select! {
            // Luetaan dataa Windowsin TAP-kortilta ja lähetetään se Irohiin
            res = tap_dev.read(&mut buf) => {
                let n = res?;
                if n == 0 { break; } // Laite suljettu
                send.write_all(&buf[..n]).await?;
            }
            // Vastaanotetaan dataa Irohista ja kirjoitetaan se TAP-kortille
            res = recv.read_chunk(2048) => {
                if let Some(bytes) = res? {
                    tap_dev.write_all(&bytes).await?;
                } else {
                    break; // Yhteys katkesi
                }
            }
        }
    }
    Ok(())
}
