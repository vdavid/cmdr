//! The rollback planner's pure decision helpers (`verify_snapshot`,
//! `classify_self_collision`, `listing_confirms_twin`, `inverse_action`, …),
//! unit-tested in isolation.
//!
//! The per-kind reversal runs end to end against the write engine's real
//! executor, so those cells live app-side, beside it
//! (`crates/cmdr-operations/src/rollback/planner_tests/`).

use std::path::{Path, PathBuf};

use super::*;
use crate::types::{EntryType, OpKind, RollbackState};
use cmdr_fs::entry::FileEntry;

/// A fixed mtime the snapshots and live entries agree on.
const MT: u64 = 1_700_000_000;

fn entry(name: &str, inode: Option<u64>, size: Option<u64>, mtime: Option<u64>) -> FileEntry {
    FileEntry {
        size,
        modified_at: mtime,
        inode,
        ..FileEntry::new(name.to_string(), format!("/{name}"), false, false)
    }
}

#[test]
fn verify_snapshot_match_drift_unverifiable() {
    let live = entry("f", None, Some(10), Some(MT));
    // Both fields recorded and equal ⇒ Match.
    assert_eq!(
        verify_snapshot(Some(10), Some(MT as i64), live.size, live.modified_at),
        SnapshotVerdict::Match
    );
    // A recorded field differs ⇒ Drift.
    assert_eq!(
        verify_snapshot(Some(10), Some(MT as i64 + 1), live.size, live.modified_at),
        SnapshotVerdict::Drift
    );
    assert_eq!(
        verify_snapshot(Some(11), Some(MT as i64), live.size, live.modified_at),
        SnapshotVerdict::Drift
    );
    // Only size recorded (volume transfers carry no mtime), and it matches ⇒ Match.
    assert_eq!(
        verify_snapshot(Some(10), None, live.size, live.modified_at),
        SnapshotVerdict::Match
    );
    // A recorded field whose live counterpart is absent ⇒ Unverifiable (fail safe).
    let no_mtime = entry("f", None, Some(10), None);
    assert_eq!(
        verify_snapshot(Some(10), Some(MT as i64), no_mtime.size, no_mtime.modified_at),
        SnapshotVerdict::Unverifiable
    );
    // Nothing recorded ⇒ nothing to prove identity on ⇒ Unverifiable.
    assert_eq!(
        verify_snapshot(None, None, live.size, live.modified_at),
        SnapshotVerdict::Unverifiable
    );
}

#[test]
fn self_collision_is_decided_by_identity_never_by_folding() {
    let classify = |same_volume: bool, from: &str, to: &str, a: &FileEntry, b: &FileEntry| {
        classify_self_collision(same_volume, Path::new(from), Path::new(to), a, b)
    };
    let twin = |parent: &str, from: &str, to: &str| SpellingTwin {
        parent: PathBuf::from(parent),
        from_name: from.to_string(),
        to_name: to.to_string(),
    };
    // Real inodes (LocalPosix, hard-linked files): same inode ⇒ self.
    let a = entry("dog.JPG", Some(7), Some(1), Some(MT));
    let b = entry("dog.jpg", Some(7), Some(1), Some(MT));
    assert_eq!(classify(true, "/d/dog.JPG", "/d/dog.jpg", &a, &b), SelfCollision::Same);
    // Different inodes ⇒ a real collision, even on the same path.
    let c = entry("dog.jpg", Some(9), Some(1), Some(MT));
    assert_eq!(
        classify(true, "/d/dog.jpg", "/d/dog.jpg", &a, &c),
        SelfCollision::Different
    );
    // A shared inode under a genuinely different name is a hard link we'd replace.
    assert_eq!(
        classify(true, "/d/cat.jpg", "/d/dog.jpg", &a, &b),
        SelfCollision::Different
    );

    // No inodes (MTP/SMB/SFTP), same volume, case-only difference ⇒ the listing
    // decides; folding alone is never proof (#511).
    let x = entry("dog.JPG", None, Some(1), Some(MT));
    let y = entry("dog.jpg", None, Some(1), Some(MT));
    assert_eq!(
        classify(true, "/d/dog.JPG", "/d/dog.jpg", &x, &y),
        SelfCollision::AskTheListings(vec![twin("/d", "dog.JPG", "dog.jpg")])
    );
    // A normalization-only difference, and a case-only folder in the middle.
    assert_eq!(
        classify(true, "/Cafe\u{0301}.txt", "/Café.txt", &x, &y),
        SelfCollision::AskTheListings(vec![twin("/", "Cafe\u{0301}.txt", "Café.txt")])
    );
    assert_eq!(
        classify(true, "/srv/media/a.txt", "/srv/Media/a.txt", &x, &y),
        SelfCollision::AskTheListings(vec![twin("/srv", "media", "Media")])
    );
    // A remote root keeps its `scheme://` in the parent the listing asks for.
    assert_eq!(
        classify(true, "sftp://nas/srv/img.jpg", "sftp://nas/srv/IMG.JPG", &x, &y),
        SelfCollision::AskTheListings(vec![twin("sftp://nas/srv", "img.jpg", "IMG.JPG")])
    );
    // No inodes, DIFFERENT volume, same relative path ⇒ NOT self (the occupant is a
    // genuinely different file — a move-back must never overwrite it).
    assert_eq!(classify(false, "/a.txt", "/a.txt", &x, &y), SelfCollision::Different);
    // No inodes, same volume, genuinely different sibling ⇒ not self.
    assert_eq!(
        classify(true, "/d/cat.jpg", "/d/dog.jpg", &x, &y),
        SelfCollision::Different
    );
}

#[test]
fn a_listing_confirms_a_twin_only_when_it_holds_just_the_landed_spelling() {
    let twin = SpellingTwin {
        parent: PathBuf::from("/d"),
        from_name: "img.jpg".to_string(),
        to_name: "IMG.JPG".to_string(),
    };
    let listing = |names: &[&str]| -> Vec<FileEntry> { names.iter().map(|n| entry(n, None, None, None)).collect() };
    // Case-insensitive volume: only the landed spelling is stored.
    assert!(listing_confirms_twin(&listing(&["img.jpg", "other"]), &twin));
    // Case-sensitive volume holding both: two entries.
    assert!(!listing_confirms_twin(&listing(&["img.jpg", "IMG.JPG"]), &twin));
    // Only the target spelling, or neither: nothing proves it's one entry.
    assert!(!listing_confirms_twin(&listing(&["IMG.JPG"]), &twin));
    assert!(!listing_confirms_twin(&listing(&[]), &twin));
}

#[test]
fn pure_mapping_helpers() {
    assert_eq!(inverse_kind(OpKind::Copy), OpKind::Delete);
    assert_eq!(inverse_kind(OpKind::CreateFolder), OpKind::Delete);
    assert_eq!(inverse_kind(OpKind::ArchiveEdit), OpKind::Delete);
    assert_eq!(inverse_kind(OpKind::Move), OpKind::Move);
    assert_eq!(inverse_kind(OpKind::Trash), OpKind::Move);
    assert_eq!(inverse_kind(OpKind::Rename), OpKind::Rename);

    assert_eq!(
        inverse_action(OpKind::Copy, EntryType::File),
        Some(InverseAction::RemoveFileIfUnchanged)
    );
    assert_eq!(
        inverse_action(OpKind::Copy, EntryType::Dir),
        Some(InverseAction::RemoveDirIfEmpty)
    );
    assert_eq!(
        inverse_action(OpKind::CreateFolder, EntryType::Dir),
        Some(InverseAction::RemoveDirIfEmpty)
    );
    assert_eq!(
        inverse_action(OpKind::Move, EntryType::File),
        Some(InverseAction::RestoreMove)
    );
    assert_eq!(inverse_action(OpKind::Delete, EntryType::File), None);

    assert_eq!(resolve_final_state(3, 0, false), RollbackState::RolledBack);
    assert_eq!(resolve_final_state(2, 1, false), RollbackState::PartiallyRolledBack);
    // Non-canceled, all skipped ⇒ partial (won't clear on retry).
    assert_eq!(resolve_final_state(0, 3, false), RollbackState::PartiallyRolledBack);
    // Canceled with nothing reversed ⇒ back to rollbackable (a clean retry).
    assert_eq!(resolve_final_state(0, 0, true), RollbackState::Rollbackable);
    // Canceled after reversing some ⇒ partial.
    assert_eq!(resolve_final_state(3, 0, true), RollbackState::PartiallyRolledBack);
}
