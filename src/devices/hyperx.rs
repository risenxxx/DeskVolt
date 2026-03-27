//! HyperX headset battery support (Cloud Alpha Wireless).
//!
//! Protocol reference:
//! - HeadsetControl: https://github.com/Sapd/HeadsetControl

#![cfg(windows)]

use crate::device::{ChargingState, Device, DeviceError, DeviceIcon};
use crate::log;
use hidapi::{HidApi, HidDevice};

// HP/HyperX Vendor ID (HP acquired HyperX)
const HYPERX_VID: u16 = 0x03F0;

// HyperX Cloud Alpha Wireless
const CLOUD_ALPHA_PID: u16 = 0x098D;

// Commands
const CMD_BATTERY: [u8; 3] = [0x21, 0xBB, 0x0B];
const CMD_CHARGING: [u8; 3] = [0x21, 0xBB, 0x0C];
const MSG_SIZE: usize = 64;

pub struct HyperXCloudAlpha {
    device: HidDevice,
    battery_percent: Option<u8>,
    charging_state: ChargingState,
    connected: bool,
}

impl HyperXCloudAlpha {
    pub fn discover() -> Option<Self> {
        log::log("HyperX: Starting device discovery");
        let api = HidApi::new().ok()?;

        let devices: Vec<_> = api
            .device_list()
            .filter(|d| d.vendor_id() == HYPERX_VID && d.product_id() == CLOUD_ALPHA_PID)
            .collect();

        log::log(&format!("HyperX: Found {} devices with PID {:04X}", devices.len(), CLOUD_ALPHA_PID));

        for device_info in devices {
            if let Ok(device) = device_info.open_device(&api) {
                let _ = device.set_blocking_mode(false);

                let mut headset = Self {
                    device,
                    battery_percent: None,
                    charging_state: ChargingState::Unknown,
                    connected: true,
                };

                if headset.query_battery().is_ok() {
                    log::log("HyperX: Connected to Cloud Alpha Wireless");
                    return Some(headset);
                }
            }
        }

        log::log("HyperX: No devices found");
        None
    }

    fn query_battery(&mut self) -> Result<(), DeviceError> {
        // Query charging status first
        let mut request = [0u8; MSG_SIZE];
        request[..3].copy_from_slice(&CMD_CHARGING);

        self.device
            .write(&request)
            .map_err(|e| DeviceError::CommunicationError(e.to_string()))?;

        let mut response = [0u8; MSG_SIZE];
        let _ = self.device.set_blocking_mode(true);

        let bytes_read = self.device
            .read_timeout(&mut response, 500)
            .map_err(|e| DeviceError::CommunicationError(e.to_string()))?;

        if bytes_read == 0 {
            self.connected = false;
            let _ = self.device.set_blocking_mode(false);
            return Err(DeviceError::CommunicationError("No response".to_string()));
        }

        // Response byte[3] contains charging status
        let charging = response[3] == 0x01;

        // Query battery level
        request = [0u8; MSG_SIZE];
        request[..3].copy_from_slice(&CMD_BATTERY);

        self.device
            .write(&request)
            .map_err(|e| DeviceError::CommunicationError(e.to_string()))?;

        let bytes_read = self.device
            .read_timeout(&mut response, 500)
            .map_err(|e| DeviceError::CommunicationError(e.to_string()))?;

        let _ = self.device.set_blocking_mode(false);

        if bytes_read == 0 {
            self.connected = false;
            return Err(DeviceError::CommunicationError("No response".to_string()));
        }

        log::log(&format!(
            "HyperX: Response: {:02X} {:02X} {:02X} {:02X}",
            response[0], response[1], response[2], response[3]
        ));

        // Response byte[3] contains battery percentage
        let battery = response[3];

        if battery > 100 {
            return Err(DeviceError::ProtocolError("Invalid battery value".to_string()));
        }

        self.battery_percent = Some(battery);
        self.charging_state = if charging {
            ChargingState::Charging
        } else if battery >= 100 {
            ChargingState::Full
        } else {
            ChargingState::Discharging
        };
        self.connected = true;

        log::log(&format!(
            "HyperX: Battery {}%, charging: {}",
            battery, charging
        ));

        Ok(())
    }
}

impl Device for HyperXCloudAlpha {
    fn id(&self) -> &str {
        "hyperx-cloud-alpha"
    }

    fn name(&self) -> &str {
        "Cloud Alpha Wireless"
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
