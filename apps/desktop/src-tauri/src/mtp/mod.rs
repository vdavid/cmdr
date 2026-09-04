//! MTP (Media Transfer Protocol) support for Android devices.
//!
//! This module provides device discovery and file operations for Android devices
//! connected via USB in "File transfer / Android Auto" mode.
//!
//! # Architecture
//!
//! - `types`: Type definitions for frontend communication
//! - `discovery`: Device detection using mtp-rs
//! - `connection`: Device connection management with global registry and file browsing
//! - `events`: the `tauri_specta` event payloads, and the adapter that turns the session layer's typed device events into them
//! - `volume_wiring`: Registers an attached storage as an `MtpVolume` (the session layer never does)
//! - `macos_workaround`: Handles ptpcamerad interference on macOS (macOS only)
//!
//! # Platform Support
//!
//! MTP support works on macOS and Linux. The underlying crate (`mtp-rs`) is pure
//! Rust and supports both platforms.
//! On macOS, the system daemon `ptpcamerad` may claim devices first;
//! see `macos_workaround` module for handling this.
//! On Linux, USB device permissions may require udev rules.

pub mod connection;
mod discovery;
pub mod events;
#[cfg(target_os = "macos")]
pub mod macos_workaround;
pub mod types;
#[cfg(feature = "virtual-mtp")]
pub mod virtual_device;
pub mod volume_wiring;
pub mod watcher;

pub use connection::{ConnectedDeviceInfo, MtpConnectionError, MtpDisconnectReason, MtpObjectInfo, connection_manager};
pub use discovery::list_mtp_devices;
pub use events::{
    MtpDeviceConnected, MtpDeviceDisconnected, MtpExclusiveAccessError, MtpPermissionError, MtpPtpcameradRestored,
    MtpPtpcameradSuppressed, MtpStorageRemoved,
};
pub use types::{MtpDeviceInfo, MtpStorageInfo};
pub use watcher::{set_mtp_enabled, set_mtp_enabled_flag, start_mtp_watcher};

/// The Terminal command that users can run to work around ptpcamerad on macOS.
/// Returns an empty string on non-macOS platforms (ptpcamerad doesn't exist there).
#[cfg(target_os = "macos")]
pub use macos_workaround::PTPCAMERAD_WORKAROUND_COMMAND;

#[cfg(not(target_os = "macos"))]
pub const PTPCAMERAD_WORKAROUND_COMMAND: &str = "";
