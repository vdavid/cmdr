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
//! - host keys — `network::sftp_host_keys::AppHostKeys`
//! - indexing — `index_host::VolumeIndexNotifier`
//! - activity — `priority::host_policy::AppUserActivity`
//! - analytics — `analytics::volume_sink::PostHogVolumeAnalytics`
//! - settings — `file_system::backend_settings::AppBackendSettings`
//! - runtime — the app's own tokio handle, so there's one thread pool
//!
//! ## The host is a value
//!
//! `VolumeHost` is cheaply cloned (nine `Arc` bumps), and a backend takes one in
//! its constructor and keeps it. The `OnceLock` here is just where the app parks
//! the one it built, so a connect path reached from an IPC command can pick it
//! up; ❌ it is NOT how a backend finds its host. A test that wants FAKES builds
//! its own and passes it in, which is why nothing here needs an
//! install-and-restore guard and no test has to be serialized against the rest of
//! the binary.
//!
//! ## Only the frontend channel needs the app to be running
//!
//! Every adapter but the event sink answers from something a test binary has too:
//! the listing cache, the secret store, the trusted-host-key store, the index
//! handle, the priority tracker, the analytics client, and the settings module
//! are all process-global and work before `setup()` ever runs. So [`host`] hands out the REAL wiring even without
//! an `AppHandle` and leaves only the frontend event channel silent, which is
//! what lets an app-side backend test assert on the real listing cache without
//! standing a Tauri app up.

use std::sync::{Arc, OnceLock};

use tauri::AppHandle;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::events::VolumeEventSink;

/// The host [`install`] built. Backend construction paths reach it through
/// [`host`].
static HOST: OnceLock<VolumeHost> = OnceLock::new();

/// Wires every seam but the frontend event channel, which is the one answer that
/// needs a running app.
fn wire(events: Option<Arc<dyn VolumeEventSink>>) -> VolumeHost {
    let builder = VolumeHost::builder()
        .listings(Arc::new(crate::file_system::listing::listing_host::AppListings))
        .credentials(Arc::new(crate::network::credential_store::KeychainCredentials))
        .host_keys(Arc::new(crate::network::sftp_host_keys::AppHostKeys))
        // Mapped rather than passed through: a backend crate must not depend on
        // the index, or `cargo check` on it would compile a quarter of the app.
        .indexing(Arc::new(crate::index_host::VolumeIndexNotifier))
        .activity(Arc::new(crate::priority::host_policy::AppUserActivity))
        .analytics(Arc::new(crate::analytics::volume_sink::PostHogVolumeAnalytics))
        .settings(Arc::new(crate::file_system::backend_settings::AppBackendSettings));
    match events {
        Some(events) => builder
            // The app's own runtime, so background work a backend starts shares
            // one thread pool with everything else. Backends need this because
            // their watchers run on OS threads with no reactor, where
            // `tokio::spawn` panics.
            .runtime(tauri::async_runtime::handle().inner().clone())
            .events(events)
            .build(),
        // No `runtime` either: without an app there's no pool to join, and the
        // seam builds its own shared fallback on first use.
        None => builder.build(),
    }
}

/// Wire every seam a backend can ask about to this app: runtime, listings,
/// events, credentials, trusted host keys, index notifications, activity,
/// analytics, and settings.
///
/// Call once, in `setup()`, before anything constructs a volume. Nothing here can
/// fail: a seam that isn't installed answers with a no-op rather than an error,
/// so a late call costs pane updates, not a launch.
pub fn install(app: &AppHandle) {
    // The typed connection transition becomes a Tauri payload in
    // `events/volume_mapping.rs` and nowhere else, which is what keeps every
    // user-facing word app-side.
    let host = wire(Some(Arc::new(crate::events::volume_mapping::TauriVolumeEvents::new(
        app.clone(),
    ))));

    if HOST.set(host).is_err() {
        log::warn!(target: "volume", "the volume host was already installed; keeping the first one");
    }
}

/// The host to hand a backend being constructed.
///
/// Before [`install`] — a test binary, a bench, a tool — this is the same wiring
/// minus the frontend event channel and the app's runtime. Nothing fails and
/// nothing is an `Option`: a backend built here still updates the real panes and
/// reads the real settings, it just has no frontend to tell about a session
/// coming and going.
pub fn host() -> VolumeHost {
    HOST.get().cloned().unwrap_or_else(|| wire(None))
}

#[cfg(test)]
mod tests {
    use cmdr_fs::volume::host::settings::BackendSettings;

    use super::*;

    /// Nothing installs a host in a test binary, so every backend built there
    /// gets one without a frontend behind it. If this ever panicked or hung,
    /// every backend test in the app would inherit the problem.
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

    /// The reason the fallback isn't `VolumeHost::detached()`: an app-side
    /// backend test drives a real volume and then asserts on the real listing
    /// cache. A detached fallback would leave every one of those assertions
    /// looking at a cache nothing ever wrote to.
    ///
    /// Pinned on the one answer the two wirings give differently: the app's table
    /// hands an unregistered namespace its own cautious 2, while the seam's
    /// built-in default is 4.
    #[test]
    fn a_backend_built_without_an_app_still_reaches_the_real_settings_table() {
        assert_eq!(
            host().settings().max_concurrent_operations("a-namespace-nobody-wired"),
            crate::file_system::backend_settings::AppBackendSettings
                .max_concurrent_operations("a-namespace-nobody-wired"),
            "the uninstalled host has to be the app's real wiring, not a no-op stand-in"
        );
        assert_ne!(
            host().settings().max_concurrent_operations("a-namespace-nobody-wired"),
            VolumeHost::detached()
                .settings()
                .max_concurrent_operations("a-namespace-nobody-wired"),
        );
    }
}
