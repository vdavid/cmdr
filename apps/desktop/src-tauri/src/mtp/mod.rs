//! MTP (Media Transfer Protocol) support for Android devices.
//!
//! This module provides device discovery and file operations for Android devices
//! connected via USB in "File transfer / Android Auto" mode.
//!
//! # Architecture
//!
//! - `types`: Type definitions for frontend communication
//! - `discovery`: Device detection using mtp-rs
//!
//! # Platform Support
//!
//! MTP support is currently macOS-only due to USB access requirements.
//! On macOS, the system daemon `ptpcamerad` may claim devices first;
//! see `macos_workaround` module (Phase 2) for handling this.

mod discovery;
pub mod types;

pub use discovery::list_mtp_devices;
pub use types::MtpDeviceInfo;
