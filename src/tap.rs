//! Cross-platform facade over the TAP device.
//!
//! Only Windows has a real implementation (`windows_tap`). The other platforms
//! return an error at runtime (the binary is Windows-only for shipping), but
//! the stub kept here lets the crate *compile* on Linux so the framing unit
//! tests can run in CI / the dev codespace.

use anyhow::{anyhow, Result};
use tokio::fs::File;

/// Open the TAP adapter named `device_name`, bring it "connected",  assign it
/// `tap_ip/24` and set its MTU to `tap_mtu`.
#[cfg(target_os = "windows")]
pub fn open_and_configure(device_name: &str, tap_ip: &str, tap_mtu: u16) -> Result<File> {
    let dev = windows_tap::open_and_configure(device_name, tap_ip, tap_mtu)?;
    Ok(dev)
}

#[cfg(not(target_os = "windows"))]
pub fn open_and_configure(_device_name: &str, _tap_ip: &str, _tap_mtu: u16) -> Result<File> {
    Err(anyhow!(
        "TAP-laitteet tuettu vain Windowsilla. Aja ohjelma Windows-koneella."
    ))
}
