//! A test's way to see what a platform without file ids sees: the paths it names read as
//! [`Sameness::Unknown`](super::Sameness::Unknown) on every check, while the rest of the disk
//! (and every other test in the process) keeps its real ids. So the cautious branches every
//! caller takes on `Unknown` run on Linux CI, not first on Windows.

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{FileId, platform};
use crate::ignore_poison::IgnorePoison;

static HIDDEN: Mutex<Vec<FileId>> = Mutex::new(Vec::new());
/// How many ids are hidden, so the check costs one relaxed load while nothing is.
static COUNT: AtomicUsize = AtomicUsize::new(0);

pub(super) fn hides(id: FileId) -> bool {
    COUNT.load(Ordering::Relaxed) != 0 && HIDDEN.lock_ignore_poison().contains(&id)
}

/// While held, the nodes at `paths` (the links themselves and what they point at) carry no
/// file id, device, or link count. Dropping it gives them back.
#[must_use = "the identities come back when the guard drops"]
pub struct HiddenIdentities(Vec<FileId>);

/// Hides the identities of `paths`, which must exist. See [`HiddenIdentities`].
pub fn hide_identity_of(paths: &[&Path]) -> HiddenIdentities {
    let mut ids = Vec::new();
    for path in paths {
        for metadata in [std::fs::symlink_metadata(path), std::fs::metadata(path)]
            .into_iter()
            .flatten()
        {
            if let Some(id) = platform::file_id(&metadata) {
                ids.push(id);
            }
        }
    }
    assert!(!ids.is_empty(), "nothing to hide: the paths must exist");
    let mut hidden = HIDDEN.lock_ignore_poison();
    hidden.extend(&ids);
    COUNT.store(hidden.len(), Ordering::Relaxed);
    HiddenIdentities(ids)
}

impl Drop for HiddenIdentities {
    fn drop(&mut self) {
        let mut hidden = HIDDEN.lock_ignore_poison();
        for id in &self.0 {
            if let Some(at) = hidden.iter().position(|held| held == id) {
                hidden.swap_remove(at);
            }
        }
        COUNT.store(hidden.len(), Ordering::Relaxed);
    }
}
