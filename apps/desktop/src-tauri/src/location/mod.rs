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
//!
//! The tools' location resolver lives here too: `resolve.rs` (the pure core and
//! the one root function), `facts.rs` (the in-memory volume facts it reads, and
//! the earlier-roots record), and `miss.rs` (a miss as a tool result). Its
//! contract: `tools/DETAILS.md` § The location contract. `store_path.rs`
//! translates app paths to and from an importance store's path space, and
//! `same_share.rs` proves two volumes one share and picks which may answer (and
//! which member a connect acts on), over the `share_identity.rs` each fact carries.

use serde::{Deserialize, Serialize};

pub(crate) mod facts;
pub(crate) mod miss;
pub(crate) mod resolve;
pub(crate) mod same_share;
pub(crate) mod share_identity;
pub(crate) mod store_path;

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
