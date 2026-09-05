//! Negotiated USB link speed, as the volume list carries it.
//!
//! Vocabulary rather than protocol: a `LocationInfo` and a `DeviceVolumeEntry`
//! carry it for BOTH device backends, and the frontend renders it, so it belongs
//! here with the rest of the shared shape and not inside whichever backend
//! happened to read it first.
//!
//! There is deliberately no `From` impl for a protocol crate's own speed enum.
//! `cmdr-fs` must not depend on `mtp-rs` (or on any transport), and neither may
//! a backend crate write the impl, since both types would be foreign to it. A
//! device backend converts with a plain function of its own.

use serde::{Deserialize, Serialize};

// ❗ The doc comment below reaches the frontend: `specta` copies it into
// `bindings.ts`, so editing it is a wire-contract change and shows up as a
// bindings diff. Callers always hold an `Option<UsbSpeed>`, `None` on every
// non-USB volume and on any platform whose producer isn't compiled in.
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
