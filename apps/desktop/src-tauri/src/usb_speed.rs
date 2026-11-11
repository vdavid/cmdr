//! Negotiated USB link speed exposed to the frontend.
//!
//! The type itself lives in `cmdr_fs::volume`, where both device backends and
//! the shared `LocationInfo` / `VolumeInfo` shape can reach it. This re-export
//! keeps the app's own `crate::usb_speed::UsbSpeed` path, which every volume
//! producer already spells.
//!
//! MTP discovery fills it in for a phone's storages; every other row carries
//! `None`.

pub use cmdr_fs::volume::UsbSpeed;
