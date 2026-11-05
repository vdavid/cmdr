#![warn(unused_crate_dependencies)]
#![deny(missing_docs)]

//! What makes a macOS update archive a Cmdr release: it unpacks, with the installer's own
//! extraction, to a `Cmdr.app` that carries Cmdr's Developer ID code signature.
//!
//! One home for that check, shared by the two places that must agree on it: the app's updater,
//! which refuses to install anything else, and the release gate (`cmdr-vet-update`), which refuses
//! to sign or publish anything the updater would refuse. `DETAILS.md` has the why. macOS only: on
//! other platforms the crate is empty.

#[cfg(target_os = "macos")]
mod archive;
#[cfg(target_os = "macos")]
mod code_signature;
#[cfg(all(test, target_os = "macos"))]
mod fixtures;

#[cfg(target_os = "macos")]
pub use archive::{ArchiveRefusal, bundle_version, extract_tarball, vet_update_archive};
#[cfg(target_os = "macos")]
pub use code_signature::{CodeSignatureRefusal, verify_release_bundle};

// Throwaway probe for light and land runs; never lands.
