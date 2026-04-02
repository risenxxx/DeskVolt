//! GDI-based rendering for the widget.

#![cfg(windows)]

use crate::device::{ChargingState, DeviceIcon, DeviceStatus};

use windows::core::w;
use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, FillRect, GetTextExtentPoint32W,
    SelectObject, SetBkMode, SetTextColor, TextOutW, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS,
    DEFAULT_CHARSET, DEFAULT_PITCH, DT_CENTER, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, FW_NORMAL,
    HDC, HFONT, OUT_TT_PRECIS, TRANSPARENT,
};

// Colors (COLORREF format: 0x00BBGGRR)
const COLOR_BG: COLORREF = COLORREF(0x002D2D2D);        // Modern dark gray background
const COLOR_TEXT: COLORREF = COLORREF(0x00FFFFFF);      // White text
const COLOR_TEXT_DIM: COLORREF = COLORREF(0x00909090);  // Lighter gray for better visibility
const COLOR_CHARGING: COLORREF = COLORREF(0x0000D4FF);  // Yellow/gold for charging
const COLOR_LOW: COLORREF = COLORREF(0x005050FF);       // Softer red for low battery

// Layout constants
const ROW_HEIGHT: i32 = 44;
const PADDING_TOP: i32 = 5;
const PADDING_LEFT: i32 = 16;
const PADDING_RIGHT: i32 = 16;
const PADDING_BOTTOM: i32 = 16;
const ICON_X: i32 = PADDING_LEFT;
const NAME_X: i32 = ICON_X + 38; // Increased gap between icon and name

pub struct Renderer {
    font: HFONT,
    font_icon: HFONT,
}

impl Renderer {
    pub fn new() -> Self {
        unsafe {
            // Create main font (Segoe UI, scaled up for readability)
            let font = CreateFontW(
                -22, // Height (negative for character height) - ~1.4x original
                0,
                0,
                0,
                FW_NORMAL.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET.0 as u32,
                OUT_TT_PRECIS.0 as u32,          // TrueType precision
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,      // ClearType for crisp text
                DEFAULT_PITCH.0 as u32 | (FF_DONTCARE.0 as u32),
                w!("Segoe UI"),
            );

            // Create icon font (Segoe MDL2 Assets - Windows 10/11 system icons)
            let font_icon = CreateFontW(
                -22, // Match text size for consistent look
                0,
                0,
                0,
                FW_NORMAL.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET.0 as u32,
                OUT_TT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                DEFAULT_PITCH.0 as u32 | (FF_DONTCARE.0 as u32),
                w!("Segoe MDL2 Assets"),
            );

            Self { font, font_icon }
        }
    }

    pub fn render(&self, hdc: HDC, rect: &RECT, devices: &[DeviceStatus]) {
        unsafe {
            // Fill background
            let bg_brush = CreateSolidBrush(COLOR_BG);
            FillRect(hdc, rect, bg_brush);
            let _ = DeleteObject(bg_brush);

            // Set text mode
            SetBkMode(hdc, TRANSPARENT);

            if devices.is_empty() {
                // No devices found
                let old_font = SelectObject(hdc, self.font);
                SetTextColor(hdc, COLOR_TEXT_DIM);

                let text = "No devices found";
                let mut text_wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                let text_len = text_wide.len() - 1;

                let mut text_rect = RECT {
                    left: PADDING_LEFT,
                    top: PADDING_TOP,
                    right: rect.right - PADDING_RIGHT,
                    bottom: rect.bottom - PADDING_BOTTOM,
                };

                DrawTextW(
                    hdc,
                    &mut text_wide[..text_len],
                    &mut text_rect,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );

                SelectObject(hdc, old_font);
                return;
            }

            // Render each device
            for (i, device) in devices.iter().enumerate() {
                let y = PADDING_TOP + (i as i32) * ROW_HEIGHT;
                self.render_device_row(hdc, rect, y, device);
            }
        }
    }

    fn render_device_row(&self, hdc: HDC, rect: &RECT, y: i32, device: &DeviceStatus) {
        unsafe {
            let text_color = if device.is_connected {
                COLOR_TEXT
            } else {
                COLOR_TEXT_DIM
            };

            let text_y = y + (ROW_HEIGHT - 22) / 2; // Vertically center text
            let icon_y = text_y + 5; // MDL2 icons need offset down to center with text

            // Draw device icon
            let old_font = SelectObject(hdc, self.font_icon);
            SetTextColor(hdc, text_color);

            // Segoe MDL2 Assets icons
            let icon_char = match device.icon {
                DeviceIcon::Mouse => "\u{E962}",      // Mouse
                DeviceIcon::Headset => "\u{E7F6}",    // Headphones
                DeviceIcon::Keyboard => "\u{E92E}",   // Keyboard
                DeviceIcon::Controller => "\u{E7FC}", // Game controller
                DeviceIcon::Generic => "\u{E83F}",    // Battery
            };

            let icon_wide: Vec<u16> = icon_char.encode_utf16().collect();
            let _ = TextOutW(hdc, ICON_X, icon_y, &icon_wide);

            // Draw device name
            SelectObject(hdc, self.font);
            SetTextColor(hdc, text_color);

            let name_wide: Vec<u16> = device.name.encode_utf16().collect();
            let _ = TextOutW(hdc, NAME_X, text_y, &name_wide);

            // Fixed layout from right edge:
            // [status_icon 24px] [gap 8px] [percent 50px] [PADDING_RIGHT]
            let right_edge = rect.right - PADDING_RIGHT;
            let percent_right = right_edge; // Percentage always at fixed right position
            let status_x = right_edge - 50 - 8 - 24; // Left of percentage with gap

            // Draw battery percentage (right-aligned to fixed position)
            SelectObject(hdc, self.font);
            let percent_str = match device.battery_percent {
                Some(p) => format!("{}%", p),
                None => "-".to_string(),
            };

            let percent_color = if !device.is_connected {
                COLOR_TEXT_DIM
            } else if device.is_low_battery() {
                COLOR_LOW
            } else {
                COLOR_TEXT
            };

            SetTextColor(hdc, percent_color);

            // Measure text width for right alignment
            let percent_wide: Vec<u16> = percent_str.encode_utf16().collect();
            let mut text_size = windows::Win32::Foundation::SIZE::default();
            let _ = GetTextExtentPoint32W(hdc, &percent_wide, &mut text_size);
            let percent_x = percent_right - text_size.cx;
            let _ = TextOutW(hdc, percent_x, text_y, &percent_wide);

            // Draw status icon (between name and percentage) - MDL2 icons
            // Disconnected devices just use dimmed text, no X icon
            let status_icon = if device.charging_state == ChargingState::Charging {
                Some(("\u{E945}", COLOR_CHARGING)) // Charging bolt
            } else if device.is_low_battery() && device.is_connected {
                Some(("\u{E814}", COLOR_LOW)) // Warning/important
            } else {
                None
            };

            if let Some((icon, color)) = status_icon {
                SelectObject(hdc, self.font_icon);
                SetTextColor(hdc, color);
                let status_wide: Vec<u16> = icon.encode_utf16().collect();
                let _ = TextOutW(hdc, status_x, icon_y, &status_wide);
            }

            SelectObject(hdc, old_font);
        }
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.font);
            let _ = DeleteObject(self.font_icon);
        }
    }
}
