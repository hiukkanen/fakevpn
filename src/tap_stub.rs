#![cfg(not(target_os = "windows"))]

use std::fs::File;
use anyhow::{anyhow, Result};

pub fn open_tap_device(_device_name: &str) -> Result<File> {
    Err(anyhow!("fakevpn requires Windows (TAP-Windows adapter)"))
}
