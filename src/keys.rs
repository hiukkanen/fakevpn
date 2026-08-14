use anyhow::{Context, Result};
use iroh::SecretKey;
use std::fs;
use std::path::{Path, PathBuf};

/// Returns the default key file path: %APPDATA%/fakevpn/key (Windows) or
/// $HOME/.fakevpn/key (other platforms).
fn default_key_path() -> Result<PathBuf> {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(PathBuf::from))
        .context("APPDATA/HOME environment variable not found for key file")?;
    Ok(base.join("fakevpn").join("key"))
}

/// Loads an existing SecretKey from a file, or creates a new one and saves it.
/// Stable Node ID: remains the same across restarts, so it can be shared
/// with the other party once.
/// 
/// If `override_path` is provided, it will be used instead of the default path.
pub fn load_or_generate(override_path: Option<&Path>) -> Result<SecretKey> {
    let path = if let Some(p) = override_path {
        p.to_path_buf()
    } else {
        default_key_path()?
    };

    if path.exists() {
        let bytes = fs::read(&path).with_context(|| {
            format!("Failed to read key file: {}", path.display())
        })?;
        if bytes.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            return Ok(SecretKey::from_bytes(&arr));
        }
        return Err(anyhow::anyhow!(
            "key file exists at '{}' but is not a valid 32-byte private key; refusing to overwrite it",
            path.display()
        ));
    }

    let key = SecretKey::generate();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .with_context(|| format!("Failed to create key file: {}", path.display()))?;
        let bytes = key.to_bytes();
        file.write_all(&bytes)
            .with_context(|| format!("Failed to write key file: {}", path.display()))?;
        file.sync_all()?;
        return Ok(key);
    }

    #[cfg(not(unix))]
    {
        fs::write(&path, key.to_bytes()).with_context(|| {
            format!("Failed to write key file: {}", path.display())
        })?;
        return Ok(key);
    }
}

