//! The writer's low-disk-space check: while the host says the disk the index lives
//! on is below the user's low-disk-space threshold, the writer stops and the volume
//! pauses (`lifecycle/state/space_pause.rs`), rather than writing until SQLite
//! reports `SQLITE_FULL` with the user's last free gigabytes gone.
//!
//! In the writer, not in each bulk run, because the writer is the one door every
//! write goes through: a replay, a scan, a cover walk, and the live loop all stop
//! the same way, at a message boundary, with whatever chunk was open rolled back.
//! A replay resumes from its last committed chunk (`watch/DETAILS.md` § "A replay
//! commits in chunks"); anything else resumes the way it would after a crash.

use std::time::{Duration, Instant};

/// How often a busy writer asks the host. The host's reading refreshes every ~2 s,
/// so asking per message would only re-read the same answer, and per second keeps a
/// slow leak from going much past the line.
pub(super) const CHECK_EVERY: Duration = Duration::from_secs(1);

/// Rate-limits the writer's question to the host, per [`CHECK_EVERY`].
pub(super) struct SpaceGuard {
    last_checked: Option<Instant>,
}

impl SpaceGuard {
    pub(super) fn new() -> Self {
        Self { last_checked: None }
    }

    /// Whether the index's disk is low, asking the host at most once per
    /// [`CHECK_EVERY`]. Between asks it answers `false`: the next ask comes soon
    /// enough, and a writer that stops a second later loses nothing.
    pub(super) fn storage_low(&mut self, now: Instant) -> bool {
        if self
            .last_checked
            .is_some_and(|at| now.saturating_duration_since(at) < CHECK_EVERY)
        {
            return false;
        }
        self.last_checked = Some(now);
        crate::indexing::host::policy::current().index_storage_low()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::indexing::host::policy::{FakeHostPolicy, install_for_test};

    #[test]
    fn the_host_is_asked_at_most_once_per_interval() {
        let _serialized = crate::indexing::handle::test_lock();
        let host = FakeHostPolicy::shared();
        let _installed = install_for_test(Arc::clone(&host) as Arc<dyn crate::indexing::host::policy::HostPolicy>);
        let mut guard = SpaceGuard::new();
        let start = Instant::now();

        assert!(!guard.storage_low(start));
        host.note_index_storage_low(true);
        assert!(
            !guard.storage_low(start + Duration::from_millis(500)),
            "not asked again inside the interval"
        );
        assert!(guard.storage_low(start + CHECK_EVERY), "asked again once it elapsed");
    }
}
