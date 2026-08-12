#![cfg(target_os = "windows")]

use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{FromRawHandle, IntoRawHandle};
use std::process::Command;
use anyhow::{anyhow, Result};
use tokio::fs::File;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, ERROR_IO_PENDING, ERROR_INVALID_FUNCTION};
use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OVERLAPPED;
use windows_sys::Win32::System::IO::{DeviceIoControl, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExA, RegQueryValueExA, HKEY_LOCAL_MACHINE, KEY_READ,
};
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};

// TAP-Windows6 "set media status connected" ioctl. Driver-defined, METHOD_BUFFERED,
// FILE_ANY_ACCESS, function 4. Value is 0x80000418. Setting the in-buffer to 1
// connects the adapter (otherwise it appears "cable unplugged" to the OS).
const TAP_WIN_IOCTL_SET_MEDIA_STATUS: u32 = 0x80000418;

// Find the TAP device GUID from the Windows registry based on its name (e.g., "FC-TAP")
fn find_tap_guid(device_name: &str) -> Result<String> {
    unsafe {
        let network_cards_path = b"SYSTEM\\CurrentControlSet\\Control\\Network\\{4D36E972-E325-11CE-BFC1-08002BE10318}\0";
        let mut hkey: isize = 0;

        if RegOpenKeyExA(HKEY_LOCAL_MACHINE, network_cards_path.as_ptr(), 0, KEY_READ, &mut hkey) != 0 {
            return Err(anyhow!("Could not open Windows network adapters registry."));
        }

        let mut index = 0;
        let mut subkey_name = [0u8; 256];

        // Read registry and find the adapter whose name matches the search
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
                break; // End reached or error
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
                    let name = if data_len > 0 {
                        std::str::from_utf8(&data[..(data_len as usize).saturating_sub(1)]).unwrap_or("").trim_matches('\0')
                    } else {
                        ""
                    };
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
        Err(anyhow!("TAP device with name '{}' not found in Windows.", device_name))
    }
}

/// Sets the TAP adapter to "connected" (media status connected). Without this,
/// the adapter appears to the system as "cable unplugged".
/// 
/// Note: The handle must have been opened with FILE_FLAG_OVERLAPPED, so we must
/// provide a valid OVERLAPPED structure and wait for completion.
fn set_media_connected(handle: HANDLE) -> Result<()> {
    let mut in_buf: [u8; 4] = 1u32.to_ne_bytes();
    let mut bytes_returned: u32 = 0;

    // Create an event for the overlapped operation
    let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
    if event == 0 {
        return Err(anyhow!("Failed to create event for overlapped IO (GetLastError={}).", unsafe { GetLastError() }));
    }

    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    overlapped.hEvent = event;

    let ok = unsafe {
        DeviceIoControl(
            handle,
            TAP_WIN_IOCTL_SET_MEDIA_STATUS,
            in_buf.as_mut_ptr() as *mut _,
            in_buf.len() as u32,
            std::ptr::null_mut(),
            0,
            &mut bytes_returned,
            &mut overlapped,
        )
    };

    // Handle the result - DeviceIoControl may return 0 with ERROR_IO_PENDING for async completion
    if ok != 0 {
        // Synchronous completion
        unsafe { CloseHandle(event) };
        Ok(())
    } else {
        let err = unsafe { GetLastError() };
        if err == ERROR_IO_PENDING {
            // Asynchronous completion - wait for the operation to finish
            unsafe { WaitForSingleObject(event, INFINITE) };
            let mut transferred = 0u32;
            let got = unsafe { GetOverlappedResult(handle, &overlapped, &mut transferred, 0) };
            unsafe { CloseHandle(event) };
            if got != 0 {
                Ok(())
            } else {
                Err(anyhow!("Overlapped DeviceIoControl failed (GetLastError={}).", unsafe { GetLastError() }))
            }
        } else if err == ERROR_INVALID_FUNCTION {
            // Some TAP drivers/dozens of system configurations do not support
            // overlapped DeviceIoControl for this IOCTL. Try a synchronous call
            // (no OVERLAPPED) as a fallback.
            unsafe { CloseHandle(event) };
            // Try to enable the interface via netsh as a best-effort workaround
            // for drivers that don't support this IOCTL. This sometimes helps on
            // systems where the adapter is administratively disabled.
            let _ = Command::new("netsh")
                .args(["interface", "set", "interface", &format!("name=\\\"{}\\\"", "FC-TAP"), "admin=ENABLED"])
                .status();
            let mut bytes_returned: u32 = 0;
            let ok_sync = unsafe {
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
            if ok_sync != 0 {
                Ok(())
            } else {
                let code = unsafe { GetLastError() };
                let msg = format_error(code);
                Err(anyhow!("Synchronous DeviceIoControl fallback failed (GetLastError={}): {}", code, msg))
            }
        } else {
            // Immediate failure
            unsafe { CloseHandle(event) };
            Err(anyhow!("Failed to set TAP adapter media status (GetLastError={}).", err))
        }
    }
}

// Provide a descriptive string for common Windows error codes.
fn format_error(code: u32) -> &'static str {
    match code {
        1 => "Invalid function (IOCTL not supported by driver)",
        2 => "File not found",
        5 => "Access denied (administrator privileges may be required)",
        6 => "Invalid handle",
        32 => "The process cannot access the file because it is being used by another process",
        87 => "The parameter is incorrect",
        _ => "Unknown Windows error",
    }
}

/// Configures the TAP adapter with a static IP address (255.255.255.0) and MTU
/// using the netsh command. Requires Administrator privileges.
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
            "Failed to set TAP device IP address. Run the program as Administrator."
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
        return Err(anyhow!("Failed to set TAP device MTU."));
    }
    Ok(())
}

/// Opens the TAP device by name, sets it to "connected", configures the IP
/// address and MTU, and returns an async file handle.
pub fn open_and_configure(device_name: &str, tap_ip: &str, tap_mtu: u16) -> Result<File> {
    let guid = find_tap_guid(device_name)?;
    let device_path = format!("\\\\.\\Global\\{}.tap", guid);

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(FILE_FLAG_OVERLAPPED)
        .open(&device_path)?;

    // Set media status to "connected" before handing the handle to tokio.
    let raw_handle = file.into_raw_handle();
    let handle: HANDLE = raw_handle as isize;
    
    // Configure IP and MTU first, as it doesn't require the raw handle
    configure_tap(device_name, tap_ip, tap_mtu)?;
    
    // Now set media status - if this fails, we need to close the handle
    if let Err(e) = set_media_connected(handle) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
        return Err(e);
    }

    // Convert the standard handle to a tokio async file.
    let tokio_file = unsafe { File::from_raw_handle(raw_handle) };
    Ok(tokio_file)
}
