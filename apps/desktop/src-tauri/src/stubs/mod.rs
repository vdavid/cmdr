//! Linux/non-macOS stubs for platform-specific functionality.
//!
//! These stubs provide minimal implementations that allow the app to run
//! on Linux for E2E testing purposes. They return sensible defaults that
//! enable the core file manager functionality to work.

pub mod mtp;
pub mod network;
pub mod permissions;
pub mod volumes;
