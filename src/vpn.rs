use anyhow::Result;
use iroh::endpoint::{RecvStream, SendStream};
use std::fs::File;
use std::io::Read;
use std::sync::{Arc, Mutex};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

const TAP_BUF_SIZE: usize = 2048;

/// QUIC requires the stream opener to write before the peer's `accept_bi` can finish.
const STREAM_OPEN: u8 = 0;

pub async fn open_stream(send: &mut SendStream) -> Result<()> {
    send.write_all(&[STREAM_OPEN]).await?;
    Ok(())
}

pub async fn accept_stream(recv: &mut RecvStream) -> Result<()> {
    let mut byte = [0u8; 1];
    recv.read_exact(&mut byte).await?;
    Ok(())
}

pub async fn bridge(tap_dev: File, mut send: SendStream, mut recv: RecvStream) -> Result<()> {
    let tap_read = tap_dev;
    let tap_write = Arc::new(Mutex::new(tap_read.try_clone()?));
    let (tap_tx, mut tap_rx) = mpsc::channel::<Vec<u8>>(64);

    let read_task = tokio::task::spawn_blocking(move || -> Result<()> {
        let mut tap = tap_read;
        let mut buf = vec![0u8; TAP_BUF_SIZE];
        loop {
            let n = tap.read(&mut buf)?;
            if n == 0 {
                break;
            }
            if tap_tx.blocking_send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
        Ok(())
    });

    loop {
        tokio::select! {
            pkt = tap_rx.recv() => {
                match pkt {
                    Some(data) => send.write_all(&data).await?,
                    None => break,
                }
            }
            chunk = recv.read_chunk(TAP_BUF_SIZE) => {
                match chunk? {
                    Some(bytes) => {
                        let data = bytes.to_vec();
                        let tap = Arc::clone(&tap_write);
                        tokio::task::spawn_blocking(move || -> Result<()> {
                            use std::io::Write;
                            tap.lock().unwrap().write_all(&data)?;
                            Ok(())
                        }).await??;
                    }
                    None => break,
                }
            }
        }
    }

    read_task.abort();
    Ok(())
}
