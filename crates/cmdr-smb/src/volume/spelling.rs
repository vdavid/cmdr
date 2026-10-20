//! Finding the server's own spelling of a path that didn't come from the server.
//!
//! An SMB name is opaque bytes the server matches exactly, so a path typed by
//! the user, restored from a saved tab, or carried over from the macOS kernel
//! mount (which decomposes every name) misses whenever it spells an accented or
//! differently cased component another way than the server stores it. No single
//! normalization of the whole path fixes that: one directory can hold its own
//! name composed and its children's decomposed. So the fix is per COMPONENT,
//! against a real listing: find the first component that doesn't open, list its
//! parent, match under Unicode-form-and-case folding, take the server's bytes,
//! and carry on down. `DETAILS.md` § "SMB names are opaque bytes".
//!
//! Reached only through [`Volume::find_stored_spelling`](cmdr_fs::volume::Volume::find_stored_spelling),
//! at the seams where a foreign path enters. ❌ Never from inside an operation:
//! every other call on this backend means the exact bytes it's handed.

use super::SmbVolume;
use super::session::SessionLease;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::VolumeError;
use log::debug;
use smb2::types::status::NtStatus;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// The one comparison key for "the same name, spelled another way", shared with
/// the rest of Cmdr. ❌ Never sent to the server.
pub(super) use cmdr_fs::name_fold::fold_name as fold;

/// How one wanted component stands against its parent's entries.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ComponentMatch {
    /// An entry carries the wanted bytes exactly.
    Exact,
    /// Exactly one entry matches under folding, and this is its stored name.
    Folded(String),
    /// Nothing matches, folded or not.
    Absent,
    /// Two or more entries match under folding and none exactly.
    Ambiguous,
}

/// Matches `wanted` against a directory's entry `names`.
///
/// An exact byte match wins outright, even beside folded look-alikes: that is
/// the entry the caller named. Otherwise one folded match is the answer, and
/// more than one is refused rather than guessed.
pub(super) fn match_component<'a>(names: impl IntoIterator<Item = &'a str>, wanted: &str) -> ComponentMatch {
    let key = fold(wanted);
    let mut found: Option<&str> = None;
    let mut ambiguous = false;
    for name in names {
        if name == wanted {
            return ComponentMatch::Exact;
        }
        if fold(name) == key {
            ambiguous |= found.is_some();
            found = Some(name);
        }
    }
    match (found, ambiguous) {
        (_, true) => ComponentMatch::Ambiguous,
        (Some(name), false) => ComponentMatch::Folded(name.to_string()),
        (None, false) => ComponentMatch::Absent,
    }
}

/// How many corrections the share remembers before the oldest goes.
///
/// A correction is only learned from a foreign path that missed, and every path
/// derived from a corrected directory is already exact, so a session sees a
/// handful. The cap is what keeps a long session from growing it without bound.
const REMEMBERED_CORRECTIONS: usize = 256;

/// Corrections this share has learned: in directory `parent` (share-relative,
/// the server's own bytes), the foreign spelling `given` is stored as `stored`.
///
/// Saves the parent listing when the same foreign path comes back (a favorite
/// clicked again, a restored tab re-listed), which for a 58 000-entry directory
/// is seconds. A remembered correction is only ever a guess to try: it is
/// checked by the round trip that follows it, dropped when that misses, and
/// forgotten when the directory it lives in reports a change (`forget_dir`,
/// from the watcher), so it can't outlive the entry it names.
#[derive(Default)]
pub(super) struct SpellingCache {
    inner: Mutex<CacheInner>,
}

#[derive(Default)]
struct CacheInner {
    by_key: HashMap<(String, String), String>,
    /// Insertion order, oldest first, for the cap. May hold keys already
    /// forgotten; those are skipped when evicting.
    order: VecDeque<(String, String)>,
}

impl SpellingCache {
    pub(super) fn get(&self, parent: &str, given: &str) -> Option<String> {
        self.inner
            .lock_ignore_poison()
            .by_key
            .get(&(parent.to_string(), given.to_string()))
            .cloned()
    }

    pub(super) fn remember(&self, parent: &str, given: &str, stored: &str) {
        let mut inner = self.inner.lock_ignore_poison();
        let key = (parent.to_string(), given.to_string());
        if inner.by_key.insert(key.clone(), stored.to_string()).is_none() {
            inner.order.push_back(key);
        }
        while inner.by_key.len() > REMEMBERED_CORRECTIONS {
            let Some(oldest) = inner.order.pop_front() else { break };
            inner.by_key.remove(&oldest);
        }
        // Forgotten keys linger in `order`; don't let them pile up past the cap.
        if inner.order.len() > 2 * REMEMBERED_CORRECTIONS {
            let CacheInner { by_key, order } = &mut *inner;
            order.retain(|k| by_key.contains_key(k));
        }
    }

    fn forget(&self, parent: &str, given: &str) {
        self.inner
            .lock_ignore_poison()
            .by_key
            .remove(&(parent.to_string(), given.to_string()));
    }

    /// Drops every correction learned in `parent` (share-relative), which just
    /// reported a change: the entry a correction names may be gone or have a twin.
    pub(super) fn forget_dir(&self, parent: &str) {
        let mut inner = self.inner.lock_ignore_poison();
        if inner.by_key.is_empty() {
            return;
        }
        inner.by_key.retain(|(dir, _), _| dir != parent);
    }

    /// Drops everything: the server dropped change records, so any directory may
    /// have changed unseen.
    pub(super) fn forget_all(&self) {
        let mut inner = self.inner.lock_ignore_poison();
        inner.by_key.clear();
        inner.order.clear();
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.inner.lock_ignore_poison().by_key.len()
    }
}

/// Where a component's correction came from, which decides what a miss on it
/// means: a remembered one may be stale, a listed one raced a change.
#[derive(Clone, Copy)]
enum Fix {
    Remembered,
    Listed,
}

/// Where a path's first missing component is, as far as the server says.
enum Probe {
    /// The whole path opens as spelled.
    Opens,
    /// Component `index` doesn't open; everything above it does.
    MissingAt(usize),
}

impl SmbVolume {
    /// [`Volume::find_stored_spelling`](cmdr_fs::volume::Volume::find_stored_spelling)
    /// for this share. The module doc says how; this says the cost.
    ///
    /// Nothing when the path opens as given (one round trip, and callers only
    /// ask after a miss). Otherwise per wrong component: a round trip or two to
    /// find it (`STATUS_OBJECT_NAME_NOT_FOUND` names the leaf, `…_PATH_…` walks
    /// up), and its parent's listing, which is free when a pane shows that
    /// directory (the `authoritative_listing` oracle) or remembered from before.
    /// Checks `cancel` between round trips; one listing is a single call.
    pub(super) async fn find_stored_spelling_impl(
        &self,
        path: &Path,
        cancel: Option<&CancellationToken>,
    ) -> Result<Option<PathBuf>, VolumeError> {
        let smb_path = self.to_smb_path(path)?;
        let given: Vec<&str> = smb_path.split('/').filter(|c| !c.is_empty()).collect();
        // The anchor is where this mount sits, not a name anyone typed.
        let fixed = self.share_root.split('/').filter(|c| !c.is_empty()).count();
        if given.len() <= fixed {
            return Ok(None);
        }

        let mut comps: Vec<String> = given.iter().map(|c| (*c).to_string()).collect();
        // `comps[..floor]` is known to open; `comps[floor]` may be a correction
        // the next probe still has to confirm.
        let mut floor = fixed;
        // The component last corrected, and where the correction came from.
        let mut last_fix: Option<(usize, Fix)> = None;

        // Each pass moves the floor down or returns, and a component is corrected
        // at most twice (a remembered guess, then a listing), so this bound is never
        // the reason to stop.
        for _ in 0..=(2 * comps.len()) {
            let index = match self.probe(&comps, floor, cancel).await? {
                Probe::Opens => {
                    let corrected = last_fix.is_some();
                    return Ok(corrected.then(|| PathBuf::from(self.to_display_path(&comps.join("/")))));
                }
                Probe::MissingAt(index) => index,
            };
            let parent = comps[..index].join("/");
            let wanted = given[index];

            let consult_memory = match last_fix {
                // A remembered guess that doesn't open is stale: drop it and list.
                Some((at, Fix::Remembered)) if at == index => {
                    self.inner.spellings.forget(&parent, wanted);
                    false
                }
                // Listed, yet it didn't open: it went between the two round trips.
                // Nothing left to correct, so the caller's own answer stands.
                Some((at, Fix::Listed)) if at == index => return Ok(None),
                _ => true,
            };
            floor = index;
            if consult_memory && let Some(stored) = self.inner.spellings.get(&parent, wanted) {
                comps[index] = stored;
                last_fix = Some((index, Fix::Remembered));
                continue;
            }

            check_cancel(cancel)?;
            let names = self.entry_names(&parent).await?;
            match match_component(names.iter().map(String::as_str), wanted) {
                ComponentMatch::Folded(stored) => {
                    debug!(
                        "SmbVolume::find_stored_spelling(share={:?}): {:?} in {:?} is stored as {:?}",
                        LogPrivate::volume(&self.inner.share_name),
                        LogPrivate::name(wanted),
                        LogPrivate::dir_path(&parent),
                        LogPrivate::name(&stored)
                    );
                    self.inner.spellings.remember(&parent, wanted, &stored);
                    comps[index] = stored;
                    last_fix = Some((index, Fix::Listed));
                }
                // Listed exactly, yet it didn't open: it appeared between the two
                // round trips. Nothing to correct, so the caller's answer stands.
                ComponentMatch::Exact | ComponentMatch::Absent => return Ok(None),
                ComponentMatch::Ambiguous => {
                    return Err(VolumeError::AmbiguousName(path.to_string_lossy().into_owned()));
                }
            }
        }
        Ok(None)
    }

    /// Finds the first component of `comps` at or below `floor` that doesn't open,
    /// `comps[..floor]` being known to.
    ///
    /// The whole path first: it opening ends the search, and
    /// `STATUS_OBJECT_NAME_NOT_FOUND` says only the last component is wrong.
    /// `…_PATH_NOT_FOUND` means an ancestor is, so it steps up one ancestor at a
    /// time until one opens or answers `NAME_NOT_FOUND`. A wrong component sits
    /// near the leaf in practice (an accented album one level up), so bottom-up
    /// is the short walk.
    async fn probe(
        &self,
        comps: &[String],
        floor: usize,
        cancel: Option<&CancellationToken>,
    ) -> Result<Probe, VolumeError> {
        let SessionLease {
            tree,
            mut conn,
            generation,
        } = self.clone_session().await?;
        let mut len = comps.len();
        while len > floor {
            check_cancel(cancel)?;
            let prefix = comps[..len].join("/");
            let result = tree.stat(&mut conn, &prefix).await;
            let status = result.as_ref().err().and_then(smb2::Error::status);
            if status == Some(NtStatus::OBJECT_NAME_NOT_FOUND) {
                return Ok(Probe::MissingAt(len - 1));
            }
            if status == Some(NtStatus::OBJECT_PATH_NOT_FOUND) {
                len -= 1;
                continue;
            }
            // Anything else isn't a spelling question (a dropped session, a
            // refusal), so it goes back as what it is.
            self.handle_smb_result("find_stored_spelling", &prefix, generation, result)?;
            // The deepest prefix opening means the component under it is the miss.
            return Ok(if len == comps.len() {
                Probe::Opens
            } else {
                Probe::MissingAt(len)
            });
        }
        // Everything above `floor` opens, so the component at it is the miss.
        Ok(Probe::MissingAt(floor))
    }

    /// The names in share-relative directory `parent`: from a pane already
    /// showing it when a live watch keeps that view exact, else from the server.
    async fn entry_names(&self, parent: &str) -> Result<Vec<String>, VolumeError> {
        let display = self.to_display_path(parent);
        if let Some(entries) = self
            .inner
            .host()
            .listings()
            .authoritative_listing(&self.inner.volume_id, Path::new(&display))
        {
            return Ok(entries.into_iter().map(|e| e.name).collect());
        }
        let SessionLease {
            tree,
            mut conn,
            generation,
        } = self.clone_session().await?;
        let result = tree.list_directory(&mut conn, parent).await;
        let entries = self.handle_smb_result("find_stored_spelling", parent, generation, result)?;
        Ok(entries
            .into_iter()
            .map(|e| e.name)
            .filter(|n| n != "." && n != "..")
            .collect())
    }
}

fn check_cancel(cancel: Option<&CancellationToken>) -> Result<(), VolumeError> {
    match cancel {
        Some(token) if token.is_cancelled() => Err(VolumeError::Cancelled("Finding a stored spelling".into())),
        _ => Ok(()),
    }
}

#[cfg(test)]
#[path = "spelling_test.rs"]
mod spelling_test;
