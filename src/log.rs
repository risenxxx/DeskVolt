//! Debug logging to file for troubleshooting device issues.

#![cfg(windows)]

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static LOG_FILE: Mutex<Option<File>> = Mutex::new(None);

/// Initialize logging to a file in the same directory as the executable.
pub fn init() {
    if let Some(path) = get_log_path() {
        if let Ok(file) = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
        {
            if let Ok(mut guard) = LOG_FILE.lock() {
                *guard = Some(file);
            }
        }
    }
    log("DeskVolt started");
    log(&format!("Version: {}", env!("CARGO_PKG_VERSION")));
}

/// Log a message to the debug file.
pub fn log(msg: &str) {
    if let Ok(mut guard) = LOG_FILE.lock() {
        if let Some(ref mut file) = *guard {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let _ = writeln!(file, "[{}] {}", timestamp, msg);
            let _ = file.flush();
        }
    }
}

/// Log HID device enumeration for debugging.
pub fn log_hid_devices() {
    log("=== HID Device Enumeration ===");

    if let Ok(api) = hidapi::HidApi::new() {
        for device in api.device_list() {
            let vid = device.vendor_id();
            let pid = device.product_id();
            let interface = device.interface_number();
            let manufacturer = device.manufacturer_string().unwrap_or_default();
            let product = device.product_string().unwrap_or_default();
            let usage_page = device.usage_page();
            let usage = device.usage();

            log(&format!(
                "  VID:{:04X} PID:{:04X} IF:{} Usage:{:04X}/{:04X} {} - {}",
                vid, pid, interface, usage_page, usage, manufacturer, product
            ));
        }
    } else {
        log("  Failed to initialize HID API");
    }

    log("=== End HID Enumeration ===");
}

fn get_log_path() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .map(|dir| dir.join("deskvolt.log"))
}
