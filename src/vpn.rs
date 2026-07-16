use anyhow::Result;
use iroh::endpoint::{SendStream, RecvStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tun::AsyncDevice;

pub async fn bridge(mut tap_dev: AsyncDevice, mut send: SendStream, mut recv: RecvStream) -> Result<()> {
    let mut buf = [0u8; 2048];
    
    loop {
        tokio::select! {
            // 1. Lue TAP-laitteelta ja lähetä Irohiin
            res = tap_dev.read(&mut buf) => {
                let n = res?;
                send.write_all(&buf[..n]).await?;
            }
            // 2. Lue Irohilta ja kirjoita TAP-laitteelle
            res = recv.read_chunk(2048) => {
                // read_chunk palauttaa Result<Option<Bytes>>
                if let Some(bytes) = res? {
                    // Itse 'bytes' on jo dataa, ei tarvitse .bytes-kenttää
                    tap_dev.write_all(&bytes).await?;
                }
            }
        }
    }
}
