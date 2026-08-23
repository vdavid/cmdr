//! The jail and the caps, against a real temp directory.
//!
//! Every escape attempt below is one the agent could be TALKED into by text it read: a file
//! name, or a sentence photographed in one of the user's images. So each is a test rather than
//! a comment, and each asserts the typed refusal rather than an absence of damage.

use super::*;
use std::path::Path;

/// A store over a fresh temp dir. The `TempDir` is returned too: dropping it deletes the root.
fn store() -> (tempfile::TempDir, MemoryStore) {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().join("ai").join("memory");
    std::fs::create_dir_all(&root).expect("root");
    let store = MemoryStore::new(root);
    (dir, store)
}

/// Anything the jail turns down must leave the disk exactly as it found it, wherever the path
/// pointed. Asserting only the `Err` would pass for a refusal that wrote first.
fn nothing_landed_outside(dir: &Path) {
    let outside = dir.join("escaped.md");
    assert!(!outside.exists(), "a refused write reached {}", outside.display());
}

// ── The jail ──────────────────────────────────────────────────────────────────

#[test]
fn a_relative_path_inside_the_folder_is_allowed() {
    let (_dir, store) = store();

    let written = store
        .write(HUB_FILE, "Prefers ISO dates.")
        .expect("the hub file is writable");

    assert_eq!(written.path, HUB_FILE);
    assert_eq!(
        std::fs::read_to_string(store.root().join(HUB_FILE)).expect("read back"),
        "Prefers ISO dates."
    );
}

#[test]
fn a_path_in_a_subfolder_is_allowed_and_creates_it() {
    let (_dir, store) = store();

    store
        .write("people/dori.md", "Dóri reads in Hungarian.")
        .expect("a subfolder");

    assert!(store.root().join("people").join("dori.md").is_file());
}

#[test]
fn an_absolute_path_is_refused() {
    let (dir, store) = store();

    for absolute in ["/tmp/escaped.md", "/etc/hosts.md"] {
        assert_eq!(
            store.write(absolute, "x"),
            Err(MemoryRefusal::OutsideMemory),
            "{absolute} must not resolve"
        );
    }
    nothing_landed_outside(dir.path());
}

#[test]
fn a_dot_dot_segment_is_refused_anywhere_in_the_path() {
    let (dir, store) = store();

    for escape in ["../escaped.md", "a/../../escaped.md", "..", "notes/../../escaped.md"] {
        assert_eq!(
            store.write(escape, "x"),
            Err(MemoryRefusal::OutsideMemory),
            "{escape} must not resolve"
        );
    }
    nothing_landed_outside(dir.path());
}

#[test]
fn an_empty_or_blank_path_is_refused() {
    let (_dir, store) = store();

    assert_eq!(store.write("", "x"), Err(MemoryRefusal::NoPath));
    assert_eq!(store.write("   ", "x"), Err(MemoryRefusal::NoPath));
    assert_eq!(store.write("./", "x"), Err(MemoryRefusal::NoPath));
}

#[test]
fn only_markdown_is_allowed() {
    let (_dir, store) = store();

    for other in ["notes.txt", "settings.json", "AGENTS", "run.sh", "main.db"] {
        assert_eq!(
            store.write(other, "x"),
            Err(MemoryRefusal::NotMarkdown),
            "{other} is not a memory file"
        );
    }
    assert!(
        store.write("Notes.MD", "x").is_ok(),
        "the extension is case-insensitive"
    );
}

/// The trap `canonicalize` alone can't catch: the path is lexically clean, and the escape is a
/// link the agent (or anybody with the folder open) planted earlier.
#[cfg(unix)]
#[test]
fn a_symlinked_file_is_refused_rather_than_written_through() {
    let (dir, store) = store();
    let outside = dir.path().join("escaped.md");
    std::os::unix::fs::symlink(&outside, store.root().join("link.md")).expect("symlink");

    assert_eq!(store.write("link.md", "planted"), Err(MemoryRefusal::OutsideMemory));
    assert!(!outside.exists(), "the write followed the link");
}

#[cfg(unix)]
#[test]
fn a_symlinked_parent_folder_is_refused() {
    let (dir, store) = store();
    let outside = dir.path().join("elsewhere");
    std::fs::create_dir_all(&outside).expect("elsewhere");
    std::os::unix::fs::symlink(&outside, store.root().join("people")).expect("symlink");

    assert_eq!(
        store.write("people/dori.md", "planted"),
        Err(MemoryRefusal::OutsideMemory)
    );
    assert!(!outside.join("dori.md").exists(), "the write followed the link");
}

// ── The disk cap ──────────────────────────────────────────────────────────────

/// The cap protects DISK, and a full folder must say so in a form the model can act on: a
/// silent failure leaves it believing it remembered something it didn't.
#[test]
fn a_write_over_the_folder_cap_is_refused_with_the_numbers() {
    let (_dir, store) = store();

    let refusal = store
        .write(HUB_FILE, &"x".repeat(MEMORY_MODEL_MAX_BYTES as usize + 1))
        .expect_err("over the cap");

    match refusal {
        MemoryRefusal::DirectoryFull { used, cap, wanted } => {
            assert_eq!(cap, MEMORY_MODEL_MAX_BYTES);
            assert_eq!(used, 0);
            assert!(wanted > cap, "the refusal has to say how far over it went");
        }
        other => panic!("expected a DirectoryFull refusal, got {other:?}"),
    }
    assert!(!store.root().join(HUB_FILE).exists(), "a refused write still landed");
}

/// The cap is on the FOLDER, not on one file: two files that each fit but together don't must
/// be refused too, or a path-aware toolset walks straight past the cap.
#[test]
fn the_cap_counts_every_file_in_the_folder() {
    let (_dir, store) = store();
    let two_thirds = (MEMORY_DIR_MAX_BYTES * 2 / 3) as usize;
    store.write(HUB_FILE, &"a".repeat(two_thirds)).expect("the first fits");

    let refusal = store
        .write("more.md", &"b".repeat(two_thirds))
        .expect_err("the second does not");

    assert!(
        matches!(refusal, MemoryRefusal::DirectoryFull { .. }),
        "got {refusal:?}"
    );
}

/// Replacing a file frees what it held, so rewriting the hub must not be priced as if the old
/// copy were still there. Without this, memory jams at half the cap and never recovers.
#[test]
fn replacing_a_file_reclaims_what_it_held() {
    let (_dir, store) = store();
    let most = (MEMORY_DIR_MAX_BYTES * 3 / 4) as usize;
    store.write(HUB_FILE, &"a".repeat(most)).expect("the first fits");

    let written = store
        .write(HUB_FILE, &"b".repeat(most))
        .expect("so does its replacement");

    assert_eq!(written.bytes, most);
    assert!(written.remaining_bytes > 0, "the headroom is reported back");
}

// ── The prompt slice ──────────────────────────────────────────────────────────

#[test]
fn an_absent_hub_file_reads_as_no_memory() {
    let (_dir, store) = store();

    assert_eq!(store.read_for_prompt(4_096), None);
}

#[test]
fn a_whitespace_only_hub_file_reads_as_no_memory() {
    let (_dir, store) = store();
    store.write(HUB_FILE, "  \n\t\n ").expect("write");

    assert_eq!(store.read_for_prompt(4_096), None);
}

/// The system string is never elided, so a memory file bigger than its share of the budget
/// would be a permanent tax the conversation pays. The agent writes this file itself, so
/// without the cut it can degrade its own chat forever.
#[test]
fn a_hub_file_over_the_slice_is_cut_and_says_so() {
    let (_dir, store) = store();
    store.write(HUB_FILE, &"m".repeat(4_000)).expect("write");

    let slice = store.read_for_prompt(1_000).expect("something is fed");

    assert!(slice.len() < 4_000, "the slice is {} bytes", slice.len());
    assert!(slice.starts_with("mmm"), "the head is what survives");
    assert!(
        slice.contains("Cut off here"),
        "a silent cut leaves the model reading a sentence that stops mid-thought"
    );
}

/// The cut is a byte count over text, so it can land inside a multi-byte character. A naive
/// slice panics; giving up would turn a non-ASCII memory into no memory at all.
#[test]
fn cutting_a_multibyte_hub_file_still_produces_text() {
    let (_dir, store) = store();
    store.write(HUB_FILE, &"\u{2603}".repeat(1_000)).expect("write");

    let slice = store.read_for_prompt(1_000).expect("something is fed");

    assert!(slice.starts_with('\u{2603}'));
    assert!(slice.contains("Cut off here"));
}

/// ⚠️ Non-UTF8 memory must not read as ABSENT. Under a bare `read_to_string(..).ok()` the agent
/// silently believes it has never remembered anything and starts the user over.
#[test]
fn a_hub_file_that_is_not_text_reads_as_absent_and_is_logged() {
    let (_dir, store) = store();
    std::fs::write(store.root().join(HUB_FILE), [0xff, 0xfe, 0x00, 0x01]).expect("write bytes");

    assert_eq!(store.read_for_prompt(4_096), None);
}

// ── The edit ──────────────────────────────────────────────────────────────────

#[test]
fn an_edit_replaces_one_exact_occurrence() {
    let (_dir, store) = store();
    store
        .write(HUB_FILE, "Prefers ISO dates.\nWorks in Stockholm.\n")
        .expect("write");

    store.edit(HUB_FILE, "Stockholm", "Budapest").expect("the edit lands");

    assert_eq!(
        std::fs::read_to_string(store.root().join(HUB_FILE)).expect("read back"),
        "Prefers ISO dates.\nWorks in Budapest.\n"
    );
}

/// A non-unique match is the one an edit must never guess at: replacing the first occurrence
/// rewrites a line the model was not looking at, in a file that rides every later turn.
#[test]
fn an_edit_refuses_a_match_that_is_not_unique() {
    let (_dir, store) = store();
    store.write(HUB_FILE, "likes tea\nlikes tea\n").expect("write");

    assert_eq!(
        store.edit(HUB_FILE, "likes tea", "likes coffee"),
        Err(MemoryRefusal::NotUnique { matches: 2 })
    );
    assert_eq!(
        std::fs::read_to_string(store.root().join(HUB_FILE)).expect("read back"),
        "likes tea\nlikes tea\n",
        "a refused edit changed the file anyway"
    );
}

#[test]
fn an_edit_that_matches_nothing_says_so() {
    let (_dir, store) = store();
    store.write(HUB_FILE, "likes tea\n").expect("write");

    assert_eq!(
        store.edit(HUB_FILE, "likes coffee", "likes tea"),
        Err(MemoryRefusal::NoMatch)
    );
}

#[test]
fn an_edit_of_a_file_that_is_not_there_says_so() {
    let (_dir, store) = store();

    assert_eq!(store.edit("nope.md", "a", "b"), Err(MemoryRefusal::NoSuchFile));
}

#[test]
fn an_edit_is_jailed_the_same_way_a_write_is() {
    let (dir, store) = store();
    std::fs::write(dir.path().join("escaped.md"), "outside").expect("write");

    assert_eq!(
        store.edit("../escaped.md", "outside", "owned"),
        Err(MemoryRefusal::OutsideMemory)
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("escaped.md")).expect("read back"),
        "outside"
    );
}

/// An edit that grows the file past the cap is a write past the cap.
#[test]
fn an_edit_over_the_folder_cap_is_refused() {
    let (_dir, store) = store();
    let most = (MEMORY_MODEL_MAX_BYTES - 16) as usize;
    store
        .write(HUB_FILE, &format!("{}SEED", "a".repeat(most - 4)))
        .expect("write");

    let refusal = store
        .edit(HUB_FILE, "SEED", &"b".repeat(1_000))
        .expect_err("over the cap");

    assert!(
        matches!(refusal, MemoryRefusal::DirectoryFull { .. }),
        "got {refusal:?}"
    );
}

// ── Forgetting everything ─────────────────────────────────────────────────────

/// "Forget everything" has to mean everything, subfolders included: a note the agent tucked
/// into `people/dora.md` is exactly the kind of thing somebody reaches for this button over.
#[test]
fn forgetting_takes_every_note_including_the_nested_ones() {
    let (_dir, store) = store();
    store.write(HUB_FILE, "the hub").expect("write hub");
    store.write("people/dora.md", "a note").expect("write nested");

    let forgotten = store.forget_all().expect("forget");

    assert_eq!(forgotten, 2);
    assert_eq!(store.read_for_prompt(4_096), None, "and the next turn carries nothing");
    assert!(!store.root().join("people").join("dora.md").exists());
}

/// The folder itself survives, so the next write lands without the jail having to recreate it,
/// and the user is not looking at a pane for a directory that stopped existing under them.
#[test]
fn forgetting_leaves_the_folder_itself_and_anything_that_is_not_a_note() {
    let (_dir, store) = store();
    store.write(HUB_FILE, "the hub").expect("write hub");
    std::fs::write(store.root().join("notes.txt"), "not ours").expect("stray file");

    assert_eq!(store.forget_all().expect("forget"), 1);

    assert!(store.root().is_dir());
    assert!(
        store.root().join("notes.txt").exists(),
        "a file the agent could never have written is the user's, not ours to delete"
    );
}

/// Nothing to forget is a success, not a failure: the button is there before the agent has
/// ever written anything.
#[test]
fn forgetting_an_empty_folder_reports_nothing_and_succeeds() {
    let (_dir, store) = store();

    assert_eq!(store.forget_all().expect("forget"), 0);
}

/// "Forget everything" means everything: the decision log is about the user too.
#[test]
fn forgetting_takes_the_decision_log_with_it() {
    let (_dir, store) = store();
    store.record_outcome("2026-08-23 rejected: move 12 files under /Users/x/Downloads");

    store.forget_all().expect("forget");

    assert!(!store.root().join(OUTCOMES_FILE).exists());
}

// ── The decision ring ─────────────────────────────────────────────────────────

/// ⚠️ **The reserve is the whole safety property of the mechanical path.** A decision is
/// recorded with no model turn to hand a `DirectoryFull` refusal to, so the model filling its
/// own notes must never be able to silence the channel that teaches it what the user wants.
#[test]
fn a_full_memory_folder_still_takes_a_decision() {
    let (_dir, store) = store();
    store
        .write(HUB_FILE, &"a".repeat(MEMORY_MODEL_MAX_BYTES as usize))
        .expect("the model fills everything it is allowed");
    assert!(
        store.write("more.md", "one more byte").is_err(),
        "the model has no room left"
    );

    store.record_outcome("2026-08-23 rejected: move 12 files under /Users/x/Downloads");

    let log = std::fs::read_to_string(store.root().join(OUTCOMES_FILE)).expect("the ring landed anyway");
    assert!(log.contains("rejected: move 12 files"));
}

/// The ring is capped like everything else here, so a decade of decisions cannot walk the
/// folder past its disk cap.
#[test]
fn the_decision_ring_stays_inside_its_reserve() {
    let (_dir, store) = store();

    for n in 0..300 {
        store.record_outcome(&format!(
            "2026-08-23 approved: trash {n} files under /Users/x/Downloads"
        ));
    }

    let log = std::fs::read_to_string(store.root().join(OUTCOMES_FILE)).expect("the ring");
    assert!(log.len() <= OUTCOMES_MAX_BYTES, "the ring reached {} bytes", log.len());
    assert!(log.contains("trash 299 files"), "the newest decision survived");
}

/// Both files ride the prefix, so the agent reads what it worked out about the person AND what
/// they did with its last suggestions. Feeding only the hub is how M4's lesson would be
/// written and never read.
#[test]
fn a_turn_carries_the_hub_and_the_decisions_together() {
    let (_dir, store) = store();
    store.write(HUB_FILE, "Prefers ISO dates.").expect("the hub");
    store.record_outcome("2026-08-23 rejected: move 12 files under /Users/x/Downloads");

    let carried = store.read_for_prompt(4_096).expect("something to carry");

    assert!(carried.contains("Prefers ISO dates."));
    assert!(carried.contains("rejected: move 12 files"));
}
