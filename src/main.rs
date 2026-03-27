//! DeskVolt - Lightweight battery widget for wireless peripherals
//!
//! Displays battery status for wireless devices via direct HID communication,
//! bypassing vendor bloatware.
//!
//! This is a Windows-only application.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod config;
#[cfg(windows)]
mod device;
#[cfg(windows)]
mod devices;
#[cfg(windows)]
mod log;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod ui;
#[cfg(windows)]
mod worker;

#[cfg(not(windows))]
fn main() {
    eprintln!("DeskVolt is a Windows-only application.");
    eprintln!("Please compile and run on Windows.");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    use std::sync::atomic::AtomicU64;
    use std::sync::mpsc;
    use std::sync::Arc;

    use device::DeviceStatus;
    use tray::TrayManager;
    use ui::Widget;

    // Kill any existing instance before starting
    kill_existing_instance();

    // Enable DPI awareness for crisp rendering on high-DPI displays
    enable_dpi_awareness();

    // Initialize logging
    log::init();
    log::log_hid_devices();

    // Initialize COM
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        );
    }

    // Load config
    let config = config::Config::load();
    log::log(&format!(
        "Config loaded: position=({}, {}), poll_interval={}s, tray_hidden={}",
        config.position.x, config.position.y, config.poll_interval_secs, config.tray_hidden
    ));

    // Create channel for device status updates
    let (tx, rx) = mpsc::channel::<Vec<DeviceStatus>>();

    // Shared poll interval (can be changed via tray menu)
    let poll_interval_secs = Arc::new(AtomicU64::new(config.poll_interval_secs));
    let poll_interval_for_worker = Arc::clone(&poll_interval_secs);

    // Start background worker thread
    worker::start_worker(tx, poll_interval_for_worker);

    // Create the widget window
    let mut widget = match Widget::new(config.position) {
        Ok(w) => {
            log::log("Widget created successfully");
            w
        }
        Err(e) => {
            log::log(&format!("Failed to create widget: {}", e));
            return;
        }
    };

    // Create tray icon
    let mut tray_manager = match TrayManager::new() {
        Ok(t) => {
            log::log("Tray manager created successfully");
            t
        }
        Err(e) => {
            log::log(&format!("Failed to create tray manager: {}", e));
            return;
        }
    };

    // Register global hotkey for exit (Ctrl+Shift+Q)
    register_exit_hotkey();

    // Main message loop
    run_message_loop(&mut widget, &mut tray_manager, rx, Arc::clone(&poll_interval_secs));

    // Save config on exit
    let final_config = config::Config {
        position: widget.position(),
        poll_interval_secs: poll_interval_secs.load(std::sync::atomic::Ordering::Relaxed),
        tray_hidden: !tray_manager.is_visible(),
    };
    final_config.save();
    log::log("Config saved, exiting");

    // Cleanup COM
    unsafe {
        windows::Win32::System::Com::CoUninitialize();
    }
}

#[cfg(windows)]
fn kill_existing_instance() {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        GetCurrentProcessId, OpenProcess, TerminateProcess, PROCESS_TERMINATE,
    };

    unsafe {
        let current_pid = GetCurrentProcessId();

        // Create snapshot of all processes
        let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) => h,
            Err(_) => return,
        };

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        // Iterate through processes
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                // Convert process name to string
                let name_len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..name_len]);

                // Check if it's deskvolt.exe but not the current process
                if name.eq_ignore_ascii_case("deskvolt.exe")
                    && entry.th32ProcessID != current_pid
                {
                    // Terminate the existing process
                    if let Ok(process) =
                        OpenProcess(PROCESS_TERMINATE, false, entry.th32ProcessID)
                    {
                        let _ = TerminateProcess(process, 0);
                        let _ = CloseHandle(process);
                    }
                }

                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);
    }
}

#[cfg(windows)]
fn enable_dpi_awareness() {
    use windows::Win32::UI::HiDpi::{SetProcessDpiAwareness, PROCESS_PER_MONITOR_DPI_AWARE};

    unsafe {
        // Try to set per-monitor DPI awareness for best scaling
        let _ = SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE);
    }
}

#[cfg(windows)]
fn register_exit_hotkey() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, MOD_CONTROL, MOD_SHIFT, VK_Q,
    };

    unsafe {
        // Register Ctrl+Shift+Q as exit hotkey (ID = 1)
        let _ = RegisterHotKey(None, 1, MOD_CONTROL | MOD_SHIFT, VK_Q.0 as u32);
    }
}

#[cfg(windows)]
fn run_message_loop(
    widget: &mut ui::Widget,
    tray_manager: &mut tray::TrayManager,
    rx: std::sync::mpsc::Receiver<Vec<device::DeviceStatus>>,
    poll_interval_secs: std::sync::Arc<std::sync::atomic::AtomicU64>,
) {
    use std::time::Duration;
    use muda::MenuEvent;
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_HOTKEY, WM_QUIT,
    };

    loop {
        // Process Windows messages (non-blocking)
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    return;
                }
                // Check for our exit hotkey
                if msg.message == WM_HOTKEY && msg.wParam.0 == 1 {
                    return;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        // Process tray menu events
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            if tray::handle_menu_event(event, tray_manager, &poll_interval_secs) {
                return; // Exit requested
            }
        }

        // Check for device status updates (non-blocking)
        if let Ok(statuses) = rx.try_recv() {
            widget.update_devices(statuses.clone());
            tray_manager.update_status(&statuses);
        }

        // Small sleep to prevent busy-waiting
        std::thread::sleep(Duration::from_millis(10));
    }
}
