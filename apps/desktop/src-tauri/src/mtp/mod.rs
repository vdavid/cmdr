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
//! - `macos_workaround`: Handles ptpcamerad interference on macOS
//!
//! # Platform Support
//!
//! MTP support is currently macOS-only due to USB access requirements.
//! On macOS, the system daemon `ptpcamerad` may claim devices first;
//! see `macos_workaround` module for handling this.

pub mod connection;
mod discovery;
pub mod macos_workaround;
pub mod types;

pub use connection::{ConnectedDeviceInfo, MtpConnectionError, MtpObjectInfo, MtpOperationResult, connection_manager};
pub use discovery::list_mtp_devices;
pub use macos_workaround::PTPCAMERAD_WORKAROUND_COMMAND;
pub use types::{MtpDeviceInfo, MtpStorageInfo};
