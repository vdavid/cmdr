//! This app, acting as the index's host.
//!
//! The mirror of `indexing/host/`: that directory declares everything the three
//! index subsystems need from the application around them, and this is the one
//! place that answers, at startup, before anything can start background work.
//!
//! Keeping it in one function rather than scattered through `setup()` is the
//! point. "What does the app owe the index?" has a single readable answer, and
//! adding a seam over there means adding a line here, not hunting for the right
//! spot in a 300-line closure.

use std::sync::Arc;

use tauri::AppHandle;

use crate::indexing;

/// Wire the index subsystems to this app: runtime, event sink, config, volumes,
/// and priority policy.
///
/// Call once, at the very top of `setup()`. Every seam is best-effort and logs
/// rather than failing the launch: a second call keeps the first value (the index
/// won't swap a runtime or a registry under work that's already running), and a
/// missing data dir leaves the index unconfigured rather than pointing it at a
/// relative path.
pub fn install(app: &AppHandle) {
    // The runtime first: everything below can start background work, and the
    // subsystems spawn through their own seam so they can be extracted into a
    // Tauri-free crate. Sharing OUR runtime rather than building their own is what
    // keeps a single thread pool, and with it the QoS story that lets indexing run
    // in-process at all.
    if indexing::host::runtime::set_runtime(tauri::async_runtime::handle().inner().clone()).is_err() {
        log::warn!(target: "indexing", "index runtime was already set; keeping the first one");
    }

    // Where the index reports. The typed `IndexEvent` becomes a Tauri payload in
    // `events/index_mapping.rs` and nowhere else, which is what keeps every
    // user-facing word app-side.
    let sink = Arc::new(crate::events::index_mapping::TauriEventSink::new(app.clone()));
    if indexing::host::events::set_event_sink(sink).is_err() {
        log::warn!(target: "indexing", "index event sink was already set; keeping the first one");
    }

    // What the index is configured to do. It never reads a settings file or
    // resolves the data dir for itself: policy belongs to the product, and this is
    // the one place that turns stored settings into what the index acts on. The
    // media-policy IPC setters re-apply their own fields as the user changes them.
    match crate::config::resolved_app_data_dir(app) {
        Ok(data_dir) => indexing::host::config::set_config(crate::commands::media_index::index_config_from(
            data_dir,
            &crate::settings::load_settings(app),
        )),
        Err(e) => log::warn!(target: "indexing", "index not configured (no data dir): {e}"),
    }

    // Which volumes exist. The index never touches `VolumeManager`, the platform
    // mount probes, or the MTP session layer directly — those are the app's, and
    // they can't follow the index into its own crate.
    let volumes = Arc::new(crate::file_system::index_provider::AppVolumeProvider);
    if indexing::host::volumes::set_volume_provider(volumes).is_err() {
        log::warn!(target: "indexing", "index volume provider was already set; keeping the first one");
    }

    // Whose priority signals background work yields to. The ORDER (interactive >
    // transfers > indexing) is a product decision and stays in `priority/`.
    let policy = Arc::new(crate::priority::host_policy::AppHostPolicy);
    if indexing::host::policy::set_host_policy(policy).is_err() {
        log::warn!(target: "indexing", "index host policy was already set; keeping the first one");
    }
}
