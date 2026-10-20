//! Where every Cmdr scratch file is minted, the naming convention they all
//! follow, and which of them an operation currently owns.
//!
//! Cmdr never writes a file at its final name until the last byte has arrived.
//! Every write lands on a sibling carrying one of the markers below and takes
//! its real name by a rename, so a crash mid-transfer leaves something nobody
//! mistakes for their data (`crates/cmdr-operations/src/transfer/staged_write.rs`).
//!
//! Two markers, both ours:
//!
//! - [`STAGING_TEMP_MARKER`] (`.cmdr-tmp-`) carries the NEW bytes on their way in.
//! - [`STAGING_ASIDE_MARKER`] (`.cmdr-temp-`) holds the ORIGINAL file a
//!   safe-overwrite renamed out of the way, so it survives until the replacement
//!   is complete (`crates/cmdr-operations/src/overwrite.rs`).
//!
//! Plus one directory shape, [`STAGING_DIR_PREFIX`] (`.cmdr-staging-<op>`): the
//! folder a cross-filesystem move stages a whole tree in before renaming it into
//! place. It registers through the same mint, so it hides by the same ownership
//! rule, but a sweep only ever `remove_dir`s it — the tree inside can be the
//! user's only copy.
//!
//! And one more folder shape, [`STAGING_DELETING_MARKER`]
//! (`album.cmdr-deleting-<uuid>`): a folder a WebDAV delete moved aside to count
//! it where nothing new can land, held by the same mint while it's there.
//!
//! The two markers are infixes, not prefixes: the temp for `photo.jpg` is
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
//! ## Two categories, two rules, and why they differ
//!
//! A directory watcher happily reports scratch to the pane, and macOS apps
//! stage their saves too (`file.txt.sb-1e64c894-…`), so a listing has two kinds
//! of other people's scratch to deal with.
//!
//! **Cmdr's own** (`.cmdr-tmp-*`, `.cmdr-temp-*`, `.cmdr-staging-*`, `.cmdr-deleting-*`) hides by
//! OWNERSHIP. One a running operation put there is noise that'll be gone in
//! moments; one nobody owns is a LEFTOVER from an interrupted transfer, and
//! hiding that would be lying about what's on disk — and about a Cmdr bug worth
//! seeing. So: hide a scratch file while an operation has it open, and show
//! every other one.
//!
//! **Other apps'** (`.sb-`, macOS safe-save) hides by NAME, because we have no
//! ownership signal for a file another process is writing and no way to tell a
//! live one from an abandoned one. The coarser rule is defensible here precisely
//! because it isn't ours: an abandoned `.sb-` is evidence of TextEdit's day, not
//! of a Cmdr bug, so nothing is lost by not surfacing it. ❌ Don't reach for this
//! rule for Cmdr's own scratch, where ownership is knowable and the leftover is
//! the interesting case.
//!
//! ## What lives here and what doesn't
//!
//! This module answers "is this one of ours, does a live operation own it, and
//! does a listing hide it under these two settings?" ([`hidden_with_settings`]).
//! The settings themselves are the app's, and the listing engine applies the
//! answer on its read path (`cmdr-listing`'s `visible_rows.rs`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, Weak};

use crate::ignore_poison::IgnorePoison;

mod names;

pub use names::{
    MAX_SCRATCH_NAME_BYTES, deleting_sibling_name, temp_sibling_name, temp_sibling_path, temp_sibling_prefix,
};

/// Marks a file holding bytes on their way IN: the staging sibling a write
/// streams into before the rename that gives it its real name.
pub const STAGING_TEMP_MARKER: &str = ".cmdr-tmp-";

/// Marks a file holding the ORIGINAL bytes of a safe-overwrite: the file that
/// was already at the destination, renamed aside so it survives until the
/// replacement is fully written.
pub const STAGING_ASIDE_MARKER: &str = ".cmdr-temp-";

/// Names the DIRECTORY a cross-filesystem move stages a whole tree in, before
/// renaming it into place (`crates/cmdr-operations/src/transfer/move_op/cross_fs.rs`).
///
/// A prefix, not an infix like the two markers above: the folder carries no
/// user-chosen name to keep legible, only the operation's own id.
pub const STAGING_DIR_PREFIX: &str = ".cmdr-staging-";

/// Marks a FOLDER a delete moved aside to count it where nobody else can write,
/// before deleting it or moving it back (`crates/cmdr-webdav/DETAILS.md`
/// § "A folder's DELETE"). An infix like the file markers, so a leftover still
/// says which folder it was: `album.cmdr-deleting-<uuid>`.
pub const STAGING_DELETING_MARKER: &str = ".cmdr-deleting-";

/// Whether `name` is one of Cmdr's scratch files.
///
/// Matches on the file NAME, so pass `path.file_name()`, never a whole path: a
/// directory somewhere up the tree could otherwise carry a marker and make
/// everything under it look like scratch.
///
/// A `true` here says only "Cmdr's naming convention", NOT "safe to delete" and
/// NOT "hide it". A leftover from an interrupted transfer wears the same name as
/// a live one, and telling those apart takes the operation state
/// (`is_staging_temp_in_flight`, below), not the name.
pub fn is_staging_temp_name(name: &str) -> bool {
    name.contains(STAGING_TEMP_MARKER) || name.contains(STAGING_ASIDE_MARKER)
}

/// Whether `name` is a cross-filesystem move's staging DIRECTORY.
///
/// A strict parse, unlike [`is_staging_temp_name`]'s substring test, and
/// deliberately so: a sweep acts on one of these, and the only thing standing
/// between a folder somebody named `.cmdr-staging-notes` by hand and that sweep
/// is this function. So the whole name has to be the prefix plus exactly the
/// hyphenated UUID `cmdr_operation_log::new_operation_id` mints — nothing before it,
/// nothing after it.
pub fn is_staging_dir_name(name: &str) -> bool {
    name.strip_prefix(STAGING_DIR_PREFIX)
        .is_some_and(|id| id.len() == UUID_TEXT_LEN && uuid::Uuid::parse_str(id).is_ok())
}

/// Whether `name` is a folder a delete moved aside ([`STAGING_DELETING_MARKER`]).
///
/// Strict like [`is_staging_dir_name`], for the same reason: the tidy that acts
/// on one deletes it. So the name has to END in the marker plus a hyphenated
/// UUID; whatever stands in front is the original folder's name.
pub fn is_deleting_aside_name(name: &str) -> bool {
    name.rsplit_once(STAGING_DELETING_MARKER)
        .is_some_and(|(_, id)| id.len() == UUID_TEXT_LEN && uuid::Uuid::parse_str(id).is_ok())
}

/// The length of a hyphenated UUID, which is what `Uuid::to_string` writes, the
/// only form [`is_staging_dir_name`] accepts, and the room every scratch file
/// name keeps for its unique suffix (`staging/names.rs`).
const UUID_TEXT_LEN: usize = 36;

/// Whether `name` is scratch Cmdr itself left there, of any shape: a staged
/// write, a safe-overwrite aside, a move's staging directory, or a folder a
/// delete moved aside.
///
/// The question the listing layer asks. ❌ Never the question a SWEEP asks: a
/// sweep acts per kind, under that kind's own rules, and needs the strict test
/// for the kind it holds a record for (`cmdr_operations::in_flight_sweep`).
pub fn is_cmdr_scratch_name(name: &str) -> bool {
    is_staging_temp_name(name) || is_staging_dir_name(name) || is_deleting_aside_name(name)
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
    /// makes a leftover self-explanatory to whoever finds one. A name too long to
    /// carry the suffix gets shortened in front of the marker
    /// ([`MAX_SCRATCH_NAME_BYTES`], `staging/names.rs`).
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
        let (parent, file_name) = names::split_final_path(final_path);
        Self::adopt(parent.join(names::scratch_name(&file_name, marker, uuid)), owner)
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

/// Whether `name` is scratch a LIVE operation currently owns, of any shape
/// [`is_cmdr_scratch_name`] knows.
///
/// `false` for a leftover nobody owns: that's a real file the user should see
/// and be able to delete. An ownerless claim (`None`) counts as live; see the
/// module docs for why that's the safe reading.
pub fn is_staging_temp_in_flight(name: &str) -> bool {
    // Ordered cheapest-first: almost every filename fails the name test and
    // never reaches the lock.
    is_cmdr_scratch_name(name)
        && ACTIVE_TEMPS
            .lock_ignore_poison()
            .get(name)
            .is_some_and(|owners| owners.iter().any(owner_is_live))
}

fn owner_is_live(owner: &StagingOwner) -> bool {
    owner.as_ref().is_none_or(|token| token.strong_count() > 0)
}

/// The marker macOS safe-save writes into its scratch name.
///
/// `NSDocument`'s atomic save makes `file.txt.sb-1e64c894-vFWIzN` next to the
/// original, writes the new version there, swaps it in, and deletes it. TextEdit,
/// Preview, and anything else built on AppKit's document machinery do this on
/// every save.
const SAFE_SAVE_MARKER: &str = ".sb-";

/// Whether `name` is another app's macOS safe-save scratch.
///
/// Name-only, and type-agnostic: safe-save creates a temp DIRECTORY as well as
/// the file inside it, and both are equally uninteresting.
pub fn is_safe_save_name(name: &str) -> bool {
    name.contains(SAFE_SAVE_MARKER)
}

/// Whether `name` is scratch by NAME: one of Cmdr's, or another app's safe-save.
///
/// Pure and stable: it reads no setting and no ownership registry, so its answer
/// for a given name never changes. That's the point: [`hidden_with_settings`] is
/// gated on it, so a name this rejects is visible now and stays visible until the
/// listing itself changes, which is what lets the listing layer cache a row map
/// once and re-ask about only these few names per read. ❗ Keep the gate: without
/// it the implication is only true by inspection of three other functions, and a
/// row map built on it would start handing panes the wrong file.
pub fn could_be_hidden_from_listings(name: &str) -> bool {
    is_cmdr_scratch_name(name) || is_safe_save_name(name)
}

/// Whether a listing leaves `name` out of the pane, given the two visibility
/// settings captured by one projection pass.
///
/// Cmdr's own scratch hides by OWNERSHIP (only while an operation has it in
/// flight, so a leftover shows); another app's safe-save hides by NAME, since no
/// ownership signal exists for it. Why the two rules differ: the module docs.
pub fn hidden_with_settings(name: &str, show_staging: bool, show_safe_save: bool) -> bool {
    could_be_hidden_from_listings(name)
        && ((!show_staging && is_staging_temp_in_flight(name)) || (!show_safe_save && is_safe_save_name(name)))
}

#[cfg(test)]
#[path = "staging/names_test.rs"]
mod names_test;

#[cfg(test)]
#[path = "staging/hidden_test.rs"]
mod hidden_test;

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

    /// The name a cross-filesystem move really writes, built the way the move
    /// builds it, so the test can't drift from the producer.
    fn staging_dir_name(operation_id: &uuid::Uuid) -> String {
        format!("{STAGING_DIR_PREFIX}{operation_id}")
    }

    #[test]
    fn a_moves_staging_directory_is_recognized_from_its_operation_id() {
        let name = staging_dir_name(&uuid::Uuid::new_v4());
        assert!(is_staging_dir_name(&name), "got {name}");
        assert!(is_cmdr_scratch_name(&name), "got {name}");
    }

    /// The strict parse is the whole safety story: a sweep removes one of these,
    /// so a folder a person named to look like ours must not be one.
    #[test]
    fn a_look_alike_staging_name_is_not_one_of_ours() {
        let real = uuid::Uuid::new_v4().to_string();
        for impostor in [
            ".cmdr-staging-".to_string(),
            ".cmdr-staging-notes".to_string(),
            // A real id with something appended, or prepended.
            format!(".cmdr-staging-{real}-backup"),
            format!("old.cmdr-staging-{real}"),
            // The unhyphenated form `Uuid::parse_str` would otherwise take.
            format!(".cmdr-staging-{}", real.replace('-', "")),
            // A directory that merely CONTAINS the prefix, the way the temp
            // markers are allowed to.
            format!("photos.cmdr-staging-{real}"),
        ] {
            assert!(!is_staging_dir_name(&impostor), "got {impostor}");
        }
    }

    /// The two shapes stay separate questions: a staging directory is not a
    /// staged write, and neither is the other.
    #[test]
    fn a_staging_directory_and_a_staged_write_dont_answer_for_each_other() {
        let dir = staging_dir_name(&uuid::Uuid::new_v4());
        assert!(!is_staging_temp_name(&dir), "got {dir}");
        assert!(!is_staging_dir_name("photo.jpg.cmdr-tmp-3f2a"));
        assert!(!is_cmdr_scratch_name("photo.jpg"));
    }

    /// A tidy deletes an empty one of these, so a folder a person named to look
    /// like ours must not be one.
    #[test]
    fn a_deleting_aside_is_recognized_strictly() {
        let name = deleting_sibling_name("album");
        assert!(name.starts_with("album.cmdr-deleting-"), "got {name}");
        assert!(
            is_deleting_aside_name(&name) && is_cmdr_scratch_name(&name),
            "got {name}"
        );
        let real = uuid::Uuid::new_v4();
        for impostor in [
            "album.cmdr-deleting-notes".to_string(),
            format!("album.cmdr-deleting-{real}x"),
            format!("album.cmdr-deleting-{}", &real.to_string()[1..]),
        ] {
            assert!(!is_deleting_aside_name(&impostor), "got {impostor}");
        }
    }
}
