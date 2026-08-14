//! Cross-platform facade over the TAP device.
//!
//! Only Windows has a real implementation (`windows_tap`). The other platforms
//! return an error at runtime (the binary is Windows-only for shipping), but
//! the stub kept here lets the crate *compile* on Linux so the framing unit
//! tests can run in CI / the dev codespace.

use anyhow::Result;
use std::io::{self, Read, Write};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

#[cfg(target_os = "windows")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "windows")]
use tokio::sync::oneshot;
#[cfg(target_os = "windows")]
use std::future::Future;

#[cfg(target_os = "windows")]
struct ReadState {
    recv: oneshot::Receiver<io::Result<Vec<u8>>>,
}

#[cfg(target_os = "windows")]
struct WriteState {
    recv: oneshot::Receiver<io::Result<usize>>,
}

#[cfg(target_os = "windows")]
pub struct TapSync {
    inner: Arc<Mutex<crate::windows_tap::TapSync>>,
    read_state: Option<ReadState>,
    write_state: Option<WriteState>,
}

#[cfg(target_os = "windows")]
impl TapSync {
    fn new(inner: crate::windows_tap::TapSync) -> Self {
        Self {
            inner: Arc::new(Mutex::new(inner)),
            read_state: None,
            write_state: None,
        }
    }
}

#[cfg(target_os = "windows")]
impl AsyncRead for TapSync {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }

        if self.read_state.is_none() {
            let capacity = buf.remaining();
            let inner = self.inner.clone();
            let (tx, rx) = oneshot::channel();
            tokio::task::spawn_blocking(move || {
                let result = {
                    let mut guard = inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    let mut scratch = vec![0u8; capacity];
                    let n = Read::read(&mut *guard, &mut scratch[..]);
                    match n {
                        Ok(n) => {
                            scratch.truncate(n);
                            Ok(scratch)
                        }
                        Err(e) => Err(e),
                    }
                };
                let _ = tx.send(result);
            });
            self.read_state = Some(ReadState { recv: rx });
        }

        let state = self.read_state.as_mut().expect("read state initialized");
        match Pin::new(&mut state.recv).poll(cx) {
            Poll::Ready(Ok(Ok(bytes))) => {
                self.read_state = None;
                if !bytes.is_empty() {
                    buf.put_slice(&bytes);
                }
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Ok(Err(err))) => {
                self.read_state = None;
                Poll::Ready(Err(err))
            }
            Poll::Ready(Err(_)) => {
                self.read_state = None;
                Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "read task was cancelled",
                )))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(target_os = "windows")]
impl AsyncWrite for TapSync {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.write_state.is_none() {
            let inner = self.inner.clone();
            let payload = buf.to_vec();
            let (tx, rx) = oneshot::channel();
            tokio::task::spawn_blocking(move || {
                let result = {
                    let mut guard = inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    Write::write(&mut *guard, &payload)
                };
                let _ = tx.send(result);
            });
            self.write_state = Some(WriteState { recv: rx });
        }

        let state = self.write_state.as_mut().expect("write state initialized");
        match Pin::new(&mut state.recv).poll(cx) {
            Poll::Ready(Ok(n)) => {
                self.write_state = None;
                Poll::Ready(Ok(n))
            }
            Poll::Ready(Err(_)) => {
                self.write_state = None;
                Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "write task was cancelled",
                )))
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[cfg(not(target_os = "windows"))]
pub struct TapSync;

#[cfg(not(target_os = "windows"))]
impl Read for TapSync {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "TAP devices are only supported on Windows.",
        ))
    }
}

#[cfg(not(target_os = "windows"))]
impl Write for TapSync {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "TAP devices are only supported on Windows.",
        ))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
impl AsyncRead for TapSync {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "TAP devices are only supported on Windows.",
        )))
    }
}

#[cfg(not(target_os = "windows"))]
impl AsyncWrite for TapSync {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "TAP devices are only supported on Windows.",
        )))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[cfg(not(target_os = "windows"))]
use anyhow::anyhow;

/// Open the TAP adapter named `device_name`, bring it "connected", assign it
/// `tap_ip/24` and set its MTU to `tap_mtu`.
#[cfg(target_os = "windows")]
pub fn open_and_configure(device_name: &str, tap_ip: &str, tap_mtu: u16) -> Result<TapSync> {
    let inner = crate::windows_tap::open_and_configure(device_name, tap_ip, tap_mtu)?;
    Ok(TapSync::new(inner))
}

#[cfg(not(target_os = "windows"))]
pub fn open_and_configure(_device_name: &str, _tap_ip: &str, _tap_mtu: u16) -> Result<TapSync> {
    Err(anyhow!(
        "TAP devices are only supported on Windows. Run the program on a Windows machine."
    ))
}
