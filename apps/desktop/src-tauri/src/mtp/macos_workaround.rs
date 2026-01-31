//! macOS-specific workarounds for MTP device access.
//!
//! On macOS, the system daemon `ptpcamerad` automatically claims MTP/PTP devices
//! when connected. This module provides utilities to detect and help users
//! work around this issue.

use log::debug;
use std::process::Command;

/// The Terminal command that users can run to work around ptpcamerad.
pub const PTPCAMERAD_WORKAROUND_COMMAND: &str = "while true; do pkill -9 ptpcamerad 2>/dev/null; sleep 1; done";

/// Queries IORegistry to find the process holding exclusive access to MTP devices.
///
/// Returns the process name (e.g., "ptpcamerad") if found.
///
/// # How it works
///
/// Uses the `ioreg` command to query USB device ownership. The output contains
/// lines like: `"UsbExclusiveOwner" = "pid 45145, ptpcamerad"`
pub fn get_usb_exclusive_owner() -> Option<String> {
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

/// Checks if ptpcamerad is likely blocking MTP access.
///
/// Returns true if ptpcamerad is running and has a USB device claimed.
#[allow(dead_code, reason = "Utility function for future use in diagnostics")]
pub fn is_ptpcamerad_blocking() -> bool {
    get_usb_exclusive_owner()
        .map(|owner| owner.contains("ptpcamera"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workaround_command_is_valid_bash() {
        // Just ensure the constant is set
        assert!(!PTPCAMERAD_WORKAROUND_COMMAND.is_empty());
        assert!(PTPCAMERAD_WORKAROUND_COMMAND.contains("pkill"));
        assert!(PTPCAMERAD_WORKAROUND_COMMAND.contains("ptpcamerad"));
    }

    #[test]
    fn test_get_usb_exclusive_owner_returns_option() {
        // This test just verifies the function runs without panicking
        // The result depends on system state
        let result = get_usb_exclusive_owner();
        // Result is Option<String> - either Some or None is valid
        let _ = result;
    }
}
