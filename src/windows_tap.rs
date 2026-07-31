#![cfg(target_os = "windows")]

use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{FromRawHandle, IntoRawHandle};
use std::process::Command;
use anyhow::{anyhow, Result};
use tokio::fs::File;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OVERLAPPED;
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExA, RegQueryValueExA, HKEY_LOCAL_MACHINE, KEY_READ,
};

// TAP-Windows6 "set media status connected" ioctl. Driver-defined, METHOD_BUFFERED,
// FILE_ANY_ACCESS, function 4. Value is 0x80000418. Setting the in-buffer to 1
// connects the adapter (otherwise it appears "cable unplugged" to the OS).
const TAP_WIN_IOCTL_SET_MEDIA_STATUS: u32 = 0x80000418;

// Etsitään TAP-laitteen GUID Windowsin rekisteristä sen nimen (esim. "FC-TAP") perusteella
fn find_tap_guid(device_name: &str) -> Result<String> {
    unsafe {
        let network_cards_path = b"SYSTEM\\CurrentControlSet\\Control\\Network\\{4D36E972-E325-11CE-BFC1-08002BE10318}\0";
        let mut hkey: isize = 0;

        if RegOpenKeyExA(HKEY_LOCAL_MACHINE, network_cards_path.as_ptr(), 0, KEY_READ, &mut hkey) != 0 {
            return Err(anyhow!("Ei voitu avata Windowsin verkkosovittimien rekisteriä."));
        }

        let mut index = 0;
        let mut subkey_name = [0u8; 256];

        // Luetaan rekisteriä ja etsitään sovitinta, jonka nimi vastaa hakua
        loop {
            let mut name_len = subkey_name.len() as u32;
            let res = windows_sys::Win32::System::Registry::RegEnumKeyExA(
                hkey,
                index,
                subkey_name.as_mut_ptr(),
                &mut name_len,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );

            if res != 0 {
                break; // Loppu saavutettu tai virhe
            }

            let guid_str = std::str::from_utf8(&subkey_name[..name_len as usize]).unwrap_or("");
            let connection_path = format!(
                "SYSTEM\\CurrentControlSet\\Control\\Network\\{{4D36E972-E325-11CE-BFC1-08002BE10318}}\\{}\\Connection\0",
                guid_str
            );

            let mut subkey: isize = 0;
            if RegOpenKeyExA(HKEY_LOCAL_MACHINE, connection_path.as_ptr(), 0, KEY_READ, &mut subkey) == 0 {
                let mut data = [0u8; 256];
                let mut data_len = data.len() as u32;
                let name_key = b"Name\0";

                if RegQueryValueExA(subkey, name_key.as_ptr(), std::ptr::null_mut(), std::ptr::null_mut(), data.as_mut_ptr(), &mut data_len) == 0 {
                    let name = std::str::from_utf8(&data[..data_len as usize - 1]).unwrap_or("").trim_matches('\0');
                    if name == device_name {
                        RegCloseKey(subkey);
                        RegCloseKey(hkey);
                        return Ok(guid_str.to_string());
                    }
                }
                RegCloseKey(subkey);
            }
            index += 1;
        }

        RegCloseKey(hkey);
        Err(anyhow!("TAP-laitetta nimeltä '{}' ei löytynyt Windowsista.", device_name))
    }
}

/// Asettaa TAP-sovittimen "kytketyksi" (media status connected). Ilman tätä
/// sovitin näkyy järjestelmälle "kaapeli irti" -tilassa.
fn set_media_connected(handle: HANDLE) -> Result<()> {
    let mut in_buf: [u8; 4] = 1u32.to_ne_bytes();
    let mut bytes_returned: u32 = 0;
    let ok = unsafe {
        DeviceIoControl(
            handle,
            TAP_WIN_IOCTL_SET_MEDIA_STATUS,
            in_buf.as_mut_ptr() as *mut _,
            in_buf.len() as u32,
            std::ptr::null_mut(),
            0,
            &mut bytes_returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(anyhow!("TAP-sovittimen media-tilan asettaminen epäonnistui."));
    }
    Ok(())
}

/// Määrittää TAP-sovittimelle kiinteän IP-osoitteen (255.255.255.0) ja MTU:n
/// käyttämällä netsh-komentoa. Vaatii Admin-oikeudet.
fn configure_tap(device_name: &str, tap_ip: &str, tap_mtu: u16) -> Result<()> {
    let status = Command::new("netsh")
        .args([
            "interface", "ipv4", "set", "address",
            &format!("name={}", device_name),
            "static", tap_ip, "255.255.255.0",
        ])
        .status()?;
    if !status.success() {
        return Err(anyhow!(
            "TAP-laitteen IP-osoitteen asettaminen epäonnistui. Aja ohjelma Administrator-oikeuksilla."
        ));
    }

    let status = Command::new("netsh")
        .args([
            "interface", "ipv4", "set", "subinterface",
            &format!("{}", device_name),
            &format!("mtu={}", tap_mtu),
            "store=persistent",
        ])
        .status()?;
    if !status.success() {
        return Err(anyhow!("TAP-laitteen MTU:n asettaminen epäonnistui."));
    }
    Ok(())
}

/// Avaa TAP-laitteen nimen perusteella, asettaa sen "kytketyksi", määrittää
/// IP-osoitteen ja MTU:n, ja palauttaa asynkronisen tiedostokahvan.
pub fn open_and_configure(device_name: &str, tap_ip: &str, tap_mtu: u16) -> Result<File> {
    let guid = find_tap_guid(device_name)?;
    let device_path = format!("\\\\.\\Global\\{}.tap", guid);

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(FILE_FLAG_OVERLAPPED)
        .open(&device_path)?;

    // Asetetaan media status "connected" ennen kuin luovutamme kahvan tokioille.
    let raw_handle = file.into_raw_handle();
    let handle: HANDLE = raw_handle as isize;
    set_media_connected(handle)?;

    // Määritetään IP ja MTU netshillä.
    configure_tap(device_name, tap_ip, tap_mtu)?;

    // Muunnetaan standardi kahva Tokion asynkroniseksi tiedostoksi.
    let tokio_file = unsafe { File::from_raw_handle(raw_handle) };
    Ok(tokio_file)
}
