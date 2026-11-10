//! How this copy of Cmdr was installed: the heartbeat's `installChannel`.
//!
//! Probed once per process, off the async runtime's worker threads, and cached. An answer we can't
//! get is `None` (`null` on the wire), never an error. Why there's no download date beside it:
//! `DETAILS.md` § "Install channel".

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::OnceCell;

/// Our cask's token in `vdavid/tap` (`apps/desktop/packaging/homebrew/cmdr.rb`). A tap cask lives
/// under `Caskroom/<token>` like an official one, without the tap prefix.
const CASK_TOKEN: &str = "cmdr";

/// Homebrew's default prefixes: Apple silicon, then Intel.
const HOMEBREW_PREFIXES: [&str; 2] = ["/opt/homebrew", "/usr/local"];

/// The probe reads a few local paths, but the app may run from a network mount, which can hang.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// How this copy of Cmdr was installed. Lowercase on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum InstallChannel {
    /// Through our Homebrew cask.
    Homebrew,
    /// From a `.app` bundle that no Homebrew cask put there: the DMG from the site, in practice.
    Direct,
}

static CHANNEL: OnceCell<Option<InstallChannel>> = OnceCell::const_new();

/// The install channel, probed on first call and cached for the process lifetime. `None` when we
/// can't tell: a dev build, no `.app` bundle around the binary, not macOS, or a probe that timed out.
pub(super) async fn get() -> Option<InstallChannel> {
    *CHANNEL
        .get_or_init(|| async {
            // A dev build runs from `target/`, so the answer wouldn't mean anything.
            if cfg!(debug_assertions) || !cfg!(target_os = "macos") {
                return None;
            }
            cmdr_fs::deadline::blocking_with_timeout(PROBE_TIMEOUT, None, probe).await
        })
        .await
}

/// The `.app` bundle the running binary sits in. The updater owns that lookup, and it's macOS-only.
#[cfg(target_os = "macos")]
fn running_bundle() -> Option<PathBuf> {
    crate::updater::installer::running_bundle().ok()
}

/// No `.app` bundles off macOS, so the channel is unknown there (`get` short-circuits before this anyway).
#[cfg(not(target_os = "macos"))]
fn running_bundle() -> Option<PathBuf> {
    None
}

fn probe() -> Option<InstallChannel> {
    let bundle = running_bundle();
    let caskroom_dirs: Vec<PathBuf> = HOMEBREW_PREFIXES
        .iter()
        .map(|prefix| Path::new(prefix).join("Caskroom"))
        .collect();
    channel_for(bundle.as_deref(), &caskroom_dirs)
}

/// `Homebrew` when any of `caskroom_dirs` holds our cask, `Direct` for any other bundle, and `None`
/// when the binary isn't in a `.app` bundle at all. The cask's `Caskroom/cmdr` dir stays for as long
/// as the cask is installed, self-updates included, and `brew uninstall` removes it.
fn channel_for(bundle: Option<&Path>, caskroom_dirs: &[PathBuf]) -> Option<InstallChannel> {
    bundle?;
    if caskroom_dirs.iter().any(|dir| dir.join(CASK_TOKEN).is_dir()) {
        Some(InstallChannel::Homebrew)
    } else {
        Some(InstallChannel::Direct)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestDir;

    #[test]
    fn no_bundle_means_we_cant_tell_the_channel() {
        let dir = TestDir::new("install-channel-no-bundle");
        std::fs::create_dir_all(dir.join("Caskroom").join(CASK_TOKEN)).expect("create caskroom");
        assert_eq!(channel_for(None, &[dir.join("Caskroom")]), None);
    }

    #[test]
    fn our_cask_in_any_caskroom_means_homebrew() {
        let dir = TestDir::new("install-channel-homebrew");
        let intel = dir.join("usr-local").join("Caskroom");
        std::fs::create_dir_all(intel.join(CASK_TOKEN)).expect("create caskroom");
        let caskrooms = [dir.join("opt-homebrew").join("Caskroom"), intel];
        assert_eq!(
            channel_for(Some(Path::new("/Applications/Cmdr.app")), &caskrooms),
            Some(InstallChannel::Homebrew)
        );
    }

    #[test]
    fn a_bundle_with_other_casks_only_is_direct() {
        let dir = TestDir::new("install-channel-other-casks");
        let caskroom = dir.join("Caskroom");
        std::fs::create_dir_all(caskroom.join("ghostty")).expect("create caskroom");
        assert_eq!(
            channel_for(Some(Path::new("/Applications/Cmdr.app")), &[caskroom]),
            Some(InstallChannel::Direct)
        );
    }

    #[test]
    fn a_bundle_with_no_homebrew_at_all_is_direct() {
        let dir = TestDir::new("install-channel-no-homebrew");
        assert_eq!(
            channel_for(Some(Path::new("/Applications/Cmdr.app")), &[dir.join("Caskroom")]),
            Some(InstallChannel::Direct)
        );
    }

    #[test]
    fn a_file_named_like_our_cask_isnt_the_cask() {
        let dir = TestDir::new("install-channel-cask-file");
        let caskroom = dir.join("Caskroom");
        std::fs::create_dir_all(&caskroom).expect("create caskroom");
        std::fs::write(caskroom.join(CASK_TOKEN), "").expect("write file");
        assert_eq!(
            channel_for(Some(Path::new("/Applications/Cmdr.app")), &[caskroom]),
            Some(InstallChannel::Direct)
        );
    }

    #[test]
    fn channels_serialize_lowercase() {
        assert_eq!(
            serde_json::to_value(InstallChannel::Homebrew).expect("serialize"),
            "homebrew"
        );
        assert_eq!(
            serde_json::to_value(InstallChannel::Direct).expect("serialize"),
            "direct"
        );
    }

    #[tokio::test]
    async fn a_dev_build_reports_no_channel() {
        assert_eq!(get().await, None);
    }
}
