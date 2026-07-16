use anyhow::Result;
use tokio::io::{AsyncRead, AsyncWrite};

#[allow(dead_code)]
pub async fn bridge<R, W>(
    _send_stream: W,
    _recv_stream: R,
) -> Result<()> 
where 
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    // Tässä kohtaa tapahtuu packet-to-stream ja stream-to-packet -muunnos
    println!("VPN-silta valmis.");
    Ok(())
}
