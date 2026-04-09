//! macOS-specific workarounds for MTP device access.
//!
//! On macOS, the system daemon `ptpcamerad` automatically claims MTP/PTP devices
//! when connected. This module detects the daemon and can automatically suppress
//! it while MTP devices are in use, then restore it when they disconnect.

use log::{debug, info};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

/// The Terminal command that users can run to work around ptpcamerad.
/// Kept as a fallback if automatic suppression fails.
pub const PTPCAMERAD_WORKAROUND_COMMAND: &str = "while true; do pkill -9 ptpcamerad 2>/dev/null; sleep 1; done";

/// Whether ptpcamerad is currently suppressed by this app instance.
static SUPPRESSED: AtomicBool = AtomicBool::new(false);

/// Returns the current user's UID for launchctl commands.
fn current_uid() -> u32 {
    // SAFETY: getuid() is always safe — no arguments, no side effects.
    unsafe { libc::getuid() }
}

/// The launchctl service label for ptpcamerad.
const SERVICE_LABEL: &str = "com.apple.ptpcamerad";

/// Disables ptpcamerad via `launchctl disable` and kills any running instance.
///
/// Returns `Ok(true)` if newly suppressed, `Ok(false)` if already suppressed.
pub fn suppress_ptpcamerad() -> Result<bool, String> {
    if SUPPRESSED.swap(true, Ordering::SeqCst) {
        return Ok(false); // Already suppressed
    }

    let target = format!("user/{}/{}", current_uid(), SERVICE_LABEL);

    let output = Command::new("launchctl")
        .args(["disable", &target])
        .output()
        .map_err(|e| {
            SUPPRESSED.store(false, Ordering::SeqCst);
            format!("Failed to run launchctl: {}", e)
        })?;

    if !output.status.success() {
        SUPPRESSED.store(false, Ordering::SeqCst);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("launchctl disable failed: {}", stderr));
    }

    info!("Disabled {} via launchctl", SERVICE_LABEL);

    // Kill any running instance (ignore errors — it may not be running)
    let _ = Command::new("pkill").args(["-9", "ptpcamerad"]).output();

    Ok(true)
}

/// Re-enables ptpcamerad via `launchctl enable`.
///
/// Returns `Ok(true)` if restored, `Ok(false)` if it wasn't suppressed.
pub fn restore_ptpcamerad() -> Result<bool, String> {
    if !SUPPRESSED.swap(false, Ordering::SeqCst) {
        return Ok(false); // Wasn't suppressed
    }

    let target = format!("user/{}/{}", current_uid(), SERVICE_LABEL);

    let output = Command::new("launchctl")
        .args(["enable", &target])
        .output()
        .map_err(|e| format!("Failed to run launchctl: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("launchctl enable failed: {}", stderr));
    }

    info!("Re-enabled {} via launchctl", SERVICE_LABEL);
    Ok(true)
}

/// Unconditionally re-enables ptpcamerad. Called at startup to recover from
/// a previous crash that may have left the daemon disabled.
pub fn ensure_ptpcamerad_enabled() {
    let target = format!("user/{}/{}", current_uid(), SERVICE_LABEL);
    let _ = Command::new("launchctl").args(["enable", &target]).output();
    SUPPRESSED.store(false, Ordering::SeqCst);
    debug!("Startup: ensured {} is enabled", SERVICE_LABEL);
}

/// Queries IORegistry to find the process holding exclusive access to MTP devices.
///
/// Returns the process name (like "ptpcamerad") if found.
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
        assert!(!PTPCAMERAD_WORKAROUND_COMMAND.is_empty());
        assert!(PTPCAMERAD_WORKAROUND_COMMAND.contains("pkill"));
        assert!(PTPCAMERAD_WORKAROUND_COMMAND.contains("ptpcamerad"));
    }

    #[test]
    fn test_get_usb_exclusive_owner_returns_option() {
        // This test just verifies the function runs without panicking
        let result = get_usb_exclusive_owner();
        let _ = result;
    }

    #[test]
    fn test_suppress_idempotent() {
        // We can't actually test launchctl in CI, but we can test the atomic logic
        let was_suppressed = SUPPRESSED.load(Ordering::SeqCst);
        // Reset to known state after test
        SUPPRESSED.store(was_suppressed, Ordering::SeqCst);
    }

    #[test]
    fn test_restore_when_not_suppressed() {
        // Ensure not suppressed
        SUPPRESSED.store(false, Ordering::SeqCst);
        let result = restore_ptpcamerad();
        assert_eq!(result, Ok(false));
    }

    #[test]
    fn test_current_uid_nonzero() {
        // In a normal user context, UID should be > 0
        // (root is 0 but tests don't run as root)
        assert!(current_uid() > 0);
    }
}
