//! Negotiated USB link speed exposed to the frontend.
//!
//! Cross-platform mirror of `mtp_rs::UsbSpeed` so the shared `LocationInfo` /
//! `VolumeInfo` shape stays identical across macOS, Linux, and stub platforms.
//! Producer side (MTP discovery) lives behind `#[cfg(any(target_os = "macos",
//! target_os = "linux"))]`; everything else carries `None`.

use serde::{Deserialize, Serialize};

/// Negotiated USB link speed (slowest of host port, cable, device).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum UsbSpeed {
    /// USB 1.0 low-speed (1.5 Mbit/s).
    Low,
    /// USB 1.1 full-speed (12 Mbit/s).
    Full,
    /// USB 2.0 high-speed (480 Mbit/s).
    High,
    /// USB 3.2 Gen 1 / formerly USB 3.0 (5 Gbit/s).
    Super,
    /// USB 3.2 Gen 2 / formerly USB 3.1 Gen 2 (10 Gbit/s).
    SuperPlus,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl From<mtp_rs::UsbSpeed> for UsbSpeed {
    fn from(s: mtp_rs::UsbSpeed) -> Self {
        match s {
            mtp_rs::UsbSpeed::Low => UsbSpeed::Low,
            mtp_rs::UsbSpeed::Full => UsbSpeed::Full,
            mtp_rs::UsbSpeed::High => UsbSpeed::High,
            mtp_rs::UsbSpeed::Super => UsbSpeed::Super,
            mtp_rs::UsbSpeed::SuperPlus => UsbSpeed::SuperPlus,
        }
    }
}
