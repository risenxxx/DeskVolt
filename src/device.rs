//! Device trait and common types for battery-reporting peripherals.

use std::fmt;

/// Icon type for device identification in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DeviceIcon {
    Mouse,
    Headset,
    Keyboard,
    Controller,
    Generic,
}

/// Charging state of a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargingState {
    Discharging,
    Charging,
    Full,
    Unknown,
}

/// Snapshot of a device's current status for UI rendering.
#[derive(Debug, Clone)]
pub struct DeviceStatus {
    #[allow(dead_code)]
    pub id: String,
    pub name: String,
    pub icon: DeviceIcon,
    pub battery_percent: Option<u8>,
    pub charging_state: ChargingState,
    pub is_connected: bool,
}

impl DeviceStatus {
    pub fn is_low_battery(&self) -> bool {
        self.battery_percent.map(|p| p < 20).unwrap_or(false)
    }
}

/// Error type for device operations.
#[derive(Debug)]
#[allow(dead_code)]
pub enum DeviceError {
    NotFound,
    ConnectionFailed(String),
    CommunicationError(String),
    ProtocolError(String),
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceError::NotFound => write!(f, "Device not found"),
            DeviceError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            DeviceError::CommunicationError(msg) => write!(f, "Communication error: {}", msg),
            DeviceError::ProtocolError(msg) => write!(f, "Protocol error: {}", msg),
        }
    }
}

impl std::error::Error for DeviceError {}

/// Trait for battery-reporting devices.
///
/// Implementations should handle HID communication with specific device protocols.
pub trait Device: Send {
    /// Unique identifier for this device instance.
    fn id(&self) -> &str;

    /// Human-readable display name.
    fn name(&self) -> &str;

    /// Icon type for UI rendering.
    fn icon(&self) -> DeviceIcon;

    /// Current battery percentage (0-100), or None if unavailable.
    fn battery_percent(&self) -> Option<u8>;

    /// Current charging state.
    fn charging_state(&self) -> ChargingState;

    /// Whether the device is currently connected.
    fn is_connected(&self) -> bool;

    /// Poll the device to refresh battery status.
    ///
    /// This may involve HID communication and should be called from a background thread.
    fn poll(&mut self) -> Result<(), DeviceError>;

    /// Get a snapshot of the current device status.
    fn status(&self) -> DeviceStatus {
        DeviceStatus {
            id: self.id().to_string(),
            name: self.name().to_string(),
            icon: self.icon(),
            battery_percent: self.battery_percent(),
            charging_state: self.charging_state(),
            is_connected: self.is_connected(),
        }
    }
}

/// Registry of all known devices.
pub struct DeviceRegistry {
    devices: Vec<Box<dyn Device>>,
}

impl DeviceRegistry {
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
        }
    }

    /// Discover and register all supported devices.
    pub fn discover(&mut self) {
        // Clear existing devices
        self.devices.clear();

        // Logitech mice
        if let Some(device) = crate::devices::logitech::LogitechSuperlight::discover() {
            self.devices.push(Box::new(device));
        }

        // Logitech headsets
        if let Some(device) = crate::devices::logitech_headsets::LogitechGProX2::discover() {
            self.devices.push(Box::new(device));
        }
        if let Some(device) = crate::devices::logitech_headsets::LogitechGProX::discover() {
            self.devices.push(Box::new(device));
        }
        if let Some(device) = crate::devices::logitech_headsets::LogitechG933::discover() {
            self.devices.push(Box::new(device));
        }

        // SteelSeries headsets
        if let Some(device) = crate::devices::steelseries::SteelSeriesArctis::discover() {
            self.devices.push(Box::new(device));
        }
        if let Some(device) = crate::devices::steelseries::SteelSeriesArctis7::discover() {
            self.devices.push(Box::new(device));
        }

        // Corsair headsets
        if let Some(device) = crate::devices::corsair::CorsairVoid::discover() {
            self.devices.push(Box::new(device));
        }

        // HyperX headsets
        if let Some(device) = crate::devices::hyperx::HyperXCloudAlpha::discover() {
            self.devices.push(Box::new(device));
        }

        // PlayStation controllers
        if let Some(device) = crate::devices::playstation::DualSense::discover() {
            self.devices.push(Box::new(device));
        }
        if let Some(device) = crate::devices::playstation::DualShock4::discover() {
            self.devices.push(Box::new(device));
        }
    }

    /// Poll all devices and return their current statuses.
    pub fn poll_all(&mut self) -> Vec<DeviceStatus> {
        let mut statuses = Vec::new();

        for device in &mut self.devices {
            // Attempt to poll, ignore errors (device may be disconnected)
            let _ = device.poll();
            statuses.push(device.status());
        }

        statuses
    }

    /// Get the number of registered devices.
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }
}

impl Default for DeviceRegistry {
    fn default() -> Self {
        Self::new()
    }
}
