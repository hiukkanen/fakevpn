use anyhow::Result;
use iroh::endpoint::{SendStream, RecvStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tun::AsyncDevice;

pub async fn bridge(mut tap_dev: AsyncDevice, mut send: SendStream, mut recv: RecvStream) -> Result<()> {
    let mut buf = [0u8; 2048];
    
    loop {
        tokio::select! {
            res = tap_dev.read(&mut buf) => {
                let n = res?;
                send.write_all(&buf[..n]).await?;
            }
            res = recv.read_chunk(2048) => {
                if let Some(bytes) = res? {
                    tap_dev.write_all(&bytes).await?;
                }
            }
        }
    }
}
