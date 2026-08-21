// The lint set this crate is held to lives in the workspace root's
// `[workspace.lints]`, opted into by `Cargo.toml`'s `lints.workspace = true`.
// These two can't go with them: `unused_crate_dependencies` is judged per
// compilation unit (as a package-wide flag every test target would report unused
// externs for deps only the lib uses), and `missing_docs` is this crate's own
// contract — its API is a deliverable rather than a side effect.
#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! The SMB backend's protocol layer: everything Cmdr says to an SMB server that
//! needs no application around it.
//!
//! What lives here is decided by one question — can it be answered from the
//! protocol and its own types alone? Address building, error classification, and
//! the share-listing vocabulary can, so they're here. Discovery (mDNS), the
//! keychain, kernel mounts, the upgrade passes, and every event the frontend sees
//! can't, so they stay in the app's `network/` module.
//!
//! Nothing here may name the app, and `index-crate-isolation` enforces that:
//! `cargo check -p cmdr-smb` is the whole verification loop.
//!
//! Three modules, all re-exported at the root so callers write `cmdr_smb::`:
//!
//! - [`types`] — the share-listing vocabulary ([`ShareInfo`], [`AuthMode`],
//!   [`ShareListResult`], [`ShareListError`]). These cross IPC, so they carry
//!   serde and `specta::Type`.
//! - [`errors`] — turning an [`smb2::Error`] into a [`ShareListError`], and the
//!   `is this an auth problem?` predicate every retry path asks.
//! - [`connection`] — the smb2 address string, and the two guest / authenticated
//!   share-listing calls.
//!
//! See `CLAUDE.md` for the must-knows and `DETAILS.md` for the boundary's
//! rationale.

pub mod connection;
pub mod errors;
pub mod types;

pub use connection::{build_smb_addr, try_list_shares_as_guest, try_list_shares_authenticated};
pub use errors::{classify_authenticated_error, classify_error, is_auth_error};
pub use types::{AuthMode, ShareInfo, ShareListError, ShareListResult, convert_shares};
