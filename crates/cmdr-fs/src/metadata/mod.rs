//! What a copy puts on the item it wrote beyond the bytes, in the order the
//! filesystem allows: the one host helper every copy path calls, so the ordering
//! rules live in exactly one place.
//!
//! **Two halves.** [`apply_staged`] puts [`CarriedMetadata`] on the staged item
//! before its landing rename: xattrs, then the mode, then birth, mtime, and atime
//! in ONE call (a resource-fork write bumps mtime, and APFS drags birth down to
//! an earlier mtime). [`apply_landed`] puts [`LandedMetadata`] on the landed item
//! after it: a folder's mode (once its children are in), then the ACL, then the
//! flags.
//!
//! **Total, by type.** Every apply function returns a [`MetadataReport`] and has
//! no error path, so a refused metadata write can never fail a copy: the bytes
//! are the copy. Each class is its own independent step; a refused ACL never
//! skips the flags after it.
//!
//! **Some metadata can't go on before the landing rename.** `uchg`/`uappnd` on an
//! item (`EPERM`) and an ACL with `deny delete` on it (`EACCES`) make `rename(2)`
//! refuse, and so does a staging tree that has to be renamed again later. So the
//! rename-blocking set ([`LandedMetadata`]: the ACL, then the user flags) goes on
//! the LANDED item through [`apply_landed`], ACL first, because `uchg` refuses a
//! later ACL write.
//!
//! **What Cmdr made, Cmdr can remove.** A copy that carried Locked or `deny
//! delete` onto its landed item made something `unlink(2)` refuses, even for its
//! owner. [`clear_blockers`] clears the user flags and strips the ACL of one
//! item, and the `remove_*_clearing_blockers` helpers do it only after a first
//! removal is refused, so a plain item costs exactly the removal it did before.
//! ❗ They act on the item itself, never a symlink's target.
//!
//! macOS carries both classes. On Linux the ACL is a POSIX ACL, which carries no
//! `deny` entries and blocks no rename; the flags (`chattr`) need a capability a
//! desktop app doesn't have, so they answer [`ClassOutcome::NotApplicable`] and
//! nothing ever needs clearing. Background and the measurements behind each
//! rule: `DETAILS.md` § "`metadata`: what a copy keeps, and when it goes on".

mod blockers;
mod caps;
mod carried;
mod classes;
mod flags;
mod items;
mod mode;
mod platform;
mod staged;

use std::path::Path;

pub use blockers::{
    OpenedFolder, ParentOpening, RemovalLock, RemovalRefusal, Unblock, remove_dir_clearing_blockers,
    remove_file_clearing_blockers, remove_tree_clearing_blockers,
};
pub use caps::{MetadataAsk, MetadataCaps};
pub use carried::CarriedMetadata;
pub use classes::MetadataClasses;
pub use flags::UserFlags;
pub use items::{StagedItem, StagedTimes, Xattr};
pub use mode::{default_file_mode, default_folder_mode, routed_mode};
pub use staged::apply_staged;

// The blockers' public doors sit here, not in `blockers.rs`: that module holds the types the
// per-OS arms answer in, so a door there would make it and `platform/` import each other.

/// Clears `path`'s own `uchg`/`uappnd` and strips its ACL, flags first because
/// `uchg` refuses an ACL write. Never follows a symlink. On Linux there's never
/// anything to clear (`blockers.rs`' module docs).
pub fn clear_blockers(path: &Path) -> Unblock {
    platform::clear_blockers(path)
}

/// Whether `path` itself (never a symlink's target) wears a [`RemovalLock`].
/// System flags win over the user's lock: an item wearing both can't move
/// either way. A deny entry that only reaches the item's future children
/// (`only_inherit`) doesn't count, and neither does an ACL that can't be read:
/// the move then meets the refusal where it would have anyway. Always
/// [`RemovalLock::Free`] off macOS (`blockers.rs`' module docs).
pub fn removal_lock(path: &Path) -> RemovalLock {
    platform::removal_lock(path)
}

/// Opens the folder holding `child` so `child` can leave it: clears its own
/// `uchg`/`uappnd`, strips its ACL (a `deny delete_child` refuses the unlink),
/// and adds owner write and search when it lacks them (a `0500` folder refuses
/// it too). For a folder a copy made and carried a source's metadata onto, which
/// a reversal now empties. ❗ The folder isn't removed here: what's opened is put
/// back by [`OpenedFolder::close`].
pub fn open_parent_folder(child: &Path) -> ParentOpening {
    platform::open_parent_folder(child)
}

/// What happened to one metadata class on one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassOutcome {
    /// The item carries the source's value.
    Kept,
    /// The source had a value and the destination refused it. Logged, never an
    /// error: the copy stands.
    Refused,
    /// The platform or the destination can't hold this class.
    NotApplicable,
    /// The source had nothing of this class to carry.
    Absent,
}

impl ClassOutcome {
    /// The outcome that says more about what the copy lost: a refusal outranks a
    /// class the destination can't hold, which outranks a kept one.
    #[cfg_attr(
        target_family = "wasm",
        allow(dead_code, reason = "the browser build writes no xattrs")
    )]
    pub(super) fn worse(self, other: ClassOutcome) -> ClassOutcome {
        use ClassOutcome::{Kept, NotApplicable, Refused};
        match (self, other) {
            (Refused, _) | (_, Refused) => Refused,
            (NotApplicable, _) | (_, NotApplicable) => NotApplicable,
            (Kept, other) | (other, Kept) => other,
            (a, _) => a,
        }
    }
}

/// Per class, what one apply call did. Returned by every apply function in
/// place of a `Result`. Each half reports the classes it puts on; the rest read
/// [`ClassOutcome::Absent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetadataReport {
    /// The extended attributes (Finder tags, comments, FinderInfo, resource
    /// forks, apps' own), as one class: `Refused` if any one was.
    pub xattrs: ClassOutcome,
    /// The permission bits.
    pub mode: ClassOutcome,
    /// The creation (birth) date.
    pub created: ClassOutcome,
    /// The modification date.
    pub modified: ClassOutcome,
    /// The ACL.
    pub acl: ClassOutcome,
    /// The user flags (Hidden, Locked, append-only, no-dump).
    pub flags: ClassOutcome,
}

impl MetadataReport {
    /// Every class `Absent`: the report of a call that had nothing to put on.
    pub const ABSENT: Self = Self {
        xattrs: ClassOutcome::Absent,
        mode: ClassOutcome::Absent,
        created: ClassOutcome::Absent,
        modified: ClassOutcome::Absent,
        acl: ClassOutcome::Absent,
        flags: ClassOutcome::Absent,
    };

    /// The report of a destination that can hold none of what it was handed:
    /// `NotApplicable` for each class `carried` (and `modified`) has a value
    /// for, `Absent` for the rest. The default answer of
    /// `Volume::apply_metadata`.
    pub fn none_applicable(carried: &CarriedMetadata, modified: Option<std::time::SystemTime>) -> Self {
        Self::uniform(carried, modified, ClassOutcome::NotApplicable)
    }

    /// `outcome` for each class `carried` (and `modified`) has a value for,
    /// `Absent` for the rest: the report of a destination that answers every
    /// class the same way.
    pub fn uniform(carried: &CarriedMetadata, modified: Option<std::time::SystemTime>, outcome: ClassOutcome) -> Self {
        let class = |present: bool| if present { outcome } else { ClassOutcome::Absent };
        Self {
            xattrs: class(!carried.xattrs.is_empty()),
            mode: class(carried.mode.is_some()),
            created: class(carried.created.is_some()),
            modified: class(modified.is_some()),
            acl: ClassOutcome::Absent,
            flags: class(!carried.flags.is_empty()),
        }
    }

    /// Every class reported, in one fixed order.
    fn classes(&self) -> [ClassOutcome; 6] {
        [
            self.xattrs,
            self.mode,
            self.created,
            self.modified,
            self.acl,
            self.flags,
        ]
    }

    /// Whether the destination couldn't hold ANY class it was handed: nothing
    /// kept, nothing refused, and at least one class it can't hold. A caller
    /// applying the same kind of set item after item (a folder pass) stops
    /// asking on this answer.
    pub fn holds_none(&self) -> bool {
        let classes = self.classes();
        classes.contains(&ClassOutcome::NotApplicable)
            && !classes.contains(&ClassOutcome::Kept)
            && !classes.contains(&ClassOutcome::Refused)
    }

    /// Whether any class was refused.
    pub fn any_refused(&self) -> bool {
        self.classes().contains(&ClassOutcome::Refused)
    }

    /// The engine classes refused: the creation date, the mode, the flags, and
    /// the xattrs. The modification date and the ACL have no engine class.
    pub fn refused_classes(&self) -> MetadataClasses {
        [
            (self.created, MetadataClasses::CREATED),
            (self.mode, MetadataClasses::MODE),
            (self.flags, MetadataClasses::FLAGS),
            (self.xattrs, MetadataClasses::XATTRS),
        ]
        .into_iter()
        .filter(|(outcome, _)| *outcome == ClassOutcome::Refused)
        .fold(MetadataClasses::NONE, |refused, (_, class)| refused.union(class))
    }
}

pub use platform::{Acl, LOCAL_DISK_CAPS};

/// The metadata that goes on an item only after its landing rename, because on
/// the staged item it would refuse that rename or a later one.
///
/// Small and `Clone`: a cross-filesystem move holds one per item that has any,
/// keyed by staged path, until its Phase 3 has renamed everything into place.
// DEFAULT-OK: the empty set is "nothing to apply after landing", the answer for
// an ordinary file; it states nothing about what the disk holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LandedMetadata {
    /// A FOLDER's final mode. A file's goes on its temp ([`apply_staged`]); a
    /// folder's waits for its children, since `0500` refuses them.
    mode: Option<u32>,
    acl: Option<Acl>,
    flags: UserFlags,
}

impl LandedMetadata {
    /// The set carrying only `source`'s ACL, read without following a symlink.
    /// Empty when it has none, when it can't be read, and on Linux when its
    /// POSIX ACL is only the three mode entries every file has (`chmod` already
    /// carries those).
    pub fn of_source_acl(source: &Path) -> Self {
        Self {
            mode: None,
            acl: platform::read_acl(source),
            flags: UserFlags::NONE,
        }
    }

    /// This set plus `flags` (a source's user flags, which go on after the
    /// landing with the ACL).
    pub fn with_flags(mut self, flags: UserFlags) -> Self {
        self.flags = self.flags | flags;
        self
    }

    /// This set plus a FOLDER's final permission bits (`0o7777`), which go on
    /// once its children have landed, before the ACL and the flags.
    pub fn with_mode(mut self, mode: u32) -> Self {
        self.mode = Some(mode);
        self
    }

    /// Whether there's nothing to apply, the common case. A caller that holds
    /// an empty set skips [`apply_landed`] entirely.
    pub fn is_empty(&self) -> bool {
        self.mode.is_none() && self.acl.is_none() && self.flags.is_empty()
    }

    /// The user flags to add on landing.
    pub fn flags(&self) -> UserFlags {
        self.flags
    }

    /// Whether the set carries an ACL.
    pub fn has_acl(&self) -> bool {
        self.acl.is_some()
    }

    /// Folds `other` in: its mode and ACL replace these when it has them, and
    /// its flags join these.
    pub fn merge(&mut self, other: LandedMetadata) {
        if other.mode.is_some() {
            self.mode = other.mode;
        }
        if other.acl.is_some() {
            self.acl = other.acl;
        }
        self.flags = self.flags | other.flags;
    }
}

/// Puts `set` on the landed item at `path`: a folder's mode, then the ACL, then
/// the flags (added to what the item already wears), each only when the set has
/// one, each independent of the others. Flags go last because `uchg` refuses a
/// later `chmod` and a later ACL write.
pub fn apply_landed(path: &Path, set: &LandedMetadata) -> MetadataReport {
    note_helper_call();
    let mode = match set.mode {
        None => ClassOutcome::Absent,
        Some(mode) => platform::write_mode_at(path, mode),
    };
    let acl = match &set.acl {
        None => ClassOutcome::Absent,
        Some(acl) => platform::write_acl(path, acl),
    };
    let flags = if set.flags.is_empty() {
        ClassOutcome::Absent
    } else {
        platform::add_flags(path, set.flags)
    };
    MetadataReport {
        mode,
        acl,
        flags,
        ..MetadataReport::ABSENT
    }
}

/// Test seam: how many times this thread has called into the helper's
/// syscall-making entry points ([`apply_staged`], [`apply_landed`],
/// [`clear_blockers`]), so a test
/// can prove a plain file never reaches them.
#[cfg(any(test, feature = "testing"))]
pub fn helper_calls_on_this_thread() -> u32 {
    HELPER_CALLS.with(std::cell::Cell::get)
}

#[cfg(any(test, feature = "testing"))]
thread_local! {
    static HELPER_CALLS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

fn note_helper_call() {
    #[cfg(any(test, feature = "testing"))]
    HELPER_CALLS.with(|calls| calls.set(calls.get() + 1));
}

#[cfg(test)]
mod tests;

#[cfg(all(test, target_os = "linux"))]
mod linux_characterization_tests;
