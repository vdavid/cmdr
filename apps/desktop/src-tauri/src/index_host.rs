//! This app, acting as the index's host, and holding the handle it gets back.
//!
//! The mirror of `indexing/host/`: that directory declares everything the three
//! index subsystems need from the application around them, and this is the one
//! place that answers, at startup, before anything can start background work.
//!
//! Keeping it in one function rather than scattered through `setup()` is the
//! point. "What does the app owe the index?" has a single readable answer, and
//! adding a seam over there means adding a line here, not hunting for the right
//! spot in a 300-line closure.

use std::sync::{Arc, OnceLock};

use tauri::AppHandle;

use cmdr_index::Index;

/// The handle [`install`] built. Everything app-side reaches the index through
/// [`index`], never by calling into `indexing::` internals.
static INDEX: OnceLock<Index> = OnceLock::new();

/// Wire the index subsystems to this app and keep the handle: runtime, event
/// sink, config, volumes, priority policy, and the master switch.
///
/// Call once, at the very top of `setup()`. Every seam is best-effort and logs
/// rather than failing the launch: a missing data dir leaves the index
/// unconfigured rather than pointing it at a relative path.
pub fn install(app: &AppHandle) {
    let settings = crate::settings::load_settings(app);
    // The runtime first: everything below can start background work, and the
    // subsystems spawn through their own seam so they can be extracted into a
    // Tauri-free crate. Sharing OUR runtime rather than building their own is what
    // keeps a single thread pool, and with it the QoS story that lets indexing run
    // in-process at all.
    let mut builder = Index::builder()
        .runtime(tauri::async_runtime::handle().inner().clone())
        // Where the index reports. The typed `IndexEvent` becomes a Tauri payload
        // in `events/index_mapping.rs` and nowhere else, which is what keeps every
        // user-facing word app-side.
        .events(Arc::new(crate::events::index_mapping::TauriEventSink::new(app.clone())))
        // Which volumes exist. The index never touches `VolumeManager`, the
        // platform mount probes, or the MTP session layer directly — those are the
        // app's, and they can't follow the index into its own crate.
        .volumes(Arc::new(crate::file_system::index_provider::AppVolumeProvider))
        // Whose priority signals background work yields to. The ORDER (interactive
        // > transfers > indexing) is a product decision and stays in `priority/`.
        .host(Arc::new(crate::priority::host_policy::AppHostPolicy))
        .indexing_enabled(settings.indexing_enabled);

    // What the index is configured to do. It never reads a settings file or
    // resolves the data dir for itself: policy belongs to the product, and this is
    // the one place that turns stored settings into what the index acts on. The
    // media-policy IPC setters re-apply their own fields as the user changes them.
    match crate::config::resolved_app_data_dir(app) {
        Ok(data_dir) => {
            builder = builder.config(crate::commands::media_index::index_config_from(data_dir, &settings));
        }
        Err(e) => log::warn!(target: "indexing", "index not configured (no data dir): {e}"),
    }

    // A failure here means something read `index()` before this ran, so the app is
    // holding an index with no volumes, no event sink, and no data directory —
    // silent, and every drive would just never index. Worth reporting, not warning.
    match builder.build() {
        Ok(built) => {
            if INDEX.set(built).is_err() {
                crate::log_error!(target: "indexing", "the index handle was read before install(); it has no host wired");
            }
        }
        Err(e) => crate::log_error!(target: "indexing", "the index was built before install(): {e}"),
    }
}

/// The user default that turns the phased first index off, read once per launch.
///
/// The escape hatch for the phased first index (`cmdr_index`'s
/// `lifecycle/phases/DETAILS.md` § "The escape hatch"). Flip it with
///
/// ```text
/// defaults write com.veszelovszki.cmdr PhasedFirstIndex -bool false
/// ```
///
/// and relaunch; `defaults delete com.veszelovszki.cmdr PhasedFirstIndex` puts it
/// back. It's a user default rather than a settings key on purpose: it's for a bad
/// week, so it has to be one line somebody can paste into Terminal without opening
/// a JSON file or launching the app, and it has to be flippable while the thing
/// it's guarding against is happening. Absent means on.
static PHASED_FIRST_INDEX: OnceLock<bool> = OnceLock::new();

/// Whether this launch covers a drive's first index in phases.
///
/// Read ONCE and cached: every media-policy setter re-pushes `IndexConfig`, and a
/// value that could change between those pushes would mean a volume half way
/// through being covered suddenly gets the other behavior.
pub(crate) fn phased_first_index() -> bool {
    *PHASED_FIRST_INDEX.get_or_init(read_phased_first_index_default)
}

#[cfg(target_os = "macos")]
fn read_phased_first_index_default() -> bool {
    use objc2_foundation::{NSString, NSUserDefaults};

    // `boolForKey:` can't tell "absent" from "false", so ask for the OBJECT and
    // treat only an explicit `false` as off. An absent key is the shipping default.
    let key = NSString::from_str("PhasedFirstIndex");
    let defaults = NSUserDefaults::standardUserDefaults();
    if defaults.objectForKey(&key).is_none() {
        return true;
    }
    let enabled = defaults.boolForKey(&key);
    if !enabled {
        log::warn!(
            target: "indexing",
            "PhasedFirstIndex is off in user defaults: a drive with no completed scan gets the bulk first scan",
        );
    }
    enabled
}

/// Nothing to read: the escape hatch is a macOS user default, and macOS is where
/// the beta this protects is running.
#[cfg(not(target_os = "macos"))]
fn read_phased_first_index_default() -> bool {
    true
}

/// This app's index.
///
/// Built by [`install`] at the top of `setup()`. A test binary, a bench, or a
/// tool never calls `install`, so the first read here builds a handle with no
/// host wired: every seam degrades (nothing mounted, nothing competing, events
/// dropped), which is exactly what those callers want.
///
/// ❌ In the app, nothing may read this before `install`. It would win the race,
/// and the app would run against an index that can never see a volume. `install`
/// reports it if it ever happens.
pub fn index() -> &'static Index {
    INDEX.get_or_init(|| {
        Index::builder()
            .build()
            .expect("nothing builds the index before `index_host::install`")
    })
}

/// What a storage backend owes the file index, answered from the handle above.
///
/// The other direction from [`install`]: there the app answers the index, here it
/// answers a BACKEND that must not depend on the index at all. A backend crate
/// could import `cmdr_index` (both are Tauri-free), and that's exactly what the
/// seam exists to stop — it would put a quarter of the codebase inside
/// `cargo check -p cmdr-ftp` for two method calls. So the backend speaks the
/// seam's own vocabulary and this maps it.
pub struct VolumeIndexNotifier;

impl cmdr_fs::volume::host::indexing::IndexNotifier for VolumeIndexNotifier {
    fn watch_gap(&self, volume_id: &str, gap: cmdr_fs::volume::host::indexing::WatchGap) {
        use cmdr_fs::volume::host::indexing::WatchGap as SeamGap;
        use cmdr_index::{WatchGap, WatchScope};

        // A volume backend reports for its own volume; `WatchScope::Device` is the
        // transport layer's shape (one MTP session carrying several volumes) and
        // stays app-side with the transport that has one.
        let reason = match gap {
            SeamGap::WatcherStopped => WatchGap::WatcherStopped,
            SeamGap::EventsOverflowed => WatchGap::EventsOverflowed,
            SeamGap::ConnectionReset => WatchGap::ConnectionReset,
        };
        index().on_watch_gap(WatchScope::Volume(volume_id), reason);
    }

    fn resume_after_reconnect(&self, volume_id: &str) {
        index().resume_after_reconnect(volume_id.to_string());
    }

    fn device_object_changed(&self, device_id: &str, handle: u32) {
        index().on_device_object_changed(device_id, handle);
    }

    fn device_object_removed(&self, device_id: &str, handle: u32) {
        index().on_device_object_removed(device_id, handle);
    }
}

#[cfg(test)]
mod tests {
    use cmdr_fs::volume::host::indexing::{IndexNotifier, WatchGap};

    use super::VolumeIndexNotifier;

    /// The seam promises a backend it can report every watcher exit blindly,
    /// including for a volume nobody indexed. If that weren't free, a backend
    /// would start deciding whether a gap is worth reporting, and the ones it
    /// decided against are exactly the ones that leave an index looking fresh
    /// forever.
    #[test]
    fn reporting_a_gap_for_an_unindexed_volume_costs_nothing() {
        let volume_id = "test://index-notifier/never-indexed";

        VolumeIndexNotifier.watch_gap(volume_id, WatchGap::WatcherStopped);
        VolumeIndexNotifier.watch_gap(volume_id, WatchGap::EventsOverflowed);
        VolumeIndexNotifier.watch_gap(volume_id, WatchGap::ConnectionReset);
        VolumeIndexNotifier.resume_after_reconnect(volume_id);
    }

    /// A device backend reports every object event blindly, by bare handle, for a
    /// device nothing has ever indexed. If that cost anything the backend would
    /// start filtering, and the events it filtered out are the ones that leave a
    /// storage's index quietly wrong.
    #[test]
    fn reporting_object_changes_for_an_unindexed_device_costs_nothing() {
        let device_id = "mtp-index-notifier-never-indexed";

        VolumeIndexNotifier.device_object_changed(device_id, 1);
        VolumeIndexNotifier.device_object_removed(device_id, 1);
    }
}
