//! Stubs for platform-specific functionality.
//!
//! These stubs provide minimal implementations that allow the app to run
//! on platforms without real implementations. They return sensible defaults
//! that enable the core file manager functionality to work.

#[cfg(not(target_os = "linux"))]
pub mod accent_color;
#[cfg(not(target_os = "linux"))]
pub mod mtp;
#[cfg(not(target_os = "linux"))]
pub mod network;
#[cfg(not(target_os = "linux"))]
pub mod permissions;
#[cfg(not(target_os = "linux"))]
pub mod volumes;
