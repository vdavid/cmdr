//! This app, acting as the host every storage backend asks its questions of.
//!
//! The mirror of `cmdr_fs::volume::host`: that module declares everything a
//! backend needs from the application around it, and this is the one place that
//! answers. Each answer is a small adapter living next to the subsystem that can
//! actually give it; all this does is collect them.
//!
//! - listings — `file_system::listing::listing_host::AppListings`
//! - events — `events::volume_mapping::TauriVolumeEvents`
//! - credentials — `network::credential_store::KeychainCredentials`
//! - indexing — `index_host::VolumeIndexNotifier`
//! - activity — `priority::host_policy::AppUserActivity`
//! - analytics — `analytics::volume_sink::PostHogVolumeAnalytics`
//! - settings — `file_system::backend_settings::AppBackendSettings`
//! - runtime — the app's own tokio handle, so there's one thread pool
//!
//! ## The host is a value
//!
//! `VolumeHost` is cheaply cloned (eight `Arc` bumps), and a backend takes one in
//! its constructor and keeps it. The `OnceLock` here is just where the app parks
//! the one it built, so a connect path reached from an IPC command can pick it
//! up; ❌ it is NOT how a backend finds its host. A test builds its own with
//! fakes and passes it in, which is why nothing here needs an install-and-restore
//! guard and no test has to be serialized against the rest of the binary.

use std::sync::{Arc, OnceLock};

use tauri::AppHandle;

use cmdr_fs::volume::host::VolumeHost;

/// The host [`install`] built. Backend construction paths reach it through
/// [`host`].
static HOST: OnceLock<VolumeHost> = OnceLock::new();

/// Wire every seam a backend can ask about to this app: runtime, listings,
/// events, credentials, index notifications, activity, analytics, and settings.
///
/// Call once, in `setup()`, before anything constructs a volume. Nothing here can
/// fail: a seam that isn't installed answers with a no-op rather than an error,
/// so a late call costs pane updates, not a launch.
pub fn install(app: &AppHandle) {
    let host = VolumeHost::builder()
        // The app's own runtime, so background work a backend starts shares one
        // thread pool with everything else. Backends need this because their
        // watchers run on OS threads with no reactor, where `tokio::spawn`
        // panics.
        .runtime(tauri::async_runtime::handle().inner().clone())
        .listings(Arc::new(crate::file_system::listing::listing_host::AppListings))
        // The typed connection transition becomes a Tauri payload in
        // `events/volume_mapping.rs` and nowhere else, which is what keeps every
        // user-facing word app-side.
        .events(Arc::new(crate::events::volume_mapping::TauriVolumeEvents::new(
            app.clone(),
        )))
        .credentials(Arc::new(crate::network::credential_store::KeychainCredentials))
        // Mapped rather than passed through: a backend crate must not depend on
        // the index, or `cargo check` on it would compile a quarter of the app.
        .indexing(Arc::new(crate::index_host::VolumeIndexNotifier))
        .activity(Arc::new(crate::priority::host_policy::AppUserActivity))
        .analytics(Arc::new(crate::analytics::volume_sink::PostHogVolumeAnalytics))
        .settings(Arc::new(crate::file_system::backend_settings::AppBackendSettings))
        .build();

    if HOST.set(host).is_err() {
        log::warn!(target: "volume", "the volume host was already installed; keeping the first one");
    }
}

/// The host to hand a backend being constructed.
///
/// Before [`install`] — a test binary, a bench, a tool — this is
/// `VolumeHost::detached()`: every seam answers with a no-op, which is a complete
/// host and not a stub. That's deliberate, so no construction path needs an
/// `Option<VolumeHost>` or a "the app isn't up yet" branch.
pub fn host() -> VolumeHost {
    HOST.get().cloned().unwrap_or_else(VolumeHost::detached)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nothing installs a host in a test binary, so every backend built there
    /// gets a complete, silent one. If this ever panicked or hung, every backend
    /// test in the app would inherit the problem.
    #[test]
    fn an_uninstalled_host_answers_everything_without_a_backend_noticing() {
        let host = host();
        host.listings().directory_changed(
            "test://volume-host/detached",
            std::path::Path::new("/nowhere"),
            cmdr_fs::volume::DirectoryChange::FullRefresh,
        );
        assert!(
            host.listings()
                .authoritative_listing("test://volume-host/detached", std::path::Path::new("/nowhere"))
                .is_none()
        );
        assert!(
            host.credentials()
                .credentials("volume-host-detached.local", None)
                .is_none()
        );
        assert!(
            host.activity()
                .volume_idle_for("test://volume-host/detached", std::time::Duration::from_millis(1))
        );
        assert!(host.settings().max_concurrent_operations("ftp") >= 1);
    }
}
