//! Where every Cmdr scratch file is minted, the naming convention they all
//! follow, and which of them an operation currently owns.
//!
//! Cmdr never writes a file at its final name until the last byte has arrived.
//! Every write lands on a sibling carrying one of the markers below and takes
//! its real name by a rename, so a crash mid-transfer leaves something nobody
//! mistakes for their data (`write_operations/transfer/staged_write.rs`).
//!
//! Two markers, both ours:
//!
//! - [`STAGING_TEMP_MARKER`] (`.cmdr-tmp-`) carries the NEW bytes on their way in.
//! - [`STAGING_ASIDE_MARKER`] (`.cmdr-temp-`) holds the ORIGINAL file a
//!   safe-overwrite renamed out of the way, so it survives until the replacement
//!   is complete (`write_operations/overwrite.rs`).
//!
//! Both are infixes, not prefixes: the temp for `photo.jpg` is
//! `photo.jpg.cmdr-tmp-<uuid>`, keeping the original name legible in a crash
//! leftover. A leading dot would have hidden them from the dotfile filter for
//! free, but it would also hide them from everyone browsing with hidden files
//! shown, which is where a leftover most needs to be seen.
//!
//! ## Why an RAII mint instead of a register call
//!
//! [`StagingTemp::mint`] is the only way to name a temp, so registering isn't a
//! step anyone can forget: you can't get the path without getting the guard, and
//! dropping the guard is what un-hides the file. A fifth temp-producing site
//! added next year inherits the behavior by construction.
//!
//! ## Why a guard alone isn't enough
//!
//! A guard un-registers on drop, and a transfer driver ABANDONS tasks that won't
//! wind down under its cancel deadline — an abandoned task keeps its guard alive,
//! so that drop may never come. A leaked registration would hide a real leftover
//! forever, which is the bug this module exists to prevent, inverted.
//!
//! So a temp minted by an operation also carries a [`Weak`] to that operation's
//! liveness token, which the operation drops when it settles. A registration
//! whose owner is gone stops hiding anything, whether or not its guard was ever
//! dropped. A force-quit gets the same answer for free, since the registry lives
//! in memory and dies with the process.
//!
//! A temp minted with NO owner (the local safe-overwrite's two scratch files)
//! stays in flight until its guard drops, which is every return path including a
//! panic unwind. Only a permanently stuck blocking thread leaks one, and that
//! thread still owns the file.
//!
//! ## What lives here and what doesn't
//!
//! This module answers "is this one of ours, and does a live operation own it?".
//! Whether the user SEES it is the app's call, not a backend's: the settings and
//! the listing read-path filter live in `file_system::staging`, which re-exports
//! everything here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, Weak};

use crate::ignore_poison::IgnorePoison;

/// Marks a file holding bytes on their way IN: the staging sibling a write
/// streams into before the rename that gives it its real name.
pub const STAGING_TEMP_MARKER: &str = ".cmdr-tmp-";

/// Marks a file holding the ORIGINAL bytes of a safe-overwrite: the file that
/// was already at the destination, renamed aside so it survives until the
/// replacement is fully written.
pub const STAGING_ASIDE_MARKER: &str = ".cmdr-temp-";

/// Whether `name` is one of Cmdr's scratch files.
///
/// Matches on the file NAME, so pass `path.file_name()`, never a whole path: a
/// directory somewhere up the tree could otherwise carry a marker and make
/// everything under it look like scratch.
///
/// A `true` here says only "Cmdr's naming convention", NOT "safe to delete" and
/// NOT "hide it". A leftover from an interrupted transfer wears the same name as
/// a live one, and telling those apart takes the operation state
/// (`write_operations::is_staging_temp_in_flight`), not the name.
pub fn is_staging_temp_name(name: &str) -> bool {
    name.contains(STAGING_TEMP_MARKER) || name.contains(STAGING_ASIDE_MARKER)
}

/// Who a registration belongs to: `Some` weak handle to the minting operation's
/// liveness token, or `None` for a temp minted outside one.
///
/// Deliberately a bare `Weak<()>` rather than anything richer. All this module
/// asks is "is whoever minted this still running?", so a caller hands over a
/// downgraded token of its own and nothing about its state has to be visible
/// here. A registration whose owner no longer upgrades has outlived its
/// operation and stops counting as in flight, which is what makes an abandoned
/// task's leaked guard harmless.
pub type StagingOwner = Option<Weak<()>>;

/// The scratch files Cmdr has on disk right now, by file name, each with the
/// owner of every outstanding claim.
///
/// A `Vec` rather than a count because each claim carries its own owner. In
/// practice a name is claimed exactly once (every mint invents a fresh UUID);
/// the vector is what keeps a second claim from being un-hidden by the first
/// one's drop.
///
/// Keyed by file NAME, not path: every name ends in a UUID, so it identifies the
/// file on its own, and name matching spares the listing layer the volume-path
/// to display-path mapping SMB and MTP entries go through.
static ACTIVE_TEMPS: LazyLock<Mutex<HashMap<String, Vec<StagingOwner>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// A scratch-file path, counted as in flight for as long as this guard lives.
///
/// Obtained from [`StagingTemp::mint`] or [`StagingTemp::adopt`]; deregisters on
/// drop. Hold it for exactly as long as the file is on disk under its temporary
/// name: past the rename that gives it its real name, or past the delete that
/// removes it.
#[derive(Debug)]
pub struct StagingTemp {
    path: PathBuf,
    /// The registry key, kept so `Drop` doesn't have to re-derive it from
    /// `path`. `None` when the path had no file name and nothing was registered.
    name: Option<String>,
}

impl StagingTemp {
    /// Names the scratch file a write to `final_path` should land on: a sibling
    /// carrying `final_path`'s name plus [`STAGING_TEMP_MARKER`] and a UUID.
    ///
    /// Keeping the original name in front of the marker is deliberate: it's what
    /// makes a leftover self-explanatory to whoever finds one.
    pub fn mint(final_path: &Path, owner: StagingOwner) -> Self {
        Self::mint_with_uuid(final_path, uuid::Uuid::new_v4(), owner)
    }

    /// [`mint`](Self::mint) with the UUID chosen by the caller, for a
    /// safe-overwrite that wants its temp and its aside to share one.
    pub fn mint_with_uuid(final_path: &Path, uuid: uuid::Uuid, owner: StagingOwner) -> Self {
        Self::sibling(final_path, STAGING_TEMP_MARKER, uuid, owner)
    }

    /// Names the file a safe-overwrite renames the ORIGINAL aside to, so it
    /// survives until its replacement is fully written.
    ///
    /// Pass the same `uuid` as the [`mint_with_uuid`](Self::mint_with_uuid) that
    /// named the replacement, so a leftover pair is recognizable as two halves
    /// of one interrupted overwrite.
    pub fn mint_aside(final_path: &Path, uuid: uuid::Uuid, owner: StagingOwner) -> Self {
        Self::sibling(final_path, STAGING_ASIDE_MARKER, uuid, owner)
    }

    fn sibling(final_path: &Path, marker: &str, uuid: uuid::Uuid, owner: StagingOwner) -> Self {
        let parent = final_path.parent().unwrap_or(Path::new(""));
        let file_name = final_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self::adopt(parent.join(format!("{file_name}{marker}{uuid}")), owner)
    }

    /// Takes ownership of a scratch path that already exists, for the callers
    /// that receive one rather than mint it (a conflict-minted temp handed down
    /// to the writer).
    pub fn adopt(path: PathBuf, owner: StagingOwner) -> Self {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        if let Some(name) = &name {
            ACTIVE_TEMPS
                .lock_ignore_poison()
                .entry(name.clone())
                .or_default()
                .push(owner);
        }
        Self { path, name }
    }

    /// Where the bytes go.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for StagingTemp {
    fn drop(&mut self) {
        let Some(name) = self.name.take() else { return };
        let mut active = ACTIVE_TEMPS.lock_ignore_poison();
        let Some(owners) = active.get_mut(&name) else { return };
        // Which claim goes is irrelevant: every claim on one name comes from the
        // same operation, so they're interchangeable for liveness.
        owners.pop();
        if owners.is_empty() {
            active.remove(&name);
        }
    }
}

/// Whether `name` is a scratch file a LIVE operation currently owns.
///
/// `false` for a leftover nobody owns: that's a real file the user should see
/// and be able to delete. An ownerless claim (`None`) counts as live; see the
/// module docs for why that's the safe reading.
pub fn is_staging_temp_in_flight(name: &str) -> bool {
    // Ordered cheapest-first: almost every filename fails the name test and
    // never reaches the lock.
    is_staging_temp_name(name)
        && ACTIVE_TEMPS
            .lock_ignore_poison()
            .get(name)
            .is_some_and(|owners| owners.iter().any(owner_is_live))
}

fn owner_is_live(owner: &StagingOwner) -> bool {
    owner.as_ref().is_none_or(|token| token.strong_count() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Stands in for a running operation: the temps it mints stay in flight
    /// until the returned `Arc` is dropped, which is what settling does for
    /// real.
    fn running_operation() -> Arc<()> {
        Arc::new(())
    }

    fn name_of(temp: &StagingTemp) -> String {
        temp.path()
            .file_name()
            .expect("a minted temp always has a file name")
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn a_minted_temp_is_a_recognizable_sibling_of_its_final_name() {
        let temp = StagingTemp::mint(Path::new("/dir/photo.jpg"), None);
        assert_eq!(temp.path().parent(), Some(Path::new("/dir")));
        let name = name_of(&temp);
        assert!(
            name.starts_with("photo.jpg"),
            "the original name must stay legible: {name}"
        );
        assert!(is_staging_temp_name(&name), "got {name}");
    }

    /// The whole point of the guard: while it lives the temp is in flight, and
    /// the moment it drops it isn't.
    #[test]
    fn a_live_guard_keeps_its_temp_in_flight_and_dropping_it_stops() {
        let op = running_operation();
        let temp = StagingTemp::mint(Path::new("/dir/photo.jpg"), Some(Arc::downgrade(&op)));
        let name = name_of(&temp);

        assert!(is_staging_temp_in_flight(&name));
        drop(temp);
        assert!(!is_staging_temp_in_flight(&name));
    }

    /// An operation that wedges and leaves its temps on disk must not leave them
    /// counted as in flight. Its guard may never drop — the driver abandons a
    /// task that won't wind down, and the task keeps holding it — so the
    /// operation's liveness is what has to answer.
    #[test]
    fn a_leftover_from_a_dead_operation_is_no_longer_in_flight_even_if_its_guard_leaked() {
        let op = running_operation();
        let temp = StagingTemp::mint(Path::new("/dir/photo.jpg"), Some(Arc::downgrade(&op)));
        let name = name_of(&temp);
        assert!(is_staging_temp_in_flight(&name), "in flight while the operation runs");

        drop(op);

        // The guard deliberately stays alive, standing in for the abandoned task.
        assert!(
            !is_staging_temp_in_flight(&name),
            "a leftover nobody is running for must stop counting"
        );
        drop(temp);
    }

    /// A temp minted outside any operation (the local safe-overwrite's scratch)
    /// stays in flight until its guard drops, since there's no operation to
    /// outlive.
    #[test]
    fn an_ownerless_temp_stays_in_flight_until_its_guard_drops() {
        let temp = StagingTemp::mint(Path::new("/dir/photo.jpg"), None);
        let name = name_of(&temp);

        assert!(is_staging_temp_in_flight(&name));
        drop(temp);
        assert!(!is_staging_temp_in_flight(&name));
    }

    /// Two guards can hold one name; the first drop must not release it out from
    /// under the second.
    #[test]
    fn a_name_claimed_twice_stays_in_flight_until_the_last_guard_goes() {
        let first = StagingTemp::mint(Path::new("/dir/photo.jpg"), None);
        let name = name_of(&first);
        let second = StagingTemp::adopt(first.path().to_path_buf(), None);

        drop(first);
        assert!(is_staging_temp_in_flight(&name), "the second guard still owns it");
        drop(second);
        assert!(!is_staging_temp_in_flight(&name));
    }

    /// An aside carries the same UUID as the temp replacing it, so a leftover
    /// pair reads as two halves of one interrupted overwrite.
    #[test]
    fn an_aside_shares_its_uuid_with_the_replacement() {
        let uuid = uuid::Uuid::new_v4();
        let temp = StagingTemp::mint_with_uuid(Path::new("/dir/photo.jpg"), uuid, None);
        let aside = StagingTemp::mint_aside(Path::new("/dir/photo.jpg"), uuid, None);

        let aside_name = name_of(&aside);
        assert!(aside_name.contains(&uuid.to_string()), "got {aside_name}");
        assert!(name_of(&temp).contains(&uuid.to_string()));
        assert!(is_staging_temp_in_flight(&aside_name));
    }

    /// Ordinary files are never in flight, however many temps are.
    #[test]
    fn an_ordinary_file_is_never_in_flight() {
        let _temp = StagingTemp::mint(Path::new("/dir/photo.jpg"), None);
        assert!(!is_staging_temp_in_flight("photo.jpg"));
        assert!(!is_staging_temp_in_flight(".gitignore"));
    }

    #[test]
    fn recognizes_both_markers() {
        assert!(is_staging_temp_name("photo.jpg.cmdr-tmp-3f2a"));
        assert!(is_staging_temp_name("photo.jpg.cmdr-temp-3f2a"));
    }

    /// The markers are infixes, so the original name stays legible in front of
    /// them and a leftover tells the user which file it came from.
    #[test]
    fn matches_mid_name_not_only_at_the_start() {
        assert!(is_staging_temp_name("a.very.long.name.tar.gz.cmdr-tmp-3f2a"));
    }

    #[test]
    fn leaves_ordinary_names_alone() {
        assert!(!is_staging_temp_name("photo.jpg"));
        assert!(!is_staging_temp_name(".hidden"));
        // Close, but not ours: no trailing separator before the uuid.
        assert!(!is_staging_temp_name("notes.cmdr-tmp"));
        assert!(!is_staging_temp_name("cmdr-tmp-notes"));
    }
}
