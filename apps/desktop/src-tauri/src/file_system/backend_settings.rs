//! The app's answer to "what has the user tuned for this backend?".
//!
//! A storage backend reads its live knobs through
//! `cmdr_fs::volume::host::settings::BackendSettings` rather than a settings
//! file, an env var, or a preferences store. This resolves them from what
//! `settings::load_settings` already pushed into `file_system`.
//!
//! Live by design: the value is read per batch dispatch, so moving the slider
//! takes effect on the next batch without remounting.

use cmdr_fs::volume::host::settings::{BackendName, BackendSettings};

/// Answers a backend's live knobs from the app's stored settings.
pub struct AppBackendSettings;

impl BackendSettings for AppBackendSettings {
    fn max_concurrent_operations(&self, _backend: BackendName) -> usize {
        // ❌ Deliberately not a match on the namespace. There is exactly ONE
        // stored concurrency knob today (Settings > Advanced, clamped to 1..=32
        // by `set_smb_concurrency`), so every backend reads it; when a second
        // backend earns its own slider this becomes a lookup keyed by the
        // namespace, never a branch on it.
        super::smb_concurrency()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::{set_smb_concurrency, smb_concurrency};

    /// The seam has to see what the user last set, not a value captured when the
    /// volume mounted. The clamp is the app's, and it applies through the seam
    /// too.
    #[test]
    fn the_seam_reads_the_live_setting_and_its_clamp() {
        let previous = smb_concurrency();

        set_smb_concurrency(7);
        assert_eq!(AppBackendSettings.max_concurrent_operations("smb"), 7);

        set_smb_concurrency(9_000);
        assert_eq!(
            AppBackendSettings.max_concurrent_operations("smb"),
            32,
            "a misconfigured settings file can't overwhelm a server through the seam either"
        );

        set_smb_concurrency(previous);
    }
}
