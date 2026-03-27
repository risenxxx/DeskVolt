//! Corsair headset battery support (Void, Virtuoso).
//!
//! Protocol reference:
//! - HeadsetControl: https://github.com/Sapd/HeadsetControl

#![cfg(windows)]

use crate::device::{ChargingState, Device, DeviceError, DeviceIcon};
use crate::log;
use hidapi::{HidApi, HidDevice};

// Corsair Vendor ID
const CORSAIR_VID: u16 = 0x1B1C;

// Corsair Void/Virtuoso PIDs
const VOID_PIDS: &[u16] = &[
    0x0A14, // Void Wireless
    0x0A16, // Void Pro Wireless
    0x0A17, // Void Pro USB
    0x0A1A, // Void Pro Wireless (2019)
    0x0A55, // Void Elite Wireless
    0x0A51, // Void RGB Elite Wireless
    0x0A65, // Void RGB Elite USB
    0x1B27, // Void Wireless (older)
    0x1B2A, // Void Pro Wireless (older)
];

const VIRTUOSO_PIDS: &[u16] = &[
    0x0A40, // Virtuoso RGB Wireless
    0x0A41, // Virtuoso RGB Wireless (alt)
    0x0A42, // Virtuoso RGB Wireless SE
    0x0A44, // Virtuoso RGB Wireless XT
];

// HID details
const TARGET_USAGE_PAGE: u16 = 0xFFC5;
const TARGET_INTERFACE: i32 = 3;

// Commands
const CMD_BATTERY: [u8; 2] = [0xC9, 0x64];
const MSG_SIZE: usize = 64;

pub struct CorsairVoid {
    device: HidDevice,
    battery_percent: Option<u8>,
    charging_state: ChargingState,
    connected: bool,
    product_name: &'static str,
}

impl CorsairVoid {
    pub fn discover() -> Option<Self> {
        log::log("Corsair: Starting device discovery");
        let api = HidApi::new().ok()?;

        // Try Void series
        for &pid in VOID_PIDS {
            if let Some(device) = Self::try_open(&api, pid, "Corsair Void") {
                return Some(device);
            }
        }

        // Try Virtuoso series
        for &pid in VIRTUOSO_PIDS {
            if let Some(device) = Self::try_open(&api, pid, "Corsair Virtuoso") {
                return Some(device);
            }
        }

        log::log("Corsair: No devices found");
        None
    }

    fn try_open(api: &HidApi, pid: u16, name: &'static str) -> Option<Self> {
        // First try with usage page filter
        let devices: Vec<_> = api
            .device_list()
            .filter(|d| {
                d.vendor_id() == CORSAIR_VID
                    && d.product_id() == pid
                    && d.usage_page() == TARGET_USAGE_PAGE
            })
            .collect();

        if !devices.is_empty() {
            log::log(&format!(
                "Corsair: Found {} devices with PID {:04X}, UP {:04X}",
                devices.len(), pid, TARGET_USAGE_PAGE
            ));

            for device_info in devices {
                if let Ok(device) = device_info.open_device(api) {
                    let _ = device.set_blocking_mode(false);

                    let mut headset = Self {
                        device,
                        battery_percent: None,
                        charging_state: ChargingState::Unknown,
                        connected: true,
                        product_name: name,
                    };

                    if headset.query_battery().is_ok() {
                        log::log(&format!("Corsair: Connected to {}", name));
                        return Some(headset);
                    }
                }
            }
        }

        // Fallback: try interface 3
        let fallback: Vec<_> = api
            .device_list()
            .filter(|d| {
                d.vendor_id() == CORSAIR_VID
                    && d.product_id() == pid
                    && d.interface_number() == TARGET_INTERFACE
            })
            .collect();

        for device_info in fallback {
            if let Ok(device) = device_info.open_device(api) {
                let _ = device.set_blocking_mode(false);

                let mut headset = Self {
                    device,
                    battery_percent: None,
                    charging_state: ChargingState::Unknown,
                    connected: true,
                    product_name: name,
                };

                if headset.query_battery().is_ok() {
                    log::log(&format!("Corsair: Fallback connected to {}", name));
                    return Some(headset);
                }
            }
        }

        None
    }

    fn query_battery(&mut self) -> Result<(), DeviceError> {
        // Build request
        let mut request = [0u8; MSG_SIZE];
        request[0] = CMD_BATTERY[0];
        request[1] = CMD_BATTERY[1];

        // Send request
        self.device
            .write(&request)
            .map_err(|e| DeviceError::CommunicationError(e.to_string()))?;

        // Read response
        let mut response = [0u8; MSG_SIZE];
        let _ = self.device.set_blocking_mode(true);

        let bytes_read = self.device
            .read_timeout(&mut response, 1000)
            .map_err(|e| DeviceError::CommunicationError(e.to_string()))?;

        let _ = self.device.set_blocking_mode(false);

        if bytes_read < 5 {
            self.connected = false;
            return Err(DeviceError::CommunicationError("Short response".to_string()));
        }

        log::log(&format!(
            "Corsair: Response: {:02X} {:02X} {:02X} {:02X} {:02X}",
            response[0], response[1], response[2], response[3], response[4]
        ));

        // Parse response:
        // byte[2] = battery level (0-100) with mic flag in bit 7
        // byte[4] = status (0=disconnected, 1=normal, 2=low, 4/5=charging)
        let battery = response[2] & 0x7F; // Mask off mic bit
        let status = response[4];

        if status == 0 {
            // Headset disconnected from base station
            self.connected = false;
            // Keep last known battery
            self.charging_state = ChargingState::Unknown;
            log::log("Corsair: Headset disconnected");
            return Ok(());
        }

        if battery > 100 {
            return Err(DeviceError::ProtocolError("Invalid battery value".to_string()));
        }

        self.battery_percent = Some(battery);
        self.charging_state = match status {
            4 | 5 => ChargingState::Charging,
            _ if battery >= 100 => ChargingState::Full,
            _ => ChargingState::Discharging,
        };
        self.connected = true;

        log::log(&format!(
            "Corsair: Battery {}%, status: {}",
            battery, status
        ));

        Ok(())
    }
}

impl Device for CorsairVoid {
    fn id(&self) -> &str {
        "corsair-void"
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
