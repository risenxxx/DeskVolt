//! SteelSeries Arctis headset battery protocol implementation.
//!
//! Protocol reference:
//! - HeadsetControl project: https://github.com/Sapd/HeadsetControl

#![cfg(windows)]

use crate::device::{ChargingState, Device, DeviceError, DeviceIcon};
use crate::log;
use hidapi::{HidApi, HidDevice};

// SteelSeries Vendor ID
const STEELSERIES_VID: u16 = 0x1038;

// ============================================================================
// Arctis Nova 5 Series (newer protocol)
// ============================================================================

const ARCTIS_NOVA_5_PIDS: &[u16] = &[
    0x2232, // Arctis Nova 5 Base Station
    0x2253, // Arctis Nova 5X Base Station
];

// HID interface and usage page for Nova 5
const NOVA5_INTERFACE: i32 = 3;
const NOVA5_USAGE_PAGE: u16 = 0xFFC0;

// Nova 5 battery command
const NOVA5_CMD_BATTERY: [u8; 2] = [0x00, 0xB0];
const NOVA5_HEADSET_OFFLINE: u8 = 0x02;
const NOVA5_HEADSET_CHARGING: u8 = 0x01;

// ============================================================================
// Arctis 7/Pro Series (legacy protocol)
// ============================================================================

const ARCTIS_7_PIDS: &[u16] = &[
    0x1260, // Arctis 7 2017
    0x12AD, // Arctis 7 2019
    0x1252, // Arctis Pro Wireless
    0x1280, // Arctis 7P
    0x1284, // Arctis 7P+
    0x12B3, // Arctis 7+ 2022
];

// Arctis 7 uses interface 5
const ARCTIS7_INTERFACE: i32 = 5;

// Arctis 7 battery command (legacy)
const ARCTIS7_CMD_BATTERY: [u8; 2] = [0x06, 0x18];

// HID report sizes
const MSG_SIZE: usize = 64;
const STATUS_BUF_SIZE: usize = 128;

pub struct SteelSeriesArctis {
    device: HidDevice,
    battery_percent: Option<u8>,
    charging_state: ChargingState,
    connected: bool,
    product_name: &'static str,
}

impl SteelSeriesArctis {
    /// Attempt to discover and connect to a SteelSeries Arctis Nova 5 device.
    pub fn discover() -> Option<Self> {
        log::log("SteelSeries Nova 5: Starting device discovery");
        let api = HidApi::new().ok()?;

        for pid in ARCTIS_NOVA_5_PIDS {
            // Find device with matching VID/PID, interface 3, usage page 0xFFC0
            let devices: Vec<_> = api
                .device_list()
                .filter(|d| {
                    d.vendor_id() == STEELSERIES_VID
                        && d.product_id() == *pid
                        && d.interface_number() == NOVA5_INTERFACE
                        && d.usage_page() == NOVA5_USAGE_PAGE
                })
                .collect();

            log::log(&format!(
                "SteelSeries Nova 5: Found {} devices matching PID {:04X}, IF {}, UP {:04X}",
                devices.len(), pid, NOVA5_INTERFACE, NOVA5_USAGE_PAGE
            ));

            for device_info in devices {
                log::log(&format!(
                    "SteelSeries Nova 5: Attempting to open device: {}",
                    device_info.product_string().unwrap_or("Unknown")
                ));

                if let Ok(device) = device_info.open_device(&api) {
                    let _ = device.set_blocking_mode(false);

                    let product_name = match *pid {
                        0x2232 => "Arctis Nova 5",
                        0x2253 => "Arctis Nova 5X",
                        _ => "Arctis Nova",
                    };

                    let mut arctis = Self {
                        device,
                        battery_percent: None,
                        charging_state: ChargingState::Unknown,
                        connected: true,
                        product_name,
                    };

                    // Try to query battery to verify connection
                    if arctis.query_battery_nova5().is_ok() {
                        log::log(&format!(
                            "SteelSeries Nova 5: Successfully connected to {}",
                            product_name
                        ));
                        return Some(arctis);
                    }
                }
            }

            // Fallback: try without usage page filter (some drivers may report differently)
            let fallback_devices: Vec<_> = api
                .device_list()
                .filter(|d| {
                    d.vendor_id() == STEELSERIES_VID
                        && d.product_id() == *pid
                        && d.interface_number() == NOVA5_INTERFACE
                })
                .collect();

            log::log(&format!(
                "SteelSeries Nova 5: Fallback search found {} devices for PID {:04X}, IF {}",
                fallback_devices.len(), pid, NOVA5_INTERFACE
            ));

            for device_info in fallback_devices {
                if let Ok(device) = device_info.open_device(&api) {
                    let _ = device.set_blocking_mode(false);

                    let product_name = match *pid {
                        0x2232 => "Arctis Nova 5",
                        0x2253 => "Arctis Nova 5X",
                        _ => "Arctis Nova",
                    };

                    let mut arctis = Self {
                        device,
                        battery_percent: None,
                        charging_state: ChargingState::Unknown,
                        connected: true,
                        product_name,
                    };

                    if arctis.query_battery_nova5().is_ok() {
                        log::log(&format!(
                            "SteelSeries Nova 5: Fallback connected to {}",
                            product_name
                        ));
                        return Some(arctis);
                    }
                }
            }
        }

        log::log("SteelSeries Nova 5: No devices found");
        None
    }

    fn query_battery_nova5(&mut self) -> Result<(), DeviceError> {
        // Build request: [0x00, 0xB0] padded to MSG_SIZE
        let mut request = [0u8; MSG_SIZE];
        request[0] = NOVA5_CMD_BATTERY[0];
        request[1] = NOVA5_CMD_BATTERY[1];

        // Send request
        self.device
            .write(&request)
            .map_err(|e| DeviceError::CommunicationError(e.to_string()))?;

        // Read response (128 bytes as per HeadsetControl)
        let mut response = [0u8; STATUS_BUF_SIZE];
        let _ = self.device.set_blocking_mode(true);

        let bytes_read = self
            .device
            .read_timeout(&mut response, 1000)
            .map_err(|e| DeviceError::CommunicationError(e.to_string()))?;

        let _ = self.device.set_blocking_mode(false);

        if bytes_read == 0 {
            self.connected = false;
            return Err(DeviceError::CommunicationError(
                "No response from device".to_string(),
            ));
        }

        log::log(&format!(
            "SteelSeries Nova 5: Response ({} bytes): {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}",
            bytes_read, response[0], response[1], response[2], response[3], response[4], response[5]
        ));

        // Check if headset is offline (response[1] == 0x02)
        if response[1] == NOVA5_HEADSET_OFFLINE {
            self.connected = false;
            // Keep last known battery_percent - don't clear it
            self.charging_state = ChargingState::Unknown;
            log::log("SteelSeries Nova 5: Headset is offline/disconnected");
            return Ok(()); // Not an error, just disconnected
        }

        // Parse response (from HeadsetControl):
        // response[1] = connection status (0x02 = offline)
        // response[3] = battery percentage (0-100)
        // response[4] = charging status (0x01 = charging)
        let battery = response[3];
        let charging = response[4];

        // Validate battery value
        if battery > 100 {
            log::log(&format!("SteelSeries Nova 5: Invalid battery value: {}", battery));
            return Err(DeviceError::ProtocolError(
                "Invalid battery value".to_string(),
            ));
        }

        self.battery_percent = Some(battery);
        self.charging_state = if charging == NOVA5_HEADSET_CHARGING {
            ChargingState::Charging
        } else if battery >= 100 {
            ChargingState::Full
        } else {
            ChargingState::Discharging
        };
        self.connected = true;

        log::log(&format!(
            "SteelSeries Nova 5: Battery {}%, charging: {}",
            battery, charging == NOVA5_HEADSET_CHARGING
        ));

        Ok(())
    }
}

impl Device for SteelSeriesArctis {
    fn id(&self) -> &str {
        "steelseries-arctis-nova-5"
    }

    fn name(&self) -> &str {
        self.product_name
    }

    fn icon(&self) -> DeviceIcon {
        DeviceIcon::Headset
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
        self.query_battery_nova5()
    }
}

// ============================================================================
// Arctis 7/Pro Series Implementation
// ============================================================================

pub struct SteelSeriesArctis7 {
    device: HidDevice,
    battery_percent: Option<u8>,
    charging_state: ChargingState,
    connected: bool,
    product_name: &'static str,
}

impl SteelSeriesArctis7 {
    pub fn discover() -> Option<Self> {
        log::log("SteelSeries Arctis 7: Starting device discovery");
        let api = HidApi::new().ok()?;

        for &pid in ARCTIS_7_PIDS {
            let devices: Vec<_> = api
                .device_list()
                .filter(|d| {
                    d.vendor_id() == STEELSERIES_VID
                        && d.product_id() == pid
                        && d.interface_number() == ARCTIS7_INTERFACE
                })
                .collect();

            if devices.is_empty() {
                continue;
            }

            log::log(&format!(
                "SteelSeries Arctis 7: Found {} devices with PID {:04X}",
                devices.len(), pid
            ));

            let product_name = match pid {
                0x1260 => "Arctis 7 (2017)",
                0x12AD => "Arctis 7 (2019)",
                0x1252 => "Arctis Pro Wireless",
                0x1280 => "Arctis 7P",
                0x1284 => "Arctis 7P+",
                0x12B3 => "Arctis 7+",
                _ => "Arctis 7",
            };

            for device_info in devices {
                if let Ok(device) = device_info.open_device(&api) {
                    let _ = device.set_blocking_mode(false);

                    let mut headset = Self {
                        device,
                        battery_percent: None,
                        charging_state: ChargingState::Unknown,
                        connected: true,
                        product_name,
                    };

                    if headset.query_battery().is_ok() {
                        log::log(&format!(
                            "SteelSeries Arctis 7: Connected to {}",
                            product_name
                        ));
                        return Some(headset);
                    }
                }
            }
        }

        log::log("SteelSeries Arctis 7: No devices found");
        None
    }

    fn query_battery(&mut self) -> Result<(), DeviceError> {
        // Build request: [0x06, 0x18] padded to 8 bytes
        let mut request = [0u8; 8];
        request[0] = ARCTIS7_CMD_BATTERY[0];
        request[1] = ARCTIS7_CMD_BATTERY[1];

        self.device
            .write(&request)
            .map_err(|e| DeviceError::CommunicationError(e.to_string()))?;

        let mut response = [0u8; 8];
        let _ = self.device.set_blocking_mode(true);

        let bytes_read = self.device
            .read_timeout(&mut response, 1000)
            .map_err(|e| DeviceError::CommunicationError(e.to_string()))?;

        let _ = self.device.set_blocking_mode(false);

        if bytes_read == 0 {
            self.connected = false;
            return Err(DeviceError::CommunicationError("No response".to_string()));
        }

        log::log(&format!(
            "SteelSeries Arctis 7: Response: {:02X} {:02X} {:02X}",
            response[0], response[1], response[2]
        ));

        // Battery percentage is at byte[2], 0-100
        let battery = response[2];

        if battery > 100 {
            // 0xFF or similar means disconnected
            self.connected = false;
            log::log("SteelSeries Arctis 7: Headset disconnected");
            return Ok(());
        }

        self.battery_percent = Some(battery);
        // Arctis 7 doesn't report charging state in same way
        self.charging_state = if battery >= 100 {
            ChargingState::Full
        } else {
            ChargingState::Discharging
        };
        self.connected = true;

        log::log(&format!("SteelSeries Arctis 7: Battery {}%", battery));

        Ok(())
    }
}

impl Device for SteelSeriesArctis7 {
    fn id(&self) -> &str {
        "steelseries-arctis-7"
    }

    fn name(&self) -> &str {
        self.product_name
    }

    fn icon(&self) -> DeviceIcon {
        DeviceIcon::Headset
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
        self.query_battery()
    }
}
