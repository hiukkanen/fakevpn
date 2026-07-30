use anyhow::{Context, Result};
use iroh::SecretKey;
use std::fs;
use std::path::PathBuf;

/// Palauttaa avaintiedoston polun. Voidaan ylikirjoittaa `FAKEVPN_KEY_FILE`
/// -ympäristömuuttujalla; muuten käytetään %APPDATA%/fakevpn/key (Windows) taikka
/// $HOME/.fakevpn/key (muut alustat).
fn key_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("FAKEVPN_KEY_FILE") {
        return Ok(PathBuf::from(p));
    }
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(PathBuf::from))
        .context("Ympäristömuuttujaa APPDATA/HOME ei löydy avaintiedostolle")?;
    Ok(base.join("fakevpn").join("key"))
}

/// Lataa olemassa olevan SecretKeyn tiedostosta, tai luo uuden ja tallentaa sen.
/// Stabiili Node ID: pysyy samana uudelleenkäynnistyksissä, joten se voidaan jakaa
/// vastapuolelle kerran.
pub fn load_or_generate() -> Result<SecretKey> {
    let path = key_path()?;
    if path.exists() {
        let bytes = fs::read(&path).with_context(|| {
            format!("Avaintiedoston lukeminen epäonnistui: {}", path.display())
        })?;
        if bytes.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            return Ok(SecretKey::from_bytes(&arr));
        }
        // Väärän kokoinen tiedosto: luodaan uusi avain.
    }
    let key = SecretKey::generate();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, key.to_bytes()).with_context(|| {
        format!("Avaintiedoston kirjoittaminen epäonnistui: {}", path.display())
    })?;
    Ok(key)
}
