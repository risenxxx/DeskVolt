//! Logitech headset battery support (G Pro X, G633, G933, G935).
//!
//! Protocol references:
//! - HeadsetControl: https://github.com/Sapd/HeadsetControl

#![cfg(windows)]

use crate::device::{ChargingState, Device, DeviceError, DeviceIcon};
use crate::log;
use hidapi::{HidApi, HidDevice};

// Logitech Vendor ID
const LOGITECH_VID: u16 = 0x046D;

// G Pro X 2 LIGHTSPEED (64-byte vendor protocol)
const GPRO_X2_LIGHTSPEED_PID: u16 = 0x0AF7;

// G Pro X Wireless (HID++ protocol)
const GPRO_X_WIRELESS_PID: u16 = 0x0ABA;

// G633/G933/G935 (HID++ protocol)
const G633_PIDS: &[u16] = &[
    0x0A5C, // G633
    0x0A5B, // G933
    0x0A87, // G935
    0x0A89, // G635
    0x0AB5, // G733
    0x0AFE, // G733 (alt)
];

// ============================================================================
// G Pro X 2 LIGHTSPEED (64-byte vendor-specific protocol)
// ============================================================================

const GPX2_USAGE_PAGE: u16 = 0xFFA0;
const GPX2_INTERFACE: i32 = 3;
const GPX2_MSG_SIZE: usize = 64;

pub struct LogitechGProX2 {
    device: HidDevice,
    battery_percent: Option<u8>,
    charging_state: ChargingState,
    connected: bool,
}

impl LogitechGProX2 {
    pub fn discover() -> Option<Self> {
        log::log("Logitech G Pro X 2: Starting device discovery");
        let api = HidApi::new().ok()?;

        // Find device with matching usage page
        let devices: Vec<_> = api
            .device_list()
            .filter(|d| {
                d.vendor_id() == LOGITECH_VID
                    && d.product_id() == GPRO_X2_LIGHTSPEED_PID
                    && (d.usage_page() == GPX2_USAGE_PAGE || d.interface_number() == GPX2_INTERFACE)
            })
            .collect();

        log::log(&format!(
            "Logitech G Pro X 2: Found {} matching devices",
            devices.len()
        ));

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
                    log::log("Logitech G Pro X 2: Connected successfully");
                    return Some(headset);
                }
            }
        }

        log::log("Logitech G Pro X 2: No devices found");
        None
    }

    fn query_battery(&mut self) -> Result<(), DeviceError> {
        // Build battery request
        let mut request = [0u8; GPX2_MSG_SIZE];
        request[0] = 0x51;
        request[1] = 0x08;
        request[3] = 0x03;
        request[4] = 0x1A;
        request[6] = 0x03;
        request[8] = 0x04;
        request[9] = 0x0A;

        self.device
            .write(&request)
            .map_err(|e| DeviceError::CommunicationError(e.to_string()))?;

        let _ = self.device.set_blocking_mode(true);

        // Read responses, filtering for battery response
        let mut attempts = 0;
        while attempts < 10 {
            let mut response = [0u8; GPX2_MSG_SIZE];
            let bytes_read = self.device
                .read_timeout(&mut response, 500)
                .map_err(|e| DeviceError::CommunicationError(e.to_string()))?;

            if bytes_read == 0 {
                attempts += 1;
                continue;
            }

            // Check for power off
            if response[0] == 0x51 && response[1] == 0x05 && response[6] == 0x00 {
                self.connected = false;
                let _ = self.device.set_blocking_mode(false);
                log::log("Logitech G Pro X 2: Headset is off");
                return Ok(());
            }

            // Check for battery response
            if response[0] == 0x51 && response[1] == 0x0B && response[8] == 0x04 {
                let battery = response[10];
                let charging = response[12] == 0x02;

                if battery <= 100 {
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
                        "Logitech G Pro X 2: Battery {}%, charging: {}",
                        battery, charging
                    ));

                    let _ = self.device.set_blocking_mode(false);
                    return Ok(());
                }
            }

            attempts += 1;
        }

        let _ = self.device.set_blocking_mode(false);
        Err(DeviceError::CommunicationError("No battery response".to_string()))
    }
}

impl Device for LogitechGProX2 {
    fn id(&self) -> &str {
        "logitech-gpro-x2"
    }

    fn name(&self) -> &str {
        "G Pro X 2"
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

// ============================================================================
// G Pro X Wireless & G633/G933/G935 (HID++ voltage-based protocol)
// ============================================================================

const HIDPP_USAGE_PAGE: u16 = 0xFF43;
const HIDPP_MSG_SIZE: usize = 20;

// Voltage to percentage calibration for G Pro X
const GPRO_X_CALIBRATION: &[(u16, u8)] = &[
    (4150, 100),
    (3830, 50),
    (3780, 30),
    (3740, 20),
    (3670, 5),
    (3320, 0),
];

// Voltage to percentage calibration for G933/G633
const G933_CALIBRATION: &[(u16, u8)] = &[
    (4100, 100),
    (3900, 75),
    (3800, 50),
    (3700, 25),
    (3600, 5),
    (3500, 0),
];

fn voltage_to_percent(voltage: u16, calibration: &[(u16, u8)]) -> u8 {
    // Find the two calibration points we're between
    for i in 0..calibration.len() - 1 {
        let (v_high, p_high) = calibration[i];
        let (v_low, p_low) = calibration[i + 1];

        if voltage >= v_low && voltage <= v_high {
            // Linear interpolation
            let v_range = v_high - v_low;
            let p_range = p_high - p_low;
            if v_range == 0 {
                return p_high;
            }
            let ratio = (voltage - v_low) as f32 / v_range as f32;
            return p_low + (ratio * p_range as f32) as u8;
        }
    }

    // Outside calibration range
    if voltage >= calibration[0].0 {
        100
    } else {
        0
    }
}

pub struct LogitechGProX {
    device: HidDevice,
    battery_percent: Option<u8>,
    charging_state: ChargingState,
    connected: bool,
}

impl LogitechGProX {
    pub fn discover() -> Option<Self> {
        log::log("Logitech G Pro X: Starting device discovery");
        let api = HidApi::new().ok()?;

        let devices: Vec<_> = api
            .device_list()
            .filter(|d| {
                d.vendor_id() == LOGITECH_VID
                    && d.product_id() == GPRO_X_WIRELESS_PID
                    && d.usage_page() == HIDPP_USAGE_PAGE
            })
            .collect();

        log::log(&format!(
            "Logitech G Pro X: Found {} matching devices",
            devices.len()
        ));

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
                    log::log("Logitech G Pro X: Connected successfully");
                    return Some(headset);
                }
            }
        }

        log::log("Logitech G Pro X: No devices found");
        None
    }

    fn query_battery(&mut self) -> Result<(), DeviceError> {
        // HID++ battery query: feature 0x06, function 0x0D
        let mut request = [0u8; HIDPP_MSG_SIZE];
        request[0] = 0x11; // Long message
        request[1] = 0xFF; // Device receiver
        request[2] = 0x06; // Feature
        request[3] = 0x0D; // Function

        self.device
            .write(&request)
            .map_err(|e| DeviceError::CommunicationError(e.to_string()))?;

        let _ = self.device.set_blocking_mode(true);

        let mut response = [0u8; HIDPP_MSG_SIZE];
        let bytes_read = self.device
            .read_timeout(&mut response, 1000)
            .map_err(|e| DeviceError::CommunicationError(e.to_string()))?;

        let _ = self.device.set_blocking_mode(false);

        if bytes_read == 0 {
            self.connected = false;
            return Err(DeviceError::CommunicationError("No response".to_string()));
        }

        log::log(&format!(
            "Logitech G Pro X: Response: {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}",
            response[0], response[1], response[2], response[3],
            response[4], response[5], response[6]
        ));

        // Check for offline
        if response[2] == 0xFF {
            self.connected = false;
            log::log("Logitech G Pro X: Device offline");
            return Ok(());
        }

        // Parse voltage (big-endian) at bytes 4-5
        let voltage = ((response[4] as u16) << 8) | (response[5] as u16);
        let charging = response[6] == 0x03;

        if voltage < 3000 || voltage > 4500 {
            return Err(DeviceError::ProtocolError(format!(
                "Invalid voltage: {}",
                voltage
            )));
        }

        let percent = voltage_to_percent(voltage, GPRO_X_CALIBRATION);

        self.battery_percent = Some(percent);
        self.charging_state = if charging {
            ChargingState::Charging
        } else if percent >= 100 {
            ChargingState::Full
        } else {
            ChargingState::Discharging
        };
        self.connected = true;

        log::log(&format!(
            "Logitech G Pro X: Voltage {}mV = {}%, charging: {}",
            voltage, percent, charging
        ));

        Ok(())
    }
}

impl Device for LogitechGProX {
    fn id(&self) -> &str {
        "logitech-gpro-x"
    }

    fn name(&self) -> &str {
        "G Pro X"
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

// ============================================================================
// G633/G933/G935 Series
// ============================================================================

pub struct LogitechG933 {
    device: HidDevice,
    battery_percent: Option<u8>,
    charging_state: ChargingState,
    connected: bool,
    product_name: &'static str,
}

impl LogitechG933 {
    pub fn discover() -> Option<Self> {
        log::log("Logitech G933: Starting device discovery");
        let api = HidApi::new().ok()?;

        for &pid in G633_PIDS {
            let devices: Vec<_> = api
                .device_list()
                .filter(|d| {
                    d.vendor_id() == LOGITECH_VID
                        && d.product_id() == pid
                        && d.usage_page() == HIDPP_USAGE_PAGE
                })
                .collect();

            if devices.is_empty() {
                continue;
            }

            log::log(&format!(
                "Logitech G933: Found {} devices with PID {:04X}",
                devices.len(), pid
            ));

            let name = match pid {
                0x0A5C => "G633",
                0x0A5B => "G933",
                0x0A87 => "G935",
                0x0A89 => "G635",
                0x0AB5 | 0x0AFE => "G733",
                _ => "G-Series",
            };

            for device_info in devices {
                if let Ok(device) = device_info.open_device(&api) {
                    let _ = device.set_blocking_mode(false);

                    let mut headset = Self {
                        device,
                        battery_percent: None,
                        charging_state: ChargingState::Unknown,
                        connected: true,
                        product_name: name,
                    };

                    if headset.query_battery().is_ok() {
                        log::log(&format!("Logitech G933: Connected to {}", name));
                        return Some(headset);
                    }
                }
            }
        }

        log::log("Logitech G933: No devices found");
        None
    }

    fn query_battery(&mut self) -> Result<(), DeviceError> {
        // Same HID++ protocol as G Pro X
        let mut request = [0u8; HIDPP_MSG_SIZE];
        request[0] = 0x11;
        request[1] = 0xFF;
        request[2] = 0x06;
        request[3] = 0x0D;

        self.device
            .write(&request)
            .map_err(|e| DeviceError::CommunicationError(e.to_string()))?;

        let _ = self.device.set_blocking_mode(true);

        let mut response = [0u8; HIDPP_MSG_SIZE];
        let bytes_read = self.device
            .read_timeout(&mut response, 1000)
            .map_err(|e| DeviceError::CommunicationError(e.to_string()))?;

        let _ = self.device.set_blocking_mode(false);

        if bytes_read == 0 {
            self.connected = false;
            return Err(DeviceError::CommunicationError("No response".to_string()));
        }

        if response[2] == 0xFF {
            self.connected = false;
            log::log(&format!("Logitech {}: Device offline", self.product_name));
            return Ok(());
        }

        let voltage = ((response[4] as u16) << 8) | (response[5] as u16);
        let charging = response[6] == 0x03;

        if voltage < 3000 || voltage > 4500 {
            return Err(DeviceError::ProtocolError(format!(
                "Invalid voltage: {}",
                voltage
            )));
        }

        let percent = voltage_to_percent(voltage, G933_CALIBRATION);

        self.battery_percent = Some(percent);
        self.charging_state = if charging {
            ChargingState::Charging
        } else if percent >= 100 {
            ChargingState::Full
        } else {
            ChargingState::Discharging
        };
        self.connected = true;

        log::log(&format!(
            "Logitech {}: Voltage {}mV = {}%, charging: {}",
            self.product_name, voltage, percent, charging
        ));

        Ok(())
    }
}

impl Device for LogitechG933 {
    fn id(&self) -> &str {
        "logitech-g933"
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
