//! Where the user is looking, as the machine hears about it.
//!
//! No new seam: `HostPolicy::open_listings()` already reports every directory a
//! pane is showing, because mid-scan aggregation needs the same answer. The
//! machine polls it on the progress reporter's 500 ms tick — which is the seam's
//! own contract, "❌ not from anything faster" — and keeps a small recently-seen
//! set, so a folder somebody opened and left still gets its turn.
//!
//! ❌ Not `Index::verify_directory`, which fires for the opposite pane, for MCP
//! listings, and for every refresh: too loose a signal to reorder a drive walk by.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use cmdr_fs::ignore_poison::IgnorePoison;

use crate::indexing::host::policy::OpenListing;

/// How many recently-opened folders are worth remembering.
///
/// Small on purpose: this is "where is the user now", not a history. Past a dozen
/// the oldest entries describe somewhere they have already left, and the phases
/// would reach them about as fast anyway.
const REMEMBERED: usize = 12;

/// The folders the user has had open, most recent first.
pub(crate) struct VisitLog {
    seen: std::sync::Mutex<VecDeque<PathBuf>>,
}

impl VisitLog {
    pub(crate) fn new() -> Self {
        Self {
            seen: std::sync::Mutex::new(VecDeque::new()),
        }
    }

    /// Fold one poll's answer in, keeping only this volume's listings.
    ///
    /// A directory already remembered moves back to the front rather than
    /// doubling: a pane sitting on one folder answers the same thing every tick.
    pub(crate) fn note(&self, listings: &[OpenListing], volume_id: &str) {
        let mut seen = self.seen.lock_ignore_poison();
        for listing in listings.iter().filter(|listing| listing.volume_id == volume_id) {
            if let Some(index) = seen.iter().position(|path| *path == listing.path) {
                seen.remove(index);
            }
            seen.push_front(listing.path.clone());
        }
        seen.truncate(REMEMBERED);
    }

    /// The folder to cover next, most recently opened first.
    pub(crate) fn take(&self) -> Option<PathBuf> {
        self.seen.lock_ignore_poison().pop_front()
    }

    /// Whether any remembered folder still has its turn coming, left where it is.
    ///
    /// What a walk in flight asks to find out whether stopping would buy the user
    /// anything: the machine can't run the interlude until the walk it is in has
    /// ended, so the decision to stop and the decision to take are two separate
    /// moments. ⚠️ Every remembered folder, ❌ never only the front one: both
    /// panes report every tick, so a pane parked on a folder that has already had
    /// its turn sits in front of the folder somebody just opened.
    pub(crate) fn any_waiting(&self, already_done: impl Fn(&Path) -> bool) -> bool {
        self.seen.lock_ignore_poison().iter().any(|path| !already_done(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(volume_id: &str, path: &str) -> OpenListing {
        OpenListing {
            volume_id: volume_id.to_string(),
            path: PathBuf::from(path),
        }
    }

    /// Most recent first, one entry per folder however many ticks it was open for,
    /// and never another volume's pane.
    #[test]
    fn the_most_recently_opened_folder_is_taken_first() {
        let log = VisitLog::new();
        log.note(&[listing("root", "/a"), listing("smb-nas", "/elsewhere")], "root");
        log.note(&[listing("root", "/b")], "root");
        log.note(&[listing("root", "/a")], "root");

        assert_eq!(log.take(), Some(PathBuf::from("/a")));
        assert_eq!(log.take(), Some(PathBuf::from("/b")));
        assert_eq!(log.take(), None, "and a pane on another drive was never ours");
    }

    /// Two panes report every tick, so a pane parked on a folder that has already
    /// had its turn sits in front of the one somebody just opened. Asked of the
    /// front folder alone, the machine would never stop a walk for the second pane.
    #[test]
    fn a_folder_behind_one_that_had_its_turn_is_still_waiting() {
        let log = VisitLog::new();
        log.note(&[listing("root", "/opened")], "root");
        log.note(&[listing("root", "/parked")], "root");

        assert!(
            log.any_waiting(|path| path == Path::new("/parked")),
            "the folder behind the parked pane hasn't had its turn"
        );
        assert!(!log.any_waiting(|_| true), "and once both have, nobody is waiting");
    }

    /// A pane left open for an hour can't grow the set: the log is where the user
    /// IS, not where they have been.
    #[test]
    fn the_log_stays_small() {
        let log = VisitLog::new();
        for n in 0..(REMEMBERED * 3) {
            log.note(&[listing("root", &format!("/dir-{n}"))], "root");
        }
        assert_eq!(log.seen.lock_ignore_poison().len(), REMEMBERED);
    }
}
