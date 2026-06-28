//! The `Location` type: a `(volume_id, path)` pair that is navigation's currency.
//!
//! A bare path is ambiguous: the same path string can name a folder on the local
//! disk, an SMB share, or an MTP device. Pairing it with a mandatory `volume_id`
//! makes "a path without a volume" unrepresentable, which is what keeps navigation
//! from listing a local path over the wrong (e.g. SMB) connection.
//!
//! `Location` is platform-independent and shared by all three volume command
//! backends (macOS, Linux, stub). The `resolve_location` command (per platform)
//! turns a bare path into a `Location` and is the specta-export vehicle that
//! lands both types in `bindings.ts`. Distinct from `volumes::LocationInfo`,
//! which describes a volume/favorite, not a navigable destination.

use serde::{Deserialize, Serialize};

/// A navigable destination: a path together with the volume it lives on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub volume_id: String,
    pub path: String,
}

/// Result of resolving a bare path into a `Location` via `resolve_location`.
///
/// `timed_out: true` means the filesystem didn't respond, so the volume is
/// genuinely unknown; that is distinct from `location: None` (we resolved, but
/// no volume contains the path). Callers must not collapse the two.
#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolveLocationResult {
    pub location: Option<Location>,
    pub timed_out: bool,
}
