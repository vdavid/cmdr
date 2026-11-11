//! Where each OS keeps the SMB passwords another app saved, for the "Use the password macOS saved"
//! door. A platform seam (`docs/guides/platform-seams.md`).

use std::future::Future;

use super::UpgradeResult;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod unsupported;

// An `async fn` doesn't coerce to a fn pointer, so this pin names each one's argument and the
// output its future resolves to.
const _: fn(&str) = |volume_id| {
    fn resolves_to<T, F: Future<Output = T>>(_: F) {}
    resolves_to::<bool, _>(system_has_saved_password(volume_id));
    resolves_to::<UpgradeResult, _>(connect_directly_with_system_saved_password(volume_id));
};

/// Whether the system keychain holds an SMB password another app (Finder) saved for
/// `volume_id`'s server, read WITHOUT the consent dialog, so the frontend can decide whether to
/// offer it. `false` where the OS keeps none Cmdr reads.
pub(crate) use imp::system_has_saved_password;

/// Upgrades `volume_id` with that saved password, raising the consent dialog to read it, so it's
/// user-initiated only. Nothing saved answers `CredentialsNeeded`, which opens the sign-in sheet.
pub(crate) use imp::connect_directly_with_system_saved_password;
