use anyhow::Result;
use iroh::endpoint::{RecvStream, SendStream};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Largest single Layer-2 frame we will carry over the tunnel.
///
/// Standard Ethernet frames are 1518 bytes (1522 with a VLAN tag); 2048 leaves
/// headroom and keeps the buffer aligned. A 4-byte length prefix per frame is
/// ~0.2% overhead — negligible.
const MAX_FRAME: usize = 2048;

/// Bytes are framed over the single reliable QUIC bidirectional stream as:
/// `[4-byte big-endian length N][N bytes of L2 frame]`.
///
/// The previous implementation wrote raw frames into the byte stream and read
/// them back with `RecvStream::read_chunk`, whose chunks are *not* aligned to
/// peer writes — so frames were arbitrarily glued/split and written to the TAP
/// device corrupted. The length prefix makes the stream self-delimiting.
///
/// `RecvStream::read_exact` is **not cancel-safe**, so the receive direction
/// runs as its own loop. The two directions are combined with an outer
/// `tokio::select!` which drops whichever branch loses; this is fine because
/// the bridge is tearing down anyway.
pub async fn bridge<R, W>(
    mut tap_read: R,
    mut tap_write: W,
    mut send: SendStream,
    mut recv: RecvStream,
) -> Result<()>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    // Direction 1: TAP -> tunnel. TAP-Windows6 delivers one whole Ethernet frame
    // per read, so we prefix each read with its length.
    let to_tunnel = async {
        let mut buf = [0u8; MAX_FRAME];
        loop {
            let n = tap_read.read(&mut buf).await?;
            if n == 0 {
                break; // Device closed
            }
            send.write_all(&(n as u32).to_be_bytes()).await?;
            send.write_all(&buf[..n]).await?;
        }
        Ok::<(), anyhow::Error>(())
    };

    // Direction 2: tunnel -> TAP. read_exact is not cancel-safe, so it runs in
    // its own loop. The outer select! will drop the losing branch when the bridge
    // tears down, which is acceptable since the whole connection is ending anyway.
    let to_tap = async {
        let mut len_buf = [0u8; 4];
        let mut frame = [0u8; MAX_FRAME];
        loop {
            recv.read_exact(&mut len_buf).await?;
            let len = u32::from_be_bytes(len_buf) as usize;
            if len == 0 || len > MAX_FRAME {
                // Protocol error or garbage: stop rather than allocate a huge buffer.
                break;
            }
            recv.read_exact(&mut frame[..len]).await?;
            tap_write.write_all(&frame[..len]).await?;
        }
        Ok::<(), anyhow::Error>(())
    };

    // Run both directions concurrently. Either side finishing (device closed or
    // peer disconnected) ends the bridge.
    tokio::select! {
        res = to_tunnel => res,
        res = to_tap => res,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iroh::endpoint::{Endpoint, presets};
    use iroh::{EndpointAddr, TransportAddr};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const ALPN: &[u8] = b"fakevpn/v1";

    /// Two loopback iroh endpoints (no relay/DNS) plus the server's address.
    async fn loopback_pair() -> anyhow::Result<(Endpoint, Endpoint, EndpointAddr)> {
        let server = Endpoint::builder(presets::Minimal)
            .alpns(vec![ALPN.to_vec()])
            .bind_addr("127.0.0.1:0")?
            .bind()
            .await?;
        let server_addr = EndpointAddr::from_parts(
            server.id(),
            server.addr().ip_addrs().map(|ip| TransportAddr::Ip(*ip)),
        );
        let client = Endpoint::builder(presets::Minimal)
            .bind_addr("127.0.0.1:0")?
            .bind()
            .await?;
        Ok((server, client, server_addr))
    }

    /// Direct regression guard for the framing fix: frames written into one fake
    /// TAP (by the simulated game) must come out the other TAP byte-identical,
    /// regardless of size or back-to-back timing. Previously `read_chunk`
    /// glued/split frames into corrupted output.
    #[tokio::test]
    async fn framing_reassembles_frames_exactly() -> anyhow::Result<()> {
        let frames: Vec<Vec<u8>> = vec![
            vec![0u8; 1],                                         // 1-byte frame
            (0..).map(|i| (i & 0xff) as u8).take(1518).collect(), // full-size Ethernet frame
            b"hello tapper".to_vec(),                              // small, back-to-back
            b"second-in-a-row".to_vec(),
            (0..).map(|i| (i & 0xff) as u8).take(900).collect(),   // mid-size
        ];

        let (server_ep, client_ep, server_addr) = loopback_pair().await?;

        // Each fake TAP is a `duplex`: one end (game side) the test writes/reads,
        // the other end (bridge side) is split into read+write halves for `bridge`.
        // client TAP: test writes frames in.
        let (client_game, client_tap) = tokio::io::duplex(1 << 20);
        // server TAP: test reads frames out.
        let (server_tap, server_game) = tokio::io::duplex(1 << 20);

        // Server side: accept the inbound connection + bidi stream, run `bridge`.
        let server_task = tokio::spawn(async move {
            let connecting = server_ep.accept().await.ok_or_else(|| {
                anyhow::anyhow!("endpoint closed before incoming connection")
            })?;
            let conn = connecting.await?;
            let (send, recv) = conn.accept_bi().await?;
            let (r, w) = tokio::io::split(server_tap);
            bridge(r, w, send, recv).await
        });

        // Client side: connect, open the bidi stream, run `bridge` on client TAP.
        let conn = client_ep.connect(server_addr, ALPN).await?;
        let (client_send, client_recv) = conn.open_bi().await?;
        let (crate_read, crate_write) = tokio::io::split(client_tap);
        let _client_bridge =
            tokio::spawn(bridge(crate_read, crate_write, client_send, client_recv));

        // Simulated game: write each frame into the client TAP.
        let mut game_write = client_game;
        for f in &frames {
            game_write.write_all(f).await?;
        }

        // Simulated game: read the reassembled bytes back out of the server TAP.
        let total: usize = frames.iter().map(|f| f.len()).sum();
        let mut got = vec![0u8; total];
        let mut game_read = server_game;
        match tokio::time::timeout(Duration::from_secs(10), game_read.read_exact(&mut got)).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return Err(anyhow::anyhow!("read from server TAP failed: {}", e)),
            Err(_) => return Err(anyhow::anyhow!("timeout waiting for all {} bytes", total)),
        }

        let expected: Vec<u8> = frames.concat();
        assert_eq!(got, expected, "tunnel did not reassemble frames byte-identically");

        // Let the bridges wind down without panicking.
        drop(game_write);
        let _ = tokio::time::timeout(Duration::from_secs(5), server_task).await;
        Ok(())
    }
}
