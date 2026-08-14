#![cfg(target_os = "windows")]

use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{IntoRawHandle, RawHandle};
use std::process::Command;
use anyhow::{anyhow, Result};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, ERROR_IO_PENDING, ERROR_INVALID_FUNCTION,
};
use windows_sys::Win32::Storage::FileSystem::{
    ReadFile, WriteFile, FILE_FLAG_OVERLAPPED,
};
use windows_sys::Win32::System::IO::{DeviceIoControl, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExA, RegQueryValueExA, HKEY_LOCAL_MACHINE, KEY_READ, REG_SZ,
};
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};

pub struct TapSync {
    handle: HANDLE,
}

unsafe impl Send for TapSync {}
unsafe impl Sync for TapSync {}

impl TapSync {
    pub fn from_raw_handle(handle: RawHandle) -> Self {
        Self {
            handle: handle as HANDLE,
        }
    }

    fn io_result_from_err(code: u32) -> io::Error {
        io::Error::from_raw_os_error(code as i32)
    }

    fn overlapped_read(&self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if event == 0 {
            return Err(io::Error::last_os_error());
        }

        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        overlapped.hEvent = event;

        let mut bytes_read = 0u32;
        let ok = unsafe {
            ReadFile(
                self.handle,
                buf.as_mut_ptr() as *mut _,
                buf.len() as u32,
                &mut bytes_read,
                &mut overlapped,
            )
        };

        if ok == 0 {
            let err = unsafe { GetLastError() };
            if err == ERROR_IO_PENDING {
                unsafe { WaitForSingleObject(event, INFINITE) };
                let mut transferred = 0u32;
                let got = unsafe { GetOverlappedResult(self.handle, &mut overlapped, &mut transferred, 0) };
                unsafe { CloseHandle(event) };
                if got == 0 {
                    return Err(Self::io_result_from_err(unsafe { GetLastError() }));
                }
                return Ok(transferred as usize);
            }
            unsafe { CloseHandle(event) };
            return Err(Self::io_result_from_err(err));
        }

        unsafe { CloseHandle(event) };
        Ok(bytes_read as usize)
    }

    fn overlapped_write(&self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if event == 0 {
            return Err(io::Error::last_os_error());
        }

        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        overlapped.hEvent = event;

        let mut bytes_written = 0u32;
        let ok = unsafe {
            WriteFile(
                self.handle,
                buf.as_ptr() as *const _,
                buf.len() as u32,
                &mut bytes_written,
                &mut overlapped,
            )
        };

        if ok == 0 {
            let err = unsafe { GetLastError() };
            if err == ERROR_IO_PENDING {
                unsafe { WaitForSingleObject(event, INFINITE) };
                let mut transferred = 0u32;
                let got = unsafe { GetOverlappedResult(self.handle, &mut overlapped, &mut transferred, 0) };
                unsafe { CloseHandle(event) };
                if got == 0 {
                    return Err(Self::io_result_from_err(unsafe { GetLastError() }));
                }
                return Ok(transferred as usize);
            }
            unsafe { CloseHandle(event) };
            return Err(Self::io_result_from_err(err));
        }

        unsafe { CloseHandle(event) };
        Ok(bytes_written as usize)
    }
}

impl Read for TapSync {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.overlapped_read(buf)
    }
}

impl Write for TapSync {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.overlapped_write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}



impl Drop for TapSync {
    fn drop(&mut self) {
        if self.handle != 0 {
            unsafe {
                CloseHandle(self.handle);
            }
            self.handle = 0;
        }
    }
}

const TAP_WIN_IOCTL_SET_MEDIA_STATUS: u32 = 0x00220018;

struct RawHandleGuard(RawHandle);

impl RawHandleGuard {
    fn new(handle: RawHandle) -> Self {
        Self(handle)
    }

    fn as_handle(&self) -> HANDLE {
        self.0 as HANDLE
    }

    fn into_raw(self) -> RawHandle {
        let handle = self.0;
        std::mem::forget(self);
        handle
    }
}

impl Drop for RawHandleGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                CloseHandle(self.0 as HANDLE);
            }
        }
    }
}

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
                if res == 259 {
                    break; // ERROR_NO_MORE_ITEMS: end of enumeration.
                }

                eprintln!("[WARNING] RegEnumKeyExA failed for adapter index {} with error code {}.", index, res);
                break;
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
                let mut value_type = 0u32;
                let name_key = b"Name\0";

                if RegQueryValueExA(
                    subkey,
                    name_key.as_ptr(),
                    std::ptr::null_mut(),
                    &mut value_type,
                    data.as_mut_ptr(),
                    &mut data_len,
                ) == 0 {
                    if value_type != REG_SZ {
                        RegCloseKey(subkey);
                        index += 1;
                        continue;
                    }

                    let name = if data_len > 0 {
                        let bytes = &data[..data_len as usize];
                        std::str::from_utf8(bytes).unwrap_or("").trim_end_matches('\0')
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
fn set_media_connected(handle: HANDLE, device_name: &str) -> Result<()> {
    eprintln!("[DEBUG] Attempting to set TAP media status...");
    let mut in_buf: [u8; 4] = 1u32.to_ne_bytes();
    let mut bytes_returned: u32 = 0;

    // Create an event for the overlapped operation
    let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
    if event == 0 {
        let err = unsafe { GetLastError() };
        eprintln!("[DEBUG] Failed to create event, error code: {}", err);
        return Err(anyhow!("Failed to create event for overlapped IO (GetLastError={}).", err));
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
        eprintln!("[DEBUG] Media status set successfully (synchronous)");
        unsafe { CloseHandle(event) };
        Ok(())
    } else {
        let err = unsafe { GetLastError() };
        eprintln!("[DEBUG] DeviceIoControl returned error: {} ({})", err, format_error(err));
        if err == ERROR_IO_PENDING {
            // Asynchronous completion - wait for the operation to finish
            eprintln!("[DEBUG] Waiting for async IO...");
            unsafe { WaitForSingleObject(event, INFINITE) };
            let mut transferred = 0u32;
            let got = unsafe { GetOverlappedResult(handle, &overlapped, &mut transferred, 0) };
            unsafe { CloseHandle(event) };
            if got != 0 {
                eprintln!("[DEBUG] Async IO completed successfully");
                Ok(())
            } else {
                let async_err = unsafe { GetLastError() };
                eprintln!("[DEBUG] Async IO failed with error: {}", async_err);
                Err(anyhow!("Overlapped DeviceIoControl failed (GetLastError={}).", async_err))
            }
        } else if err == ERROR_INVALID_FUNCTION {
            // IOCTL not supported. Try netsh enable as a workaround, then attempt synchronous IOCTL.
            eprintln!("[DEBUG] IOCTL not supported by driver, trying netsh workaround...");
            unsafe { CloseHandle(event) };
            
            // Try to enable the interface via netsh using the actual adapter name.
            let netsh_name = format!("name={}", device_name);
            eprintln!("[DEBUG] Running: netsh interface set interface {} admin=ENABLED", netsh_name);
            let netsh_result = Command::new("netsh")
                .args(["interface", "set", "interface", &netsh_name, "admin=ENABLED"])
                .output();
            
            match netsh_result {
                Ok(output) => {
                    eprintln!("[DEBUG] netsh stdout: {}", String::from_utf8_lossy(&output.stdout));
                    if !output.stderr.is_empty() {
                        eprintln!("[DEBUG] netsh stderr: {}", String::from_utf8_lossy(&output.stderr));
                    }
                }
                Err(e) => {
                    eprintln!("[DEBUG] netsh command failed: {}", e);
                }
            }
            
            // Try synchronous IOCTL as fallback
            eprintln!("[DEBUG] Attempting synchronous DeviceIoControl...");
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
                eprintln!("[DEBUG] Synchronous DeviceIoControl succeeded");
                Ok(())
            } else {
                let code = unsafe { GetLastError() };
                let msg = format_error(code);
                eprintln!("[WARNING] Synchronous DeviceIoControl also failed: {} ({})", code, msg);
                Err(anyhow!("Synchronous DeviceIoControl fallback failed (GetLastError={}): {}", code, msg))
            }
        } else {
            // Immediate failure
            unsafe { CloseHandle(event) };
            let msg = format_error(err);
            eprintln!("[ERROR] Failed to set TAP adapter media status: {} ({})", err, msg);
            Err(anyhow!("Failed to set TAP adapter media status (GetLastError={}): {}.", err, msg))
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
/// address and MTU, and returns a single TAP handle wrapper.
///
/// The TAP driver does not like duplicated read/write handles on Windows; the
/// bridge must therefore keep a single underlying handle and serialize read/write
/// access instead of calling `tokio::io::split`.
pub fn open_and_configure(device_name: &str, tap_ip: &str, tap_mtu: u16) -> Result<TapSync> {
    let guid = find_tap_guid(device_name)?;
    let device_path = format!("\\\\.\\Global\\{}.tap", guid);
    eprintln!("[DEBUG] Opening TAP device at: {}", device_path);

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(FILE_FLAG_OVERLAPPED)
        .open(&device_path)?;

    eprintln!("[DEBUG] TAP device opened successfully");

    // Bring the interface online before assigning the IP and MTU so the adapter
    // is not configured while still reporting as unplugged.
    let raw_handle = file.into_raw_handle();
    let handle_guard = RawHandleGuard::new(raw_handle);
    let handle: HANDLE = handle_guard.as_handle();

    // Some TAP drivers don't support this IOCTL, but if the fallback fails we
    // must surface the error rather than silently continuing with a misconfigured adapter.
    if let Err(e) = set_media_connected(handle, device_name) {
        eprintln!("[WARNING] Failed to set media status: {}.", e);
        return Err(e);
    }

    eprintln!("[DEBUG] Configuring TAP adapter IP and MTU...");
    configure_tap(device_name, tap_ip, tap_mtu)?;
    eprintln!("[DEBUG] TAP adapter configured");

let handle = handle_guard.into_raw();
    eprintln!("[DEBUG] Returning synchronous TAP handle");
    Ok(TapSync::from_raw_handle(handle))
}
