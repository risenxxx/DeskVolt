//! Background worker thread for device polling.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::device::{DeviceRegistry, DeviceStatus};

/// Start the background worker thread that polls devices at the specified interval.
/// The poll_interval_secs is shared and can be modified at runtime.
pub fn start_worker(tx: Sender<Vec<DeviceStatus>>, poll_interval_secs: Arc<AtomicU64>) {
    thread::spawn(move || {
        let mut registry = DeviceRegistry::new();

        // Initial device discovery
        registry.discover();

        // Send initial status
        let statuses = registry.poll_all();
        let _ = tx.send(statuses);

        loop {
            // Read current poll interval (can change at runtime)
            let interval_secs = poll_interval_secs.load(Ordering::Relaxed);
            thread::sleep(Duration::from_secs(interval_secs));

            // Re-discover devices periodically (handles reconnection)
            if registry.device_count() == 0 {
                registry.discover();
            }

            // Poll all devices
            let statuses = registry.poll_all();

            // Send update to main thread
            if tx.send(statuses).is_err() {
                // Main thread has closed the channel, exit worker
                break;
            }
        }
    });
}
