//! Selector resolution and the freeze it produces.

use std::cell::RefCell;

use rusqlite::Connection;

use super::*;
use crate::agent::store::proposals::{
    AcceptanceOutcome, ClaimOutcome, GroupIntent, NewSweep, get_group, page_ops, record_acceptance,
};
use crate::agent::store::{MIGRATIONS, run_migrations};
use crate::location::Location;

/// A drive index that answers from a fixed list, and counts how often it's asked. The count is
/// what proves freeze-at-creation: a selector that got re-resolved would ask twice.
struct FakeIndex {
    files: RefCell<Vec<IndexedFile>>,
    refusal: Option<SelectorRefusal>,
    calls: RefCell<u32>,
}

impl FakeIndex {
    fn holding(paths: &[&str]) -> Self {
        Self {
            files: RefCell::new(paths.iter().map(|p| file(p)).collect()),
            refusal: None,
            calls: RefCell::new(0),
        }
    }

    fn refusing(refusal: SelectorRefusal) -> Self {
        Self {
            files: RefCell::new(Vec::new()),
            refusal: Some(refusal),
            calls: RefCell::new(0),
        }
    }
}

impl SelectorIndex for FakeIndex {
    fn resolve(&self, _selector: &OpSelector) -> Result<Vec<IndexedFile>, SelectorRefusal> {
        *self.calls.borrow_mut() += 1;
        match &self.refusal {
            Some(refusal) => Err(refusal.clone()),
            None => Ok(self.files.borrow().clone()),
        }
    }
}

fn file(path: &str) -> IndexedFile {
    IndexedFile {
        path: path.to_string(),
        size: Some(1_234),
        modified_at: Some(1_700_000_000),
        inode: Some(42),
    }
}

fn migrated_conn() -> Connection {
    let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
    conn.execute_batch("PRAGMA foreign_keys = ON;").expect("pragma");
    run_migrations(&conn, MIGRATIONS).expect("migrate");
    conn
}

fn downloads_dmgs() -> OpSelector {
    OpSelector {
        root: Location {
            volume_id: "root".to_string(),
            path: "/Users/someone/Downloads".to_string(),
        },
        name_glob: Some("*.dmg".to_string()),
        min_size: None,
        max_size: None,
        modified_before: Some(1_700_000_000),
        modified_after: None,
    }
}

/// A selector resolves to concrete ops at CREATION, and is never resolved again. Changing what
/// the index would answer after the group exists changes nothing about the group: what the
/// user saw is what runs.
#[test]
fn a_selector_freezes_at_creation_and_is_never_resolved_again() {
    let conn = migrated_conn();
    let index = FakeIndex::holding(&["/Users/someone/Downloads/one.dmg", "/Users/someone/Downloads/two.dmg"]);
    let selector = downloads_dmgs();

    let ops = resolve_selector_ops(&index, &selector).expect("resolve");
    let group = selector_group(
        &selector,
        GroupIntent::Trash { sources: ops },
        Some("You installed both of these months ago.".to_string()),
    )
    .expect("build group");
    let proposed = propose(&conn, &NewSweep::default(), std::slice::from_ref(&group), 100).expect("propose");
    let group_id = proposed.group_ids[0];

    // The drive gains two more matching files after the proposal was made.
    index
        .files
        .borrow_mut()
        .push(file("/Users/someone/Downloads/three.dmg"));
    index.files.borrow_mut().push(file("/Users/someone/Downloads/four.dmg"));

    let AcceptanceOutcome::Accepted { binding } = record_acceptance(&conn, group_id, &[], 200).expect("preflight")
    else {
        panic!("preflight refused");
    };
    assert_eq!(binding.op_count, 2, "the frozen list is what gets approved");
    let outcome = approve(&conn, group_id, 300).expect("approve");
    assert!(matches!(outcome, ClaimOutcome::Claimed(_)), "{outcome:?}");

    let frozen = page_ops(&conn, group_id, 100, 0).expect("ops");
    assert_eq!(
        frozen.iter().map(|op| op.source_path.as_str()).collect::<Vec<_>>(),
        ["/Users/someone/Downloads/one.dmg", "/Users/someone/Downloads/two.dmg"]
    );
    assert_eq!(
        *index.calls.borrow(),
        1,
        "the selector is resolved exactly once, at creation — never at approval"
    );
}

// ── What the user's answer teaches the agent ──────────────────────────────────

/// A group with a real sweep behind it, and a memory store to learn into.
fn a_trashable_group(conn: &Connection) -> i64 {
    let index = FakeIndex::holding(&["/Users/someone/Downloads/one.dmg"]);
    let selector = downloads_dmgs();
    let ops = resolve_selector_ops(&index, &selector).expect("resolve");
    let group = selector_group(&selector, GroupIntent::Trash { sources: ops }, None).expect("build group");
    propose(conn, &NewSweep::default(), std::slice::from_ref(&group), 100)
        .expect("propose")
        .group_ids[0]
}

fn memory_store() -> (tempfile::TempDir, MemoryStore) {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().join("memory");
    std::fs::create_dir_all(&root).expect("root");
    let store = MemoryStore::new(root);
    (dir, store)
}

fn remembered(store: &MemoryStore) -> String {
    std::fs::read_to_string(store.root().join(crate::agent::memory::OUTCOMES_FILE)).unwrap_or_default()
}

/// A "no" in the review is a judgment about the proposal, and the whole point of M4 is that the
/// agent hears it. The lesson goes into the memory ring with no model call, so an approval and
/// a rejection teach it equally.
#[test]
fn saying_no_in_the_review_teaches_the_agent() {
    let conn = migrated_conn();
    let (_dir, memory) = memory_store();
    let group_id = a_trashable_group(&conn);

    let outcome = reject(&conn, group_id, 200, RejectSource::Review, Some(&memory)).expect("reject");

    assert!(matches!(outcome, RejectOutcome::Rejected), "{outcome:?}");
    let learned = remembered(&memory);
    assert!(learned.contains("turned down"), "nothing was learned: {learned:?}");
    assert!(learned.contains("trash"), "the verb is part of the lesson: {learned:?}");
}

/// ⚠️ **Escape on a rename review is recorded as a rejection, and it is NOT one.**
/// `cancel_bulk_rename_proposal` calls the same store transition, but the user expressed no
/// opinion by closing a window. Learning from it would teach the agent something nobody said,
/// and the follow-up turn it would ask for lands in whatever thread the user has open, because
/// that sweep's `conversation_id` is the RAIL conversation.
#[test]
fn closing_a_dialog_teaches_the_agent_nothing() {
    let conn = migrated_conn();
    let (_dir, memory) = memory_store();
    let group_id = a_trashable_group(&conn);

    let outcome = reject(&conn, group_id, 200, RejectSource::DialogDismissed, Some(&memory)).expect("reject");

    assert!(
        matches!(outcome, RejectOutcome::Rejected),
        "the group still gets its answer: {outcome:?}"
    );
    assert_eq!(
        remembered(&memory),
        "",
        "a dismissed dialog is not an opinion, so there is nothing to remember"
    );
}

/// The user's own half of the record. ⚠️ A `ConversationEvent` alone would teach the agent
/// NOTHING (they never enter the LLM transcript), which is why the memory ring exists beside
/// it — but a lesson the user can't see in the thread they got the suggestion in is just as
/// wrong in the other direction.
#[test]
fn a_decision_shows_up_in_the_thread_that_suggested_it() {
    let conn = migrated_conn();
    let conversation_id = crate::agent::store::create_conversation(&conn, "Downloads", 50, None).expect("thread");
    let index = FakeIndex::holding(&["/Users/someone/Downloads/one.dmg"]);
    let selector = downloads_dmgs();
    let ops = resolve_selector_ops(&index, &selector).expect("resolve");
    let group = selector_group(&selector, GroupIntent::Trash { sources: ops }, None).expect("build group");
    let sweep = NewSweep {
        conversation_id: Some(conversation_id),
        ..NewSweep::default()
    };
    let group_id = propose(&conn, &sweep, std::slice::from_ref(&group), 100)
        .expect("propose")
        .group_ids[0];

    reject(&conn, group_id, 200, RejectSource::Review, None).expect("reject");

    let timeline = crate::agent::store::list_messages(&conn, conversation_id, 100, 0).expect("messages");
    let decided = timeline.iter().find_map(|message| match &message.content {
        crate::agent::store::StoredContent::Event(crate::agent::store::ConversationEvent::ProposalDecided {
            decision,
        }) => Some(decision),
        _ => None,
    });
    let decided = decided.expect("the thread says what the user answered");
    assert_eq!(decided.verb, crate::agent::types::ProposalVerb::Trash);
    assert_eq!(decided.ops, 1);
}

/// The transition is conditional, so the hook fires once per group however many times the
/// button is pressed. Without that, a double click would double the lesson's weight.
#[test]
fn a_second_rejection_of_the_same_group_teaches_nothing_more() {
    let conn = migrated_conn();
    let (_dir, memory) = memory_store();
    let group_id = a_trashable_group(&conn);

    reject(&conn, group_id, 200, RejectSource::Review, Some(&memory)).expect("reject");
    let after_first = remembered(&memory);
    reject(&conn, group_id, 300, RejectSource::Review, Some(&memory)).expect("reject again");

    assert_eq!(remembered(&memory), after_first);
}

/// The index snapshot rides onto the op rows, so the executor can tell at apply time whether
/// the file is still the one the user reviewed.
#[test]
fn resolved_ops_carry_the_creation_snapshot() {
    let index = FakeIndex::holding(&["/Users/someone/Downloads/one.dmg"]);
    let ops = resolve_selector_ops(&index, &downloads_dmgs()).expect("resolve");

    let snapshot = ops[0].snapshot.expect("a resolved op carries what the index knew");
    assert_eq!(snapshot.size, Some(1_234));
    assert_eq!(snapshot.mtime, Some(1_700_000_000));
    assert_eq!(snapshot.inode, Some(42));
}

/// The pattern survives on the group as display text, and the selector itself as JSON: the
/// dialog leads with the pattern and expands to the resolved list.
#[test]
fn the_group_carries_the_pattern_as_text_and_the_selector_as_json() {
    let conn = migrated_conn();
    let index = FakeIndex::holding(&["/Users/someone/Downloads/one.dmg"]);
    let selector = downloads_dmgs();
    let ops = resolve_selector_ops(&index, &selector).expect("resolve");
    let group = selector_group(&selector, GroupIntent::Trash { sources: ops }, None).expect("group");

    let proposed = propose(&conn, &NewSweep::default(), &[group], 100).expect("propose");
    let stored = get_group(&conn, proposed.group_ids[0]).expect("read").expect("exists");

    assert_eq!(stored.display_name, "/Users/someone/Downloads/*.dmg");
    let round_tripped: OpSelector =
        serde_json::from_str(&stored.selector.expect("the selector is stored")).expect("valid JSON");
    assert_eq!(
        round_tripped, selector,
        "the selector round-trips for the dialog to render"
    );
}

/// A volume with no live index refuses, rather than resolving to an empty list. "I can't see
/// that drive" and "nothing matched" are different answers, and only one of them is honest.
#[test]
fn an_unindexed_volume_refuses_rather_than_resolving_to_nothing() {
    let index = FakeIndex::refusing(SelectorRefusal::NotIndexed {
        volume_id: "smb-nas".to_string(),
    });

    let refusal = resolve_selector_ops(&index, &downloads_dmgs()).expect_err("must refuse");
    assert!(matches!(refusal, SelectorRefusal::NotIndexed { .. }), "{refusal:?}");
}

/// A selector with no glob names its whole root, and says so without inventing prose.
#[test]
fn a_selector_without_a_glob_reads_as_its_root() {
    let selector = OpSelector {
        name_glob: None,
        ..downloads_dmgs()
    };
    assert_eq!(selector.pattern_text(), "/Users/someone/Downloads/");
}

// ── The production resolver, against a real index DB ─────────────────────────────

/// One entry in a synthetic index: a file under `dir`, with what the index knows about it.
struct IndexRow {
    dir: &'static str,
    name: &'static str,
    is_symlink: bool,
    size: Option<u64>,
    mtime: Option<u64>,
}

/// An ordinary indexed file.
fn row(dir: &'static str, name: &'static str, size: u64, mtime: u64) -> IndexRow {
    IndexRow {
        dir,
        name,
        is_symlink: false,
        size: Some(size),
        mtime: Some(mtime),
    }
}

/// A symlink, which resolution must skip.
fn symlink_row(dir: &'static str, name: &'static str) -> IndexRow {
    IndexRow {
        is_symlink: true,
        ..row(dir, name, 1, 1)
    }
}

/// Build a tiny root index at `path`, creating each row's parent chain on the way.
fn build_index(path: &std::path::Path, rows: &[IndexRow]) {
    use cmdr_index::store::{IndexStore, ROOT_ID};
    use std::collections::HashMap;

    let store = IndexStore::open(path).expect("open index");
    let conn = store.read_conn();
    let mut dir_ids: HashMap<String, i64> = HashMap::new();
    let mut next_id = ROOT_ID + 1;

    fn ensure_dir(conn: &Connection, dir: &str, dir_ids: &mut HashMap<String, i64>, next_id: &mut i64) -> i64 {
        use cmdr_index::store::{IndexStore, ROOT_ID};
        if dir.is_empty() || dir == "/" {
            return ROOT_ID;
        }
        if let Some(&id) = dir_ids.get(dir) {
            return id;
        }
        let parent = std::path::Path::new(dir)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let parent_id = ensure_dir(conn, &parent, dir_ids, next_id);
        let name = std::path::Path::new(dir)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let id = *next_id;
        *next_id += 1;
        IndexStore::insert_entry_v2_with_id(conn, id, parent_id, &name, true, false, None, None, None, None)
            .expect("insert dir");
        dir_ids.insert(dir.to_string(), id);
        id
    }

    for entry in rows {
        let parent_id = ensure_dir(conn, entry.dir, &mut dir_ids, &mut next_id);
        let id = next_id;
        next_id += 1;
        IndexStore::insert_entry_v2_with_id(
            conn,
            id,
            parent_id,
            entry.name,
            false,
            entry.is_symlink,
            entry.size,
            entry.size,
            entry.mtime,
            None,
        )
        .expect("insert entry");
    }
}

/// The production resolver reads the real drive index: it descends the whole subtree, keeps
/// only what the predicates accept, carries each hit's size and date, and stops at the root
/// the selector named.
#[test]
fn the_drive_index_resolves_a_selector_over_a_real_index() {
    let _pool_guard = cmdr_index::test_read_pool_lock();
    let dir = tempfile::tempdir().expect("temp dir");
    let index_path = dir.path().join("index-root.db");
    build_index(
        &index_path,
        &[
            // Two matches, one of them a directory level down: a selector covers the subtree.
            row("/Users/someone/Downloads", "one.dmg", 5_000, 1_000),
            row("/Users/someone/Downloads/older", "two.dmg", 9_000, 500),
            // Wrong extension, too new, a symlink, and outside the root: none may match.
            row("/Users/someone/Downloads", "notes.txt", 12, 900),
            row("/Users/someone/Downloads", "fresh.dmg", 7_000, 9_999),
            symlink_row("/Users/someone/Downloads", "link.dmg"),
            row("/Users/someone/Documents", "elsewhere.dmg", 4_000, 100),
        ],
    );
    cmdr_index::test_install_root_read_pool(index_path).expect("install the read pool");

    let resolved = DriveIndex.resolve(&OpSelector {
        root: Location {
            volume_id: cmdr_index::ROOT_VOLUME_ID.to_string(),
            path: "/Users/someone/Downloads".to_string(),
        },
        name_glob: Some("*.dmg".to_string()),
        min_size: None,
        max_size: None,
        modified_before: Some(5_000),
        modified_after: None,
    });
    cmdr_index::test_uninstall_root_read_pool();

    let found = resolved.expect("the root is indexed");
    assert_eq!(
        found.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        [
            "/Users/someone/Downloads/older/two.dmg",
            "/Users/someone/Downloads/one.dmg"
        ],
        "the subtree is covered, and nothing outside the root, too new, misnamed, or symlinked is"
    );
    assert_eq!(found[1].size, Some(5_000), "the index's own facts ride along");
    assert_eq!(found[1].modified_at, Some(1_000));
}

/// A root the index has never heard of refuses with `RootNotFound`, which is a different
/// answer from "nothing in there matched" and reaches the user as one.
#[test]
fn a_root_the_index_doesnt_hold_refuses_rather_than_matching_nothing() {
    let _pool_guard = cmdr_index::test_read_pool_lock();
    let dir = tempfile::tempdir().expect("temp dir");
    let index_path = dir.path().join("index-root.db");
    build_index(&index_path, &[row("/Users/someone/Downloads", "one.dmg", 1, 1)]);
    cmdr_index::test_install_root_read_pool(index_path).expect("install the read pool");

    let resolved = DriveIndex.resolve(&OpSelector {
        root: Location {
            volume_id: cmdr_index::ROOT_VOLUME_ID.to_string(),
            path: "/Users/someone/Downloadz".to_string(),
        },
        ..downloads_dmgs()
    });
    cmdr_index::test_uninstall_root_read_pool();

    assert!(
        matches!(resolved, Err(SelectorRefusal::RootNotFound { .. })),
        "{resolved:?}"
    );
}
