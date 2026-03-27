//! Logitech HID++ 2.0 protocol implementation for G Pro X Superlight 2/2c.
//!
//! Protocol reference:
//! - Linux kernel: drivers/hid/hid-logitech-hidpp.c
//! - Solaar project: https://github.com/pwr-Solaar/Solaar

#![cfg(windows)]

use crate::device::{ChargingState, Device, DeviceError, DeviceIcon};
use crate::log;
use hidapi::{HidApi, HidDevice};

// Logitech Vendor ID
const LOGITECH_VID: u16 = 0x046d;

// Known Product IDs for G Pro X Superlight 2/2c receiver
// Based on Solaar and libratbag device databases
const SUPERLIGHT_PIDS: &[u16] = &[
    0xc547, // G Pro X Superlight (original)
    0xc54d, // Lightspeed receiver (common)
    0xc09b, // G Pro X Superlight 2 receiver
    0x0af7, // Superlight variant
    0xc547, // Another Lightspeed variant
];

// HID++ uses vendor-specific usage page
const HIDPP_USAGE_PAGE: u16 = 0xFF00;

// HID++ Report IDs
#[allow(dead_code)]
const REPORT_ID_SHORT: u8 = 0x10;
const REPORT_ID_LONG: u8 = 0x11;

// Report lengths
#[allow(dead_code)]
const SHORT_REPORT_LEN: usize = 7;
const LONG_REPORT_LEN: usize = 20;

// Device index (first paired device)
const DEVICE_INDEX: u8 = 0x01;

// HID++ 2.0 Feature codes
#[allow(dead_code)]
const FEATURE_ROOT: u16 = 0x0000;
const FEATURE_UNIFIED_BATTERY: u16 = 0x1004;

// Function indices
const FUNC_ROOT_GET_FEATURE: u8 = 0x00;
const FUNC_BATTERY_GET_STATUS: u8 = 0x10;

// Software ID (ORed with function index)
const SOFTWARE_ID: u8 = 0x01;

pub struct LogitechSuperlight {
    device: HidDevice,
    battery_feature_index: Option<u8>,
    battery_percent: Option<u8>,
    charging_state: ChargingState,
    connected: bool,
}

impl LogitechSuperlight {
    /// Attempt to discover and connect to a Logitech Superlight device.
    pub fn discover() -> Option<Self> {
        log::log("Logitech: Starting device discovery");
        let api = HidApi::new().ok()?;

        // First, try to find devices with HID++ usage page (0xFF00)
        for pid in SUPERLIGHT_PIDS {
            let devices: Vec<_> = api
                .device_list()
                .filter(|d| {
                    d.vendor_id() == LOGITECH_VID
                        && d.product_id() == *pid
                        && d.usage_page() == HIDPP_USAGE_PAGE
                })
                .collect();

            log::log(&format!(
                "Logitech: Found {} devices matching PID {:04X} with HID++ usage page",
                devices.len(),
                pid
            ));

            for device_info in devices {
                log::log(&format!(
                    "Logitech: Trying device: {} (IF {}, Usage {:04X}/{:04X})",
                    device_info.product_string().unwrap_or("Unknown"),
                    device_info.interface_number(),
                    device_info.usage_page(),
                    device_info.usage()
                ));

                if let Ok(device) = device_info.open_device(&api) {
                    let _ = device.set_blocking_mode(false);

                    let mut superlight = Self {
                        device,
                        battery_feature_index: None,
                        battery_percent: None,
                        charging_state: ChargingState::Unknown,
                        connected: true,
                    };

                    if superlight.discover_battery_feature().is_ok() {
                        log::log("Logitech: Successfully connected and discovered battery feature");
                        return Some(superlight);
                    }
                }
            }
        }

        // Fallback: try all Logitech devices with any PID from our list
        log::log("Logitech: HID++ usage page search failed, trying direct open");
        for pid in SUPERLIGHT_PIDS {
            // Log all matching devices for debugging
            let all_devices: Vec<_> = api
                .device_list()
                .filter(|d| d.vendor_id() == LOGITECH_VID && d.product_id() == *pid)
                .collect();

            for device_info in &all_devices {
                log::log(&format!(
                    "Logitech: Available device PID {:04X}: IF {}, Usage {:04X}/{:04X}, {}",
                    pid,
                    device_info.interface_number(),
                    device_info.usage_page(),
                    device_info.usage(),
                    device_info.product_string().unwrap_or("Unknown")
                ));
            }

            // Try to open any matching device
            for device_info in all_devices {
                if let Ok(device) = device_info.open_device(&api) {
                    let _ = device.set_blocking_mode(false);

                    let mut superlight = Self {
                        device,
                        battery_feature_index: None,
                        battery_percent: None,
                        charging_state: ChargingState::Unknown,
                        connected: true,
                    };

                    if superlight.discover_battery_feature().is_ok() {
                        log::log(&format!(
                            "Logitech: Fallback connected via PID {:04X}",
                            pid
                        ));
                        return Some(superlight);
                    }
                }
            }
        }

        log::log("Logitech: No devices found");
        None
    }

    /// Discover the feature index for UNIFIED_BATTERY.
    fn discover_battery_feature(&mut self) -> Result<(), DeviceError> {
        log::log("Logitech: Discovering UNIFIED_BATTERY feature (0x1004)");

        // Build ROOT get_feature request for UNIFIED_BATTERY (0x1004)
        let mut request = [0u8; LONG_REPORT_LEN];
        request[0] = REPORT_ID_LONG;
        request[1] = DEVICE_INDEX;
        request[2] = 0x00; // Feature index 0 = ROOT
        request[3] = FUNC_ROOT_GET_FEATURE | SOFTWARE_ID;
        request[4] = (FEATURE_UNIFIED_BATTERY >> 8) as u8;
        request[5] = (FEATURE_UNIFIED_BATTERY & 0xFF) as u8;

        // Send request
        self.device
            .write(&request)
            .map_err(|e| {
                log::log(&format!("Logitech: Write failed: {}", e));
                DeviceError::CommunicationError(e.to_string())
            })?;

        // Read response (with timeout)
        let mut response = [0u8; LONG_REPORT_LEN];
        let timeout_ms = 1000;

        // Set blocking temporarily for the response
        let _ = self.device.set_blocking_mode(true);

        let bytes_read = self
            .device
            .read_timeout(&mut response, timeout_ms)
            .map_err(|e| {
                log::log(&format!("Logitech: Read failed: {}", e));
                DeviceError::CommunicationError(e.to_string())
            })?;

        let _ = self.device.set_blocking_mode(false);

        if bytes_read == 0 {
            log::log("Logitech: No response from device");
            return Err(DeviceError::CommunicationError(
                "No response from device".to_string(),
            ));
        }

        log::log(&format!(
            "Logitech: Response ({} bytes): {:02X} {:02X} {:02X} {:02X} {:02X}",
            bytes_read, response[0], response[1], response[2], response[3], response[4]
        ));

        // Validate response
        if response[0] != REPORT_ID_LONG || response[1] != DEVICE_INDEX {
            log::log("Logitech: Invalid response header");
            return Err(DeviceError::ProtocolError("Invalid response header".to_string()));
        }

        // Check for error response (feature index 0xFF means error)
        if response[2] == 0xFF {
            log::log("Logitech: Device returned error response");
            return Err(DeviceError::ProtocolError(
                "Device returned error response".to_string(),
            ));
        }

        // Feature index is in params[0] of the response
        let feature_index = response[4];
        if feature_index == 0 {
            log::log("Logitech: UNIFIED_BATTERY feature not supported");
            return Err(DeviceError::ProtocolError(
                "UNIFIED_BATTERY feature not supported".to_string(),
            ));
        }

        log::log(&format!(
            "Logitech: UNIFIED_BATTERY feature found at index {}",
            feature_index
        ));
        self.battery_feature_index = Some(feature_index);
        Ok(())
    }

    /// Query battery status using UNIFIED_BATTERY feature.
    fn query_battery(&mut self) -> Result<(), DeviceError> {
        let feature_index = self
            .battery_feature_index
            .ok_or(DeviceError::ProtocolError("Battery feature not discovered".to_string()))?;

        // Drain any pending data first (notifications, etc.)
        let mut drain_buf = [0u8; LONG_REPORT_LEN];
        while self.device.read_timeout(&mut drain_buf, 10).unwrap_or(0) > 0 {
            log::log(&format!(
                "Logitech: Draining pending data: {:02X} {:02X} {:02X}",
                drain_buf[0], drain_buf[1], drain_buf[2]
            ));
        }

        // Build GET_STATUS request
        let mut request = [0u8; LONG_REPORT_LEN];
        request[0] = REPORT_ID_LONG;
        request[1] = DEVICE_INDEX;
        request[2] = feature_index;
        request[3] = FUNC_BATTERY_GET_STATUS | SOFTWARE_ID;

        // Send request
        self.device
            .write(&request)
            .map_err(|e| DeviceError::CommunicationError(e.to_string()))?;

        // Read responses until we get the one matching our request
        let _ = self.device.set_blocking_mode(true);

        let mut attempts = 0;
        let max_attempts = 5;

        while attempts < max_attempts {
            let mut response = [0u8; LONG_REPORT_LEN];
            let bytes_read = self
                .device
                .read_timeout(&mut response, 1000)
                .map_err(|e| DeviceError::CommunicationError(e.to_string()))?;

            if bytes_read == 0 {
                self.connected = false;
                let _ = self.device.set_blocking_mode(false);
                log::log("Logitech: Battery query - no response");
                return Err(DeviceError::CommunicationError(
                    "No response from device".to_string(),
                ));
            }

            log::log(&format!(
                "Logitech: Response {}: {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}",
                attempts, response[0], response[1], response[2], response[3],
                response[4], response[5], response[6]
            ));

            // Check if this is the response to our battery query
            // Response should have: Report ID, Device Index, Feature Index, Function|SW_ID
            let is_our_response = response[1] == DEVICE_INDEX
                && response[2] == feature_index
                && (response[3] & 0xF0) == (FUNC_BATTERY_GET_STATUS & 0xF0);

            if is_our_response {
                let _ = self.device.set_blocking_mode(false);

                // Parse response:
                // params[0] (byte 4) = state_of_charge (battery %)
                // params[1] (byte 5) = level flags
                // params[2] (byte 6) = charging_status
                // params[3] (byte 7) = external_power_status

                let battery_percent = response[4];
                let charging_status = response[6];

                // Validate battery percentage
                if battery_percent > 100 {
                    log::log(&format!(
                        "Logitech: Invalid battery percentage: {}, using level flags",
                        battery_percent
                    ));
                    // Fall back to level flags if percentage is invalid
                    // Level flags: bit0=critical, bit1=low, bit2=good, bit3=full
                    let level_flags = response[5];
                    let estimated = if level_flags & 0x08 != 0 {
                        100
                    } else if level_flags & 0x04 != 0 {
                        60
                    } else if level_flags & 0x02 != 0 {
                        20
                    } else if level_flags & 0x01 != 0 {
                        5
                    } else {
                        50 // Unknown
                    };
                    self.battery_percent = Some(estimated);
                } else {
                    self.battery_percent = Some(battery_percent);
                }

                self.charging_state = match charging_status {
                    0 => ChargingState::Discharging,
                    1 | 2 => ChargingState::Charging,
                    3 => ChargingState::Full,
                    _ => ChargingState::Unknown,
                };
                self.connected = true;

                log::log(&format!(
                    "Logitech: Battery {}%, charging status: {}",
                    self.battery_percent.unwrap_or(0), charging_status
                ));

                return Ok(());
            }

            // Not our response, might be a notification - try again
            attempts += 1;
        }

        let _ = self.device.set_blocking_mode(false);
        log::log("Logitech: Failed to get battery response after retries");
        Err(DeviceError::CommunicationError(
            "No valid battery response".to_string(),
        ))
    }
}

impl Device for LogitechSuperlight {
    fn id(&self) -> &str {
        "logitech-superlight-2"
    }

    fn name(&self) -> &str {
        "Pro X Superlight 2c"
    }

    fn icon(&self) -> DeviceIcon {
        DeviceIcon::Mouse
    }

    fn battery_percent(&self) -> Option<u8> {
        self.battery_percent
    }

    fn charging_state(&self) -> ChargingState {
        self.charging_state
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn poll(&mut self) -> Result<(), DeviceError> {
        // If we don't have the feature index yet, try to discover it
        if self.battery_feature_index.is_none() {
            self.discover_battery_feature()?;
        }

        self.query_battery()
    }
}
