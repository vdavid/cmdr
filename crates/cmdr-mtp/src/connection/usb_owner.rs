//! Which process is holding the USB device we couldn't open (macOS).
//!
//! Asked at exactly one moment: an open came back with an exclusive-access
//! error, and the answer is what turns "the device is busy" into "ptpcamerad has
//! it", which is the difference between a dead end and an offer the app can
//! make. Nothing about the answer needs an app around it, so it lives beside the
//! failure that asks for it; the ptpcamerad SUPPRESSION lifecycle it feeds (the
//! `launchctl` calls, the notice, the restore on quit) is the app's.

use log::debug;
use std::process::Command;

/// The process holding exclusive access to an MTP device, as `"pid <n>, <name>"`
/// when the registry gives both, or the raw owner string when it doesn't.
///
/// `None` when nothing holds one, or when `ioreg` isn't answerable. It's a
/// best-effort attribution for a message, never a control-flow input: the open
/// already failed by the time anyone asks.
pub(super) fn get_usb_exclusive_owner() -> Option<String> {
    // Run ioreg to query USB device ownership
    let output = Command::new("ioreg").args(["-l", "-w", "0"]).output().ok()?;

    if !output.status.success() {
        debug!("ioreg command failed");
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Look for lines containing "UsbExclusiveOwner" and "ptpcamera"
    for line in stdout.lines() {
        if line.contains("UsbExclusiveOwner") && line.contains("ptpcamera") {
            // Parse: "UsbExclusiveOwner" = "pid 45145, ptpcamerad"
            if let Some(value) = line.split('=').nth(1) {
                let value = value.trim().trim_matches('"');
                // Parse "pid 45145, ptpcamerad"
                if let Some(stripped) = value.strip_prefix("pid ") {
                    let parts: Vec<&str> = stripped.splitn(2, ", ").collect();
                    if parts.len() == 2 {
                        debug!("Found USB exclusive owner: {} (pid {})", parts[1], parts[0]);
                        return Some(format!("pid {}, {}", parts[0], parts[1]));
                    }
                }
            }
        }
    }

    // Also check for other processes that might hold the device
    for line in stdout.lines() {
        if line.contains("UsbExclusiveOwner")
            && let Some(value) = line.split('=').nth(1)
        {
            let value = value.trim().trim_matches('"').trim();
            if !value.is_empty() {
                debug!("Found USB exclusive owner: {}", value);
                return Some(value.to_string());
            }
        }
    }

    debug!("No USB exclusive owner found");
    None
}

#[cfg(test)]
mod tests {
    use super::get_usb_exclusive_owner;

    /// Whatever the developer's machine has plugged in, the query answers rather
    /// than panicking: it shells out and parses, and both halves have to tolerate
    /// a registry that says nothing.
    #[test]
    fn asking_who_holds_the_device_always_answers() {
        let result = get_usb_exclusive_owner();
        let _ = result.is_some();
    }
}
