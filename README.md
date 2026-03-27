<div align="center">

# DeskVolt

**Lightweight Windows desktop widget for wireless peripheral battery status**

<img src="https://github.com/user-attachments/assets/1d8dd71f-76fb-4caf-80d7-233a561d1152" alt="DeskVolt Screenshot" width="208" />

*Direct HID communication. No bloatware. No G HUB. No SteelSeries GG.*

</div>

---

## Supported Devices

### Tested

| Device | Protocol | Status |
|--------|----------|--------|
| Logitech G Pro X Superlight 2c | HID++ 2.0 | Tested |
| SteelSeries Arctis Nova 5 | Proprietary HID | Tested |

### Supported (Not Yet Tested)

| Device | Protocol |
|--------|----------|
| **Logitech Mice** | |
| G Pro X Superlight 2 | HID++ 2.0 |
| **Logitech Headsets** | |
| G Pro X 2 LIGHTSPEED | Vendor-specific 64-byte |
| G Pro X Wireless | HID++ (voltage-based) |
| G633 / G635 | HID++ (voltage-based) |
| G733 | HID++ (voltage-based) |
| G933 / G935 | HID++ (voltage-based) |
| **SteelSeries Headsets** | |
| Arctis Nova 5P/5X | Proprietary HID |
| Arctis 7 (2017/2019) | Legacy HID |
| Arctis 7P / 7P+ / 7+ | Legacy HID |
| Arctis Pro Wireless | Legacy HID |
| **Corsair Headsets** | |
| Void Wireless / Pro / Elite | Proprietary HID |
| Virtuoso RGB Wireless / SE / XT | Proprietary HID |
| **HyperX Headsets** | |
| Cloud Alpha Wireless | Proprietary HID |
| **PlayStation Controllers** | |
| DualSense (PS5) | Standard HID |
| DualSense Edge | Standard HID |
| DualShock 4 (PS4) | Standard HID |

## Features

- **Direct HID Communication**: No vendor software required
- **Lightweight**: Near-zero CPU usage, <10MB RAM
- **Draggable Widget**: Position anywhere on screen
- **Auto-discovery**: Automatically detects supported devices
- **Status Icons**: Charging indicator, low battery warning
- **Portable by Design**: Config stored in installation folder

## Installation

Download the latest release from [Releases](https://github.com/risenxxx/deskvolt/releases):

- **`deskvolt-setup-*.exe`** - Installer (recommended, allows custom install location)
- **`deskvolt.exe`** - Standalone executable (just extract and run)

## Building

### Requirements

- Windows 10/11
- Rust 1.70+ (install via [rustup](https://rustup.rs))
- Visual Studio Build Tools (for `windows-rs`)

### Build Commands

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release
```

The release binary will be at `target/release/deskvolt.exe`.

## Usage

1. Run `deskvolt.exe`
2. The widget appears in the bottom-right corner
3. Drag to reposition (position is saved)
4. Press `Ctrl+Shift+Q` to exit

## Configuration

Config file (`deskvolt.ini`) is stored in the same folder as the executable.

```ini
# DeskVolt Configuration
position_x=1620
position_y=980
poll_interval=2
```

| Setting | Description | Default |
|---------|-------------|---------|
| `position_x` | Widget X position (-1 for auto) | -1 |
| `position_y` | Widget Y position (-1 for auto) | -1 |
| `poll_interval` | Battery poll interval in seconds | 2 |

Poll interval can also be changed via the tray icon menu (2s, 5s, 10s, 30s options).

## Adding Device Support

DeskVolt is designed to be extensible. To add a new device:

1. Create a new file in `src/devices/` (e.g., `corsair.rs`)
2. Implement the `Device` trait from `src/device.rs`
3. Add device discovery to `DeviceRegistry::discover()`

See existing implementations for reference:
- `src/devices/logitech.rs` - HID++ 2.0 protocol
- `src/devices/steelseries.rs` - SteelSeries proprietary protocol

## Protocol References

### Logitech HID++ 2.0
- [Linux kernel driver](https://github.com/torvalds/linux/blob/master/drivers/hid/hid-logitech-hidpp.c)
- [Solaar project](https://github.com/pwr-Solaar/Solaar)

### Headsets (SteelSeries, Corsair, HyperX, Logitech)
- [HeadsetControl project](https://github.com/Sapd/HeadsetControl)

### PlayStation Controllers
- [dualsensectl](https://github.com/nowrep/dualsensectl)
- [ds4drv](https://github.com/chrippa/ds4drv)

## License

MIT
