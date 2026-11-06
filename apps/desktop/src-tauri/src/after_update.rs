//! Whether this run is the first of a newly installed version, for the two things that care.
//!
//! - macOS's Local Network permission can stay pinned to the binary an update replaced, so the
//!   first launch after an in-place update re-announces the bundle to LaunchServices
//!   (`updater::local_network_refresh`, #1019).
//! - The share notices (`network::os_mount_notice`) speak up about a dial this Mac blocked on that
//!   launch, even from the quiet launch sweep: right after an update, that block is almost always
//!   the stuck permission, and the person needs the switch to fix it.
//!
//! The answer comes from a stamp in the app data dir (`last-launched-version.json`), rewritten
//! whenever the running version differs. A missing stamp is a first run of this feature, not
//! necessarily a first install, so it counts as "after an update" when the install already
//! scheduled update checks (`update_schedule::has_record`): the release that ships this is itself
//! installed by an update, and its first launch is exactly the one that needs the fix.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::OnceLock;
use tauri::{AppHandle, Runtime};

const STAMP_FILE_NAME: &str = "last-launched-version.json";

/// How this launch relates to the last one, by app version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LaunchKind {
    /// The version that ran last time.
    SameVersion,
    /// A different version than last time: an update (or, rarely, a downgrade by hand).
    FirstAfterUpdate,
    /// No sign of an earlier run on this data dir.
    FirstEver,
}

#[derive(Debug, Serialize, Deserialize)]
struct Stamp {
    version: String,
}

static LAUNCH_KIND: OnceLock<LaunchKind> = OnceLock::new();

/// Works out this launch's [`LaunchKind`] once, from `setup()`, and stamps the running version.
/// Reads and writes one small file in the app data dir.
pub(crate) fn init<R: Runtime>(app: &AppHandle<R>) -> LaunchKind {
    let kind = match crate::config::resolved_app_data_dir(app) {
        Ok(data_dir) => classify_and_stamp(&data_dir, env!("CARGO_PKG_VERSION")),
        Err(e) => {
            log::debug!("No app data dir to tell an update from a relaunch: {e}");
            LaunchKind::SameVersion
        }
    };
    if LAUNCH_KIND.set(kind).is_err() {
        log::debug!("The launch kind was already set; keeping the first answer");
    }
    kind
}

/// Whether this run is the first one after the app's version changed. `false` before [`init`].
pub(crate) fn is_first_run_after_update() -> bool {
    LAUNCH_KIND.get() == Some(&LaunchKind::FirstAfterUpdate)
}

fn classify_and_stamp(data_dir: &Path, running: &str) -> LaunchKind {
    let path = data_dir.join(STAMP_FILE_NAME);
    let stamped = std::fs::read_to_string(&path)
        .ok()
        .and_then(|contents| serde_json::from_str::<Stamp>(&contents).ok())
        .map(|stamp| stamp.version);
    let kind = classify(
        stamped.as_deref(),
        crate::update_schedule::has_record(data_dir),
        running,
    );
    if kind != LaunchKind::SameVersion {
        log::info!(
            "First launch of {running} (last launched: {}): {kind:?}",
            stamped.as_deref().unwrap_or("unknown")
        );
        write_stamp(&path, running);
    }
    kind
}

/// `stamped` is the version the stamp names, `ran_before` whether this data dir shows an earlier
/// run anyway (an update check on record).
fn classify(stamped: Option<&str>, ran_before: bool, running: &str) -> LaunchKind {
    match stamped {
        Some(version) if version == running => LaunchKind::SameVersion,
        Some(_) => LaunchKind::FirstAfterUpdate,
        None if ran_before => LaunchKind::FirstAfterUpdate,
        None => LaunchKind::FirstEver,
    }
}

fn write_stamp(path: &Path, running: &str) {
    let Ok(content) = serde_json::to_string_pretty(&Stamp {
        version: running.to_string(),
    }) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if let Err(e) = crate::config::durable_write_json(path, &tmp, &content) {
        log::warn!("Couldn't remember the launched version: {e}");
    }
}

#[cfg(test)]
#[path = "after_update_test.rs"]
mod tests;
