//! Build script for DeskVolt.
//!
//! Generates the application icon and embeds Windows resources.

use std::fs::File;
use std::io::Write;
use std::path::Path;

fn main() {
    // Only run on Windows
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "windows" {
        return;
    }

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir);

    // Generate the ICO file in OUT_DIR (works in CI)
    let ico_path = out_path.join("deskvolt.ico");
    generate_battery_icon(&ico_path);

    // Generate the RC file
    let rc_path = out_path.join("deskvolt.rc");
    let mut rc_file = File::create(&rc_path).expect("Failed to create RC file");
    // Use forward slashes for RC file compatibility
    let ico_path_str = ico_path.display().to_string().replace('\\', "/");
    writeln!(rc_file, "1 ICON \"{}\"", ico_path_str).expect("Failed to write RC file");

    // Compile the resources
    embed_resource::compile(&rc_path, embed_resource::NONE);
}

/// Generate a simple battery icon in ICO format.
fn generate_battery_icon(path: &Path) {
    const SIZE: usize = 32;

    // Generate RGBA pixel data for a battery icon
    let mut rgba = vec![0u8; SIZE * SIZE * 4];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let idx = (y * SIZE + x) * 4;

            // Battery body: rounded rectangle from (4,8) to (26,24)
            // Battery tip: small rectangle from (26,11) to (28,21)
            let in_body = x >= 4 && x <= 26 && y >= 8 && y <= 24;
            let in_tip = x >= 26 && x <= 28 && y >= 11 && y <= 21;

            // Green fill for the battery (showing ~75% charge)
            let fill_width = 16; // 75% of body width
            let is_filled = x >= 5 && x < 5 + fill_width && y >= 9 && y <= 23;

            if in_body || in_tip {
                if is_filled {
                    // Green filled portion
                    rgba[idx] = 80;      // R
                    rgba[idx + 1] = 200; // G
                    rgba[idx + 2] = 80;  // B
                    rgba[idx + 3] = 255; // A
                } else {
                    // Border/outline
                    let is_border = x == 4 || x == 26 || y == 8 || y == 24
                        || (in_tip && (x == 28 || y == 11 || y == 21));
                    if is_border {
                        rgba[idx] = 220;
                        rgba[idx + 1] = 220;
                        rgba[idx + 2] = 220;
                        rgba[idx + 3] = 255;
                    } else {
                        // Dark background inside battery
                        rgba[idx] = 45;
                        rgba[idx + 1] = 45;
                        rgba[idx + 2] = 45;
                        rgba[idx + 3] = 255;
                    }
                }
            }
            // Outside: transparent (already 0)
        }
    }

    // Convert RGBA to ICO format (simplified single-size ICO)
    let ico_data = create_ico(&rgba, SIZE as u32);

    let mut file = File::create(path).expect("Failed to create ICO file");
    file.write_all(&ico_data).expect("Failed to write ICO file");
}

/// Create a simple ICO file with a single 32x32 image.
fn create_ico(rgba: &[u8], size: u32) -> Vec<u8> {
    let mut ico = Vec::new();

    // ICO header
    ico.extend_from_slice(&[0, 0]); // Reserved
    ico.extend_from_slice(&[1, 0]); // Type: 1 = ICO
    ico.extend_from_slice(&[1, 0]); // Number of images

    // Image directory entry
    ico.push(size as u8);           // Width
    ico.push(size as u8);           // Height
    ico.push(0);                     // Color palette (0 = no palette)
    ico.push(0);                     // Reserved
    ico.extend_from_slice(&[1, 0]); // Color planes
    ico.extend_from_slice(&[32, 0]); // Bits per pixel

    // Calculate sizes
    let row_size = size * 4; // RGBA
    let pixel_data_size = row_size * size;
    let mask_row_size = ((size + 31) / 32) * 4;
    let mask_size = mask_row_size * size;
    let bmp_size = 40 + pixel_data_size + mask_size; // BITMAPINFOHEADER + pixels + mask

    // Image data size and offset
    ico.extend_from_slice(&(bmp_size as u32).to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes()); // Offset to image data (6 + 16)

    // BITMAPINFOHEADER
    ico.extend_from_slice(&40u32.to_le_bytes()); // Header size
    ico.extend_from_slice(&(size as i32).to_le_bytes()); // Width
    ico.extend_from_slice(&((size * 2) as i32).to_le_bytes()); // Height (doubled for ICO)
    ico.extend_from_slice(&1u16.to_le_bytes()); // Planes
    ico.extend_from_slice(&32u16.to_le_bytes()); // Bits per pixel
    ico.extend_from_slice(&0u32.to_le_bytes()); // Compression
    ico.extend_from_slice(&(pixel_data_size + mask_size).to_le_bytes()); // Image size
    ico.extend_from_slice(&0u32.to_le_bytes()); // X pixels per meter
    ico.extend_from_slice(&0u32.to_le_bytes()); // Y pixels per meter
    ico.extend_from_slice(&0u32.to_le_bytes()); // Colors used
    ico.extend_from_slice(&0u32.to_le_bytes()); // Important colors

    // Pixel data (BGRA, bottom-to-top)
    for y in (0..size as usize).rev() {
        for x in 0..size as usize {
            let src_idx = (y * size as usize + x) * 4;
            ico.push(rgba[src_idx + 2]); // B
            ico.push(rgba[src_idx + 1]); // G
            ico.push(rgba[src_idx]);     // R
            ico.push(rgba[src_idx + 3]); // A
        }
    }

    // AND mask (all zeros for full opacity with alpha channel)
    for _ in 0..mask_size {
        ico.push(0);
    }

    ico
}
