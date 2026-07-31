use anyhow::{Context, Result};
use iroh::SecretKey;
use std::fs;
use std::path::PathBuf;

/// Returns the key file path. Can be overridden with the `FAKEVPN_KEY_FILE`
/// environment variable; otherwise uses %APPDATA%/fakevpn/key (Windows) or
/// $HOME/.fakevpn/key (other platforms).
fn key_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("FAKEVPN_KEY_FILE") {
        return Ok(PathBuf::from(p));
    }
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(PathBuf::from))
        .context("APPDATA/HOME environment variable not found for key file")?;
    Ok(base.join("fakevpn").join("key"))
}

/// Loads an existing SecretKey from a file, or creates a new one and saves it.
/// Stable Node ID: remains the same across restarts, so it can be shared
/// with the other party once.
pub fn load_or_generate() -> Result<SecretKey> {
    let path = key_path()?;
    if path.exists() {
        let bytes = fs::read(&path).with_context(|| {
            format!("Failed to read key file: {}", path.display())
        })?;
        if bytes.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            return Ok(SecretKey::from_bytes(&arr));
        }
        // Wrong file size: create a new key.
    }
    let key = SecretKey::generate();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, key.to_bytes()).with_context(|| {
        format!("Failed to write key file: {}", path.display())
    })?;
    Ok(key)
}
