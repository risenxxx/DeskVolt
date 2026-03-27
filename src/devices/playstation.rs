//! PlayStation controller battery support (DualSense, DualShock 4).
//!
//! Protocol references:
//! - dualsensectl: https://github.com/nowrep/dualsensectl
//! - ds4drv: https://github.com/chrippa/ds4drv

#![cfg(windows)]

use crate::device::{ChargingState, Device, DeviceError, DeviceIcon};
use crate::log;
use hidapi::{HidApi, HidDevice};

// Sony Vendor ID
const SONY_VID: u16 = 0x054C;

// DualSense (PS5)
const DUALSENSE_PID: u16 = 0x0CE6;
const DUALSENSE_EDGE_PID: u16 = 0x0DF2;

// DualShock 4 (PS4)
const DUALSHOCK4_V1_PID: u16 = 0x05C4;
const DUALSHOCK4_V2_PID: u16 = 0x09CC;

/// PlayStation DualSense (PS5) controller.
pub struct DualSense {
    device: HidDevice,
    battery_percent: Option<u8>,
    charging_state: ChargingState,
    connected: bool,
    is_edge: bool,
}

impl DualSense {
    pub fn discover() -> Option<Self> {
        log::log("DualSense: Starting device discovery");
        let api = HidApi::new().ok()?;

        for (pid, is_edge) in [(DUALSENSE_PID, false), (DUALSENSE_EDGE_PID, true)] {
            let devices: Vec<_> = api
                .device_list()
                .filter(|d| d.vendor_id() == SONY_VID && d.product_id() == pid)
                .collect();

            if !devices.is_empty() {
                log::log(&format!("DualSense: Found {} devices with PID {:04X}", devices.len(), pid));
            }

            for device_info in devices {
                if let Ok(device) = device_info.open_device(&api) {
                    let _ = device.set_blocking_mode(false);

                    let name = if is_edge { "DualSense Edge" } else { "DualSense" };
                    log::log(&format!("DualSense: Connected to {}", name));

                    return Some(Self {
                        device,
                        battery_percent: None,
                        charging_state: ChargingState::Unknown,
                        connected: true,
                        is_edge,
                    });
                }
            }
        }

        log::log("DualSense: No devices found");
        None
    }

    fn query_battery(&mut self) -> Result<(), DeviceError> {
        // DualSense sends input reports continuously, just read them
        let mut buf = [0u8; 78];

        let _ = self.device.set_blocking_mode(true);
        let bytes_read = self.device
            .read_timeout(&mut buf, 1000)
            .map_err(|e| DeviceError::CommunicationError(e.to_string()))?;
        let _ = self.device.set_blocking_mode(false);

        if bytes_read == 0 {
            self.connected = false;
            return Err(DeviceError::CommunicationError("No data from controller".to_string()));
        }

        // Battery status location depends on connection type
        // USB: Report 0x01, battery at offset 53
        // Bluetooth: Report 0x31, battery at offset 54
        let (battery_byte, status_offset) = if buf[0] == 0x01 {
            // USB mode
            (buf.get(53), 53)
        } else if buf[0] == 0x31 {
            // Bluetooth mode
            (buf.get(54), 54)
        } else {
            // Try to find battery in common locations
            (buf.get(53), 53)
        };

        if let Some(&status) = battery_byte {
            // Lower 4 bits: battery level (0-10, multiply by 10 for percentage)
            // Upper 4 bits: charging status
            let level = (status & 0x0F).min(10);
            let charging_bits = (status >> 4) & 0x0F;

            self.battery_percent = Some(level * 10);
            self.charging_state = match charging_bits {
                0x00 => ChargingState::Discharging,
                0x01 => ChargingState::Charging,
                0x02 => ChargingState::Full,
                _ => ChargingState::Unknown,
            };
            self.connected = true;

            log::log(&format!(
                "DualSense: Battery {}%, charging status: {:02X} (offset {})",
                level * 10, charging_bits, status_offset
            ));

            Ok(())
        } else {
            Err(DeviceError::ProtocolError("Could not read battery status".to_string()))
        }
    }
}

impl Device for DualSense {
    fn id(&self) -> &str {
        if self.is_edge {
            "sony-dualsense-edge"
        } else {
            "sony-dualsense"
        }
    }

    fn name(&self) -> &str {
        if self.is_edge {
            "DualSense Edge"
        } else {
            "DualSense"
        }
    }

    fn icon(&self) -> DeviceIcon {
        DeviceIcon::Controller
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

/// PlayStation DualShock 4 (PS4) controller.
pub struct DualShock4 {
    device: HidDevice,
    battery_percent: Option<u8>,
    charging_state: ChargingState,
    connected: bool,
}

impl DualShock4 {
    pub fn discover() -> Option<Self> {
        log::log("DualShock4: Starting device discovery");
        let api = HidApi::new().ok()?;

        for pid in [DUALSHOCK4_V1_PID, DUALSHOCK4_V2_PID] {
            let devices: Vec<_> = api
                .device_list()
                .filter(|d| d.vendor_id() == SONY_VID && d.product_id() == pid)
                .collect();

            if !devices.is_empty() {
                log::log(&format!("DualShock4: Found {} devices with PID {:04X}", devices.len(), pid));
            }

            for device_info in devices {
                if let Ok(device) = device_info.open_device(&api) {
                    let _ = device.set_blocking_mode(false);
                    log::log("DualShock4: Connected to controller");

                    return Some(Self {
                        device,
                        battery_percent: None,
                        charging_state: ChargingState::Unknown,
                        connected: true,
                    });
                }
            }
        }

        log::log("DualShock4: No devices found");
        None
    }

    fn query_battery(&mut self) -> Result<(), DeviceError> {
        let mut buf = [0u8; 64];

        let _ = self.device.set_blocking_mode(true);
        let bytes_read = self.device
            .read_timeout(&mut buf, 1000)
            .map_err(|e| DeviceError::CommunicationError(e.to_string()))?;
        let _ = self.device.set_blocking_mode(false);

        if bytes_read == 0 {
            self.connected = false;
            return Err(DeviceError::CommunicationError("No data from controller".to_string()));
        }

        // Battery is at byte 30 for USB, byte 32 for Bluetooth
        // Lower 4 bits = level (0-10), bit 4 = cable connected
        let status_byte = if buf[0] == 0x11 {
            // Bluetooth
            buf.get(32)
        } else {
            // USB
            buf.get(30)
        };

        if let Some(&status) = status_byte {
            let level = (status & 0x0F).min(10);
            let cable = (status >> 4) & 0x01;

            self.battery_percent = Some(level * 10);
            self.charging_state = if cable == 1 {
                if level >= 10 {
                    ChargingState::Full
                } else {
                    ChargingState::Charging
                }
            } else {
                ChargingState::Discharging
            };
            self.connected = true;

            log::log(&format!(
                "DualShock4: Battery {}%, cable: {}",
                level * 10, cable == 1
            ));

            Ok(())
        } else {
            Err(DeviceError::ProtocolError("Could not read battery status".to_string()))
        }
    }
}

impl Device for DualShock4 {
    fn id(&self) -> &str {
        "sony-dualshock4"
    }

    fn name(&self) -> &str {
        "DualShock 4"
    }

    fn icon(&self) -> DeviceIcon {
        DeviceIcon::Controller
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
