//! Configuration persistence for widget position and settings.
//!
//! Settings are always stored in the same directory as the executable,
//! making the application fully portable.

use std::env;
use std::fs;
use std::path::PathBuf;

const CONFIG_FILE: &str = "deskvolt.ini";

/// Widget position on screen.
#[derive(Debug, Clone, Copy)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

impl Default for Position {
    fn default() -> Self {
        // Default to bottom-right corner with some margin
        Self { x: -1, y: -1 } // -1 means "auto-position"
    }
}

/// Application configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub position: Position,
    pub poll_interval_secs: u64,
    pub tray_hidden: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            position: Position::default(),
            poll_interval_secs: 2, // 2 seconds for responsive updates
            tray_hidden: false,    // Tray visible by default
        }
    }
}

impl Config {
    /// Load configuration from file, or return defaults.
    pub fn load() -> Self {
        let path = match get_config_path() {
            Some(p) => p,
            None => return Self::default(),
        };

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return Self::default(),
        };

        let mut config = Self::default();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();

                match key {
                    "position_x" => {
                        if let Ok(x) = value.parse() {
                            config.position.x = x;
                        }
                    }
                    "position_y" => {
                        if let Ok(y) = value.parse() {
                            config.position.y = y;
                        }
                    }
                    "poll_interval" => {
                        if let Ok(interval) = value.parse() {
                            config.poll_interval_secs = interval;
                        }
                    }
                    "tray_hidden" => {
                        config.tray_hidden = value == "true" || value == "1";
                    }
                    _ => {}
                }
            }
        }

        config
    }

    /// Save configuration to file.
    pub fn save(&self) {
        let path = match get_config_path() {
            Some(p) => p,
            None => return,
        };

        let content = format!(
            "# DeskVolt Configuration\n\
             position_x={}\n\
             position_y={}\n\
             poll_interval={}\n\
             tray_hidden={}\n",
            self.position.x, self.position.y, self.poll_interval_secs, self.tray_hidden
        );

        let _ = fs::write(&path, content);
    }
}

/// Load tray hidden state (standalone function for tray module).
pub fn load_tray_hidden() -> bool {
    Config::load().tray_hidden
}

/// Save tray hidden state (standalone function for tray module).
pub fn save_tray_hidden(hidden: bool) {
    let mut config = Config::load();
    config.tray_hidden = hidden;
    config.save();
}

/// Get the executable's directory.
fn get_exe_dir() -> Option<PathBuf> {
    env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf()))
}

/// Get the path to the configuration file (always in exe directory).
fn get_config_path() -> Option<PathBuf> {
    get_exe_dir().map(|dir| dir.join(CONFIG_FILE))
}
