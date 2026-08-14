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
pub use crate::windows_tap::TapSync;

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
    crate::windows_tap::open_and_configure(device_name, tap_ip, tap_mtu)
}

#[cfg(not(target_os = "windows"))]
pub fn open_and_configure(_device_name: &str, _tap_ip: &str, _tap_mtu: u16) -> Result<TapSync> {
    Err(anyhow!(
        "TAP devices are only supported on Windows. Run the program on a Windows machine."
    ))
}
