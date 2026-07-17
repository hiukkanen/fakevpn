#![cfg(target_os = "windows")]

use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::FromRawHandle;
use tokio::fs::File;
use anyhow::{anyhow, Result};
use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OVERLAPPED;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExA, RegQueryValueExA, HKEY_LOCAL_MACHINE, KEY_READ,
};

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

pub fn open_tap_device(device_name: &str) -> Result<File> {
    let guid = find_tap_guid(device_name)?;
    let device_path = format!("\\\\.\\Global\\{}.tap", guid);

    // Avataan laite Windowsin asynkronisella (OVERLAPPED) tilalla
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(FILE_FLAG_OVERLAPPED)
        .open(&device_path)?;

    // Muunnetaan standardi synkroninen kahva Tokion asynkroniseksi tiedostoksi
    let raw_handle = std::os::windows::io::IntoRawHandle::into_raw_handle(file);
    let tokio_file = unsafe { File::from_raw_handle(raw_handle) };

    Ok(tokio_file)
}
