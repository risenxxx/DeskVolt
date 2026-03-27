//! System tray icon for DeskVolt.
//!
//! Displays battery status in the tooltip and provides menu options.

#![cfg(windows)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use muda::{Menu, MenuEvent, MenuItem, MenuId, PredefinedMenuItem, Submenu, CheckMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::config;
use crate::device::DeviceStatus;

// Menu item IDs
const MENU_HIDE_ID: &str = "hide";
const MENU_EXIT_ID: &str = "exit";
const MENU_POLL_2S: &str = "poll_2";
const MENU_POLL_5S: &str = "poll_5";
const MENU_POLL_10S: &str = "poll_10";
const MENU_POLL_30S: &str = "poll_30";

pub struct TrayManager {
    tray_icon: Option<TrayIcon>,
    hide_item: MenuItem,
    exit_item: MenuItem,
    poll_2s: CheckMenuItem,
    poll_5s: CheckMenuItem,
    poll_10s: CheckMenuItem,
    poll_30s: CheckMenuItem,
    current_tooltip: String,
}

impl TrayManager {
    /// Create a new tray manager. If tray is configured as hidden, starts hidden.
    pub fn new() -> Result<Self, String> {
        let hide_item = MenuItem::with_id(MenuId::new(MENU_HIDE_ID), "Hide tray icon", true, None);
        let exit_item = MenuItem::with_id(MenuId::new(MENU_EXIT_ID), "Exit", true, None);

        // Get current poll interval to set initial check state
        let current_interval = config::Config::load().poll_interval_secs;

        let poll_2s = CheckMenuItem::with_id(MenuId::new(MENU_POLL_2S), "2 seconds", true, current_interval == 2, None);
        let poll_5s = CheckMenuItem::with_id(MenuId::new(MENU_POLL_5S), "5 seconds", true, current_interval == 5, None);
        let poll_10s = CheckMenuItem::with_id(MenuId::new(MENU_POLL_10S), "10 seconds", true, current_interval == 10, None);
        let poll_30s = CheckMenuItem::with_id(MenuId::new(MENU_POLL_30S), "30 seconds", true, current_interval == 30 || (current_interval != 2 && current_interval != 5 && current_interval != 10), None);

        let mut manager = Self {
            tray_icon: None,
            hide_item,
            exit_item,
            poll_2s,
            poll_5s,
            poll_10s,
            poll_30s,
            current_tooltip: "DeskVolt - Loading...".to_string(),
        };

        // Show tray if not configured as hidden
        if !config::load_tray_hidden() {
            manager.show()?;
        }

        Ok(manager)
    }

    /// Show the tray icon.
    pub fn show(&mut self) -> Result<(), String> {
        if self.tray_icon.is_some() {
            return Ok(()); // Already visible
        }

        let icon = create_battery_icon(None)?;

        // Build poll interval submenu
        let poll_submenu = Submenu::new("Update interval", true);
        let _ = poll_submenu.append(&self.poll_2s);
        let _ = poll_submenu.append(&self.poll_5s);
        let _ = poll_submenu.append(&self.poll_10s);
        let _ = poll_submenu.append(&self.poll_30s);

        // Build menu
        let menu = Menu::new();
        let _ = menu.append(&poll_submenu);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&self.hide_item);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&self.exit_item);

        let tray_icon = TrayIconBuilder::new()
            .with_tooltip(&self.current_tooltip)
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .build()
            .map_err(|e| format!("Failed to create tray icon: {}", e))?;

        self.tray_icon = Some(tray_icon);
        config::save_tray_hidden(false);

        Ok(())
    }

    /// Hide the tray icon.
    pub fn hide(&mut self) {
        if let Some(tray) = self.tray_icon.take() {
            drop(tray);
            config::save_tray_hidden(true);
            show_hidden_notification();
        }
    }

    /// Hide without notification (used on startup).
    pub fn hide_silently(&mut self) {
        if let Some(tray) = self.tray_icon.take() {
            drop(tray);
        }
    }

    /// Update the tray tooltip with current device statuses.
    pub fn update_status(&mut self, devices: &[DeviceStatus]) {
        let tooltip = build_tooltip(devices);
        self.current_tooltip = tooltip.clone();

        if let Some(ref tray) = self.tray_icon {
            let _ = tray.set_tooltip(Some(&tooltip));

            // Update icon based on lowest battery level
            let min_battery = devices
                .iter()
                .filter(|d| d.is_connected)
                .filter_map(|d| d.battery_percent)
                .min();

            if let Ok(icon) = create_battery_icon(min_battery) {
                let _ = tray.set_icon(Some(icon));
            }
        }
    }

    /// Check if tray is currently visible.
    pub fn is_visible(&self) -> bool {
        self.tray_icon.is_some()
    }

    /// Update the poll interval checkmarks.
    pub fn set_poll_interval_checked(&self, interval_secs: u64) {
        self.poll_2s.set_checked(interval_secs == 2);
        self.poll_5s.set_checked(interval_secs == 5);
        self.poll_10s.set_checked(interval_secs == 10);
        self.poll_30s.set_checked(interval_secs == 30);
    }
}

/// Handle menu events. Returns true if exit was requested.
pub fn handle_menu_event(
    event: MenuEvent,
    tray: &mut TrayManager,
    poll_interval_secs: &Arc<AtomicU64>,
) -> bool {
    let id = event.id();

    if *id == MenuId::new(MENU_EXIT_ID) {
        return true;
    }

    if *id == MenuId::new(MENU_HIDE_ID) {
        tray.hide();
        return false;
    }

    // Handle poll interval changes
    let new_interval = if *id == MenuId::new(MENU_POLL_2S) {
        Some(2u64)
    } else if *id == MenuId::new(MENU_POLL_5S) {
        Some(5u64)
    } else if *id == MenuId::new(MENU_POLL_10S) {
        Some(10u64)
    } else if *id == MenuId::new(MENU_POLL_30S) {
        Some(30u64)
    } else {
        None
    };

    if let Some(interval) = new_interval {
        poll_interval_secs.store(interval, Ordering::Relaxed);
        tray.set_poll_interval_checked(interval);

        // Save config immediately so it persists even if process is killed
        let mut cfg = config::Config::load();
        cfg.poll_interval_secs = interval;
        cfg.save();

        crate::log::log(&format!("Poll interval changed to {}s", interval));
    }

    false
}

/// Build tooltip text from device statuses.
fn build_tooltip(devices: &[DeviceStatus]) -> String {
    if devices.is_empty() {
        return "DeskVolt - No devices".to_string();
    }

    let mut lines = vec!["DeskVolt".to_string()];

    for device in devices {
        let status = if !device.is_connected {
            "Disconnected".to_string()
        } else if let Some(percent) = device.battery_percent {
            let charging = if device.charging_state == crate::device::ChargingState::Charging {
                " (charging)"
            } else {
                ""
            };
            format!("{}%{}", percent, charging)
        } else {
            "Unknown".to_string()
        };

        lines.push(format!("{}: {}", device.name, status));
    }

    // Windows tooltip is limited to ~128 chars, truncate if needed
    let result = lines.join("\n");
    if result.len() > 127 {
        result[..127].to_string()
    } else {
        result
    }
}

/// Create a battery icon with the given level.
/// Icon changes color based on battery level: green > 50%, yellow 20-50%, red < 20%.
fn create_battery_icon(battery_percent: Option<u8>) -> Result<Icon, String> {
    const SIZE: u32 = 32;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];

    // Determine color based on battery level
    let (r, g, b) = match battery_percent {
        Some(p) if p < 20 => (220, 80, 80),    // Red for low
        Some(p) if p < 50 => (220, 180, 60),   // Yellow for medium
        Some(_) => (80, 200, 80),               // Green for good
        None => (150, 150, 150),                // Gray for unknown
    };

    // Draw a simple battery shape
    for y in 0..SIZE {
        for x in 0..SIZE {
            let idx = ((y * SIZE + x) * 4) as usize;

            // Battery body: rounded rectangle from (4,8) to (26,24)
            // Battery tip: small rectangle from (26,11) to (28,21)
            let in_body = x >= 4 && x <= 26 && y >= 8 && y <= 24;
            let in_tip = x >= 26 && x <= 28 && y >= 11 && y <= 21;

            // Fill level (horizontal fill based on percentage)
            let fill_percent = battery_percent.unwrap_or(50) as u32;
            let fill_width = (22 * fill_percent) / 100; // 22 = body width (26-4)
            let is_filled = x >= 5 && x < 5 + fill_width && y >= 9 && y <= 23;

            if in_body || in_tip {
                if is_filled {
                    // Filled portion
                    rgba[idx] = r;
                    rgba[idx + 1] = g;
                    rgba[idx + 2] = b;
                    rgba[idx + 3] = 255;
                } else {
                    // Border/outline
                    let is_border = x == 4 || x == 26 || y == 8 || y == 24
                        || (in_tip && (x == 28 || y == 11 || y == 21));
                    if is_border {
                        rgba[idx] = 200;
                        rgba[idx + 1] = 200;
                        rgba[idx + 2] = 200;
                        rgba[idx + 3] = 255;
                    } else {
                        // Empty portion (dark)
                        rgba[idx] = 40;
                        rgba[idx + 1] = 40;
                        rgba[idx + 2] = 40;
                        rgba[idx + 3] = 255;
                    }
                }
            }
            // Outside battery shape: transparent
        }
    }

    Icon::from_rgba(rgba, SIZE, SIZE).map_err(|e| format!("Failed to create icon: {}", e))
}

/// Show a Windows notification that the tray is hidden.
fn show_hidden_notification() {
    // Use a simple message box since we don't have a notification library
    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK};

    unsafe {
        MessageBoxW(
            None,
            w!("Tray icon hidden.\n\nRelaunch DeskVolt to restore it."),
            w!("DeskVolt"),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}
