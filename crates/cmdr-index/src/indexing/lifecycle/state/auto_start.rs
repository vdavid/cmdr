//! Should the indexer start itself at launch?
//!
//! Two pure predicates, no registry and no I/O: the settings answer, and the
//! settings answer combined with the FDA gate. Kept apart from the machinery they
//! gate so the launch policy reads (and tests) on its own.

/// Whether indexing should auto-start on launch.
///
/// - If settings say disabled (`indexing_enabled == Some(false)`): never auto-start.
/// - Otherwise: auto-start by default (both dev and release builds).
pub fn should_auto_start(indexing_enabled: Option<bool>) -> bool {
    // User explicitly disabled indexing in settings
    if indexing_enabled == Some(false) {
        return false;
    }

    // Default true (setting not yet stored means first launch, enabled by default)
    true
}

/// Pure decision: should the indexer auto-start at app launch?
///
/// Combines the user's indexing-enabled setting with the FDA gate. The FDA gate
/// blocks the indexer from scanning `/` before the user has decided about Full
/// Disk Access, otherwise macOS native permission popups (iCloud, Photos, etc.)
/// stack on top of the in-app FDA modal at first launch.
///
/// Auto-start when ALL of the following hold:
/// - The user has not disabled indexing (`indexing_enabled != Some(false)`).
/// - The FDA gate isn't pending. The host decides that (`fda_gate::is_fda_pending`) and
///   passes the answer in, so the index never has to know what a TCC choice is: the gate is
///   pending only while the in-app onboarding modal is still up. Once the user picks Deny (same
///   session via `start_indexing_after_fda_decision`) or Allow (which restarts the app), the
///   indexer auto-starts. After Deny, the scan triggers per-folder TCC prompts as it walks
///   protected paths: that's the "individual Allow/Deny prompts" contract the user opted into by
///   denying FDA.
///
/// **FDA gates only the local (`root`) volume** (scanning `/` triggers TCC). SMB/MTP volumes are
/// not TCC-protected, so a future per-volume "Turn on indexing" for them must NOT route through
/// this gate. When no network drive is indexed, only `root` is ever started, so this is the
/// only auto-start path.
pub fn should_auto_start_indexing(indexing_enabled: Option<bool>, fda_pending: bool) -> bool {
    should_auto_start(indexing_enabled) && !fda_pending
}
