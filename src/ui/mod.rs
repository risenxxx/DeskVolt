//! Widget window creation and management.

#![cfg(windows)]

mod render;

use std::cell::RefCell;

use crate::config::Position;
use crate::device::DeviceStatus;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateRoundRectRgn,
    DeleteDC, DeleteObject, EndPaint, InvalidateRect, SelectObject, SetWindowRgn, UpdateWindow,
    PAINTSTRUCT, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;

use render::Renderer;

// Corner radius for rounded corners
const CORNER_RADIUS: i32 = 12;

// Window class name
const CLASS_NAME: PCWSTR = w!("DeskVoltWidget");

// Window dimensions (scaled for readability on 4K displays)
const ROW_HEIGHT: i32 = 44;
const WINDOW_WIDTH: i32 = 380;
const PADDING_TOP: i32 = 5;
const PADDING_BOTTOM: i32 = 16;
const MIN_HEIGHT: i32 = PADDING_TOP + ROW_HEIGHT + PADDING_BOTTOM;

// Thread-local storage for widget state (needed for window proc)
thread_local! {
    static WIDGET_STATE: RefCell<Option<WidgetState>> = const { RefCell::new(None) };
}

struct WidgetState {
    devices: Vec<DeviceStatus>,
    renderer: Renderer,
    dragging: bool,
    drag_start_x: i32,
    drag_start_y: i32,
}

pub struct Widget {
    hwnd: HWND,
}

impl Widget {
    pub fn new(position: Position) -> Result<Self, String> {
        unsafe {
            let instance = GetModuleHandleW(None)
                .map_err(|e| format!("GetModuleHandle failed: {}", e))?;

            // Register window class (no CS_HREDRAW/CS_VREDRAW to avoid flicker)
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: WNDCLASS_STYLES(0), // No redraw styles - we handle all painting
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: CLASS_NAME,
                ..Default::default()
            };

            RegisterClassExW(&wc);

            // Calculate window size
            let height = MIN_HEIGHT;

            // Get screen dimensions
            let screen_width = GetSystemMetrics(SM_CXSCREEN);
            let screen_height = GetSystemMetrics(SM_CYSCREEN);

            // Calculate position (default: top-right corner)
            // Also validate saved position is within current screen bounds
            let (x, y) = if position.x < 0 || position.y < 0
                || position.x + WINDOW_WIDTH > screen_width
                || position.y + height > screen_height
            {
                // Default to top-right with margin (or reset if off-screen)
                (screen_width - WINDOW_WIDTH - 20, 20)
            } else {
                (position.x, position.y)
            };

            // Extended window styles for desktop widget behavior
            // No WS_EX_TOPMOST - we want it to stay at desktop level
            let ex_style = WS_EX_TOOLWINDOW      // No taskbar button
                | WS_EX_LAYERED           // For transparency
                | WS_EX_NOACTIVATE;       // Don't steal focus

            let hwnd = CreateWindowExW(
                ex_style,
                CLASS_NAME,
                w!("DeskVolt"),
                WS_POPUP, // Borderless
                x,
                y,
                WINDOW_WIDTH,
                height,
                None,
                None,
                instance,
                None,
            )
            .map_err(|e| format!("CreateWindowEx failed: {}", e))?;

            // Set layered window attributes for semi-transparency
            // 230/255 ≈ 90% opacity for a nice frosted glass effect
            SetLayeredWindowAttributes(hwnd, None, 230, LWA_ALPHA)
                .map_err(|e| format!("SetLayeredWindowAttributes failed: {}", e))?;

            // Enable rounded corners (Windows 11+ native, fallback for older Windows)
            apply_rounded_corners(hwnd, WINDOW_WIDTH, height);

            // Initialize renderer
            let renderer = Renderer::new();

            // Store state in thread-local
            WIDGET_STATE.with(|state| {
                *state.borrow_mut() = Some(WidgetState {
                    devices: Vec::new(),
                    renderer,
                    dragging: false,
                    drag_start_x: 0,
                    drag_start_y: 0,
                });
            });

            // Show window at bottom of Z-order (like desktop icons)
            let _ = SetWindowPos(
                hwnd,
                HWND_BOTTOM,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = UpdateWindow(hwnd);

            Ok(Self { hwnd })
        }
    }

    pub fn update_devices(&mut self, devices: Vec<DeviceStatus>) {
        // Update state
        WIDGET_STATE.with(|state| {
            if let Some(ref mut s) = *state.borrow_mut() {
                s.devices = devices.clone();
            }
        });

        // Resize window based on device count
        let device_count = devices.len().max(1) as i32;
        let new_height = PADDING_TOP + device_count * ROW_HEIGHT + PADDING_BOTTOM;

        unsafe {
            let mut rect = RECT::default();
            let _ = GetWindowRect(self.hwnd, &mut rect);
            let _ = SetWindowPos(
                self.hwnd,
                HWND_BOTTOM,
                rect.left,
                rect.top,
                WINDOW_WIDTH,
                new_height,
                SWP_NOMOVE | SWP_NOACTIVATE,
            );

            // Re-apply rounded corners after resize
            apply_rounded_corners(self.hwnd, WINDOW_WIDTH, new_height);

            // Trigger repaint (false = don't erase background, reduces flicker)
            let _ = InvalidateRect(self.hwnd, None, false);
        }
    }

    pub fn position(&self) -> Position {
        unsafe {
            let mut rect = RECT::default();
            let _ = GetWindowRect(self.hwnd, &mut rect);
            Position {
                x: rect.left,
                y: rect.top,
            }
        }
    }
}

impl Drop for Widget {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => {
            // Return 1 to tell Windows we handled background (prevents flicker)
            LRESULT(1)
        }

        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            // Get window dimensions
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);

            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;

            // Double buffering: create off-screen DC and bitmap
            let mem_dc = CreateCompatibleDC(hdc);
            let mem_bitmap = CreateCompatibleBitmap(hdc, width, height);
            let old_bitmap = SelectObject(mem_dc, mem_bitmap);

            // Render to off-screen buffer
            WIDGET_STATE.with(|state| {
                if let Some(ref s) = *state.borrow() {
                    s.renderer.render(mem_dc, &rect, &s.devices);
                }
            });

            // Copy to screen in one operation (no flicker)
            let _ = BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);

            // Cleanup
            SelectObject(mem_dc, old_bitmap);
            let _ = DeleteObject(mem_bitmap);
            let _ = DeleteDC(mem_dc);

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }

        WM_NCHITTEST => {
            // Make the entire window draggable
            LRESULT(HTCAPTION as isize)
        }

        WM_LBUTTONDOWN => {
            // Start dragging
            WIDGET_STATE.with(|state| {
                if let Some(ref mut s) = *state.borrow_mut() {
                    s.dragging = true;
                    s.drag_start_x = (lparam.0 & 0xFFFF) as i16 as i32;
                    s.drag_start_y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                }
            });
            let _ = SetCapture(hwnd);
            LRESULT(0)
        }

        WM_LBUTTONUP => {
            WIDGET_STATE.with(|state| {
                if let Some(ref mut s) = *state.borrow_mut() {
                    s.dragging = false;
                }
            });
            let _ = ReleaseCapture();
            LRESULT(0)
        }

        WM_MOUSEMOVE => {
            let mut should_move = false;
            WIDGET_STATE.with(|state| {
                if let Some(ref s) = *state.borrow() {
                    should_move = s.dragging;
                }
            });

            if should_move {
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

                let mut rect = RECT::default();
                let _ = GetWindowRect(hwnd, &mut rect);

                let mut dx = 0;
                let mut dy = 0;
                WIDGET_STATE.with(|state| {
                    if let Some(ref s) = *state.borrow() {
                        dx = x - s.drag_start_x;
                        dy = y - s.drag_start_y;
                    }
                });

                let _ = SetWindowPos(
                    hwnd,
                    HWND_BOTTOM,
                    rect.left + dx,
                    rect.top + dy,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
            LRESULT(0)
        }

        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Apply rounded corners to a window.
/// Uses DWM on Windows 11+ for native smooth corners, falls back to region-based
/// corners on older Windows versions.
fn apply_rounded_corners(hwnd: HWND, width: i32, height: i32) {
    unsafe {
        // Try Windows 11+ native rounded corners first (DWM)
        let preference = DWMWCP_ROUND;
        let result = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &preference as *const _ as *const _,
            std::mem::size_of_val(&preference) as u32,
        );

        if result.is_ok() {
            // DWM rounded corners applied successfully
            return;
        }

        // Fallback for Windows 10 and older: use a rounded rectangle region
        let region = CreateRoundRectRgn(
            0,
            0,
            width + 1,
            height + 1,
            CORNER_RADIUS,
            CORNER_RADIUS,
        );

        if !region.is_invalid() {
            // SetWindowRgn takes ownership of the region, don't delete it
            let _ = SetWindowRgn(hwnd, region, true);
        }
    }
}
