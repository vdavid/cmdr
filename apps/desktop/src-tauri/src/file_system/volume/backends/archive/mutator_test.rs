//! Tests for the `ArchiveMutator` (temp+rename zip editing).
//!
//! Data-safety-critical, so these are the red-first TDD anchors for every
//! invariant: round-trips (verified via OUR reader AND external `unzip -t`),
//! cancel-leaves-original-intact, the merge invariant, metadata preservation,
//! and encrypted-entry byte-for-byte retention.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use super::super::test_fixtures::{build_zip, deflated, dir, stored};
use super::*;

/// Writes `bytes` to `dir/name.zip` and returns the path.
fn write_zip(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(format!("{name}.zip"));
    std::fs::write(&path, bytes).expect("write fixture zip");
    path
}

/// Reads an archive back via the `zip` crate into a `name -> contents` map.
/// Directory entries (trailing slash) map to empty bytes. Panics if the archive
/// can't be opened — a mutator that corrupted the file fails loudly here.
fn read_back(path: &Path) -> HashMap<String, Vec<u8>> {
    let file = File::open(path).expect("open result archive");
    let mut archive = ZipArchive::new(file).expect("result archive parses");
    let mut out = HashMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).expect("read entry");
        let name = entry.name().to_string();
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).expect("read entry bytes");
        out.insert(name, buf);
    }
    out
}

/// Runs `unzip -t` (integrity test) on the archive. Returns whether the external
/// tool accepts it. Skips (returns `true`) if `unzip` isn't installed.
fn unzip_accepts(path: &Path) -> bool {
    match Command::new("unzip").arg("-t").arg(path).output() {
        Ok(output) => output.status.success(),
        Err(_) => true, // `unzip` not on PATH (e.g. minimal CI image): don't fail the test.
    }
}

fn add_bytes(inner_path: &str, content: &[u8]) -> AddEntry {
    AddEntry {
        inner_path: inner_path.to_string(),
        source: AddSource::Bytes(content.to_vec()),
    }
}

// ---- Round-trips --------------------------------------------------------------

#[test]
fn add_streams_a_new_entry_and_keeps_the_rest() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = write_zip(
        tmp.path(),
        "a",
        &build_zip(&[
            stored("keep.txt", b"keep me".to_vec()),
            deflated("also.txt", b"also".to_vec()),
        ]),
    );

    let changeset = Changeset {
        adds: vec![add_bytes("new/added.txt", b"fresh bytes")],
        ..Default::default()
    };
    apply(&path, &changeset, &NoHooks).expect("apply add");

    let back = read_back(&path);
    assert_eq!(back.get("keep.txt").map(|v| v.as_slice()), Some(b"keep me".as_slice()));
    assert_eq!(back.get("also.txt").map(|v| v.as_slice()), Some(b"also".as_slice()));
    assert_eq!(
        back.get("new/added.txt").map(|v| v.as_slice()),
        Some(b"fresh bytes".as_slice())
    );
    assert!(unzip_accepts(&path), "external unzip must accept the edited archive");
}

#[test]
fn delete_drops_one_entry_and_keeps_siblings_byte_identical() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = write_zip(
        tmp.path(),
        "a",
        &build_zip(&[
            stored("one.txt", b"one".to_vec()),
            deflated("two.txt", b"two content that deflates".to_vec()),
            stored("three.txt", b"three".to_vec()),
        ]),
    );

    let changeset = Changeset {
        deletes: vec!["two.txt".to_string()],
        ..Default::default()
    };
    apply(&path, &changeset, &NoHooks).expect("apply delete");

    let back = read_back(&path);
    assert!(!back.contains_key("two.txt"), "deleted entry is gone");
    assert_eq!(back.get("one.txt").map(|v| v.as_slice()), Some(b"one".as_slice()));
    assert_eq!(back.get("three.txt").map(|v| v.as_slice()), Some(b"three".as_slice()));
    assert!(unzip_accepts(&path));
}

#[test]
fn delete_of_a_directory_drops_the_whole_subtree() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = write_zip(
        tmp.path(),
        "a",
        &build_zip(&[
            stored("keep.txt", b"keep".to_vec()),
            stored("sub/a.txt", b"a".to_vec()),
            stored("sub/deep/b.txt", b"b".to_vec()),
            dir("sub/"),
        ]),
    );

    let changeset = Changeset {
        deletes: vec!["sub".to_string()],
        ..Default::default()
    };
    apply(&path, &changeset, &NoHooks).expect("apply subtree delete");

    let back = read_back(&path);
    assert_eq!(back.get("keep.txt").map(|v| v.as_slice()), Some(b"keep".as_slice()));
    assert!(
        back.keys().all(|k| !k.starts_with("sub")),
        "subtree fully removed: {:?}",
        back.keys()
    );
    assert!(unzip_accepts(&path));
}

#[test]
fn rename_moves_a_file_and_a_subtree_prefix() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = write_zip(
        tmp.path(),
        "a",
        &build_zip(&[
            stored("old.txt", b"file".to_vec()),
            stored("olddir/x.txt", b"x".to_vec()),
            stored("olddir/deep/y.txt", b"y".to_vec()),
        ]),
    );

    let changeset = Changeset {
        renames: vec![
            ("old.txt".to_string(), "new.txt".to_string()),
            ("olddir".to_string(), "newdir".to_string()),
        ],
        ..Default::default()
    };
    apply(&path, &changeset, &NoHooks).expect("apply rename");

    let back = read_back(&path);
    assert_eq!(back.get("new.txt").map(|v| v.as_slice()), Some(b"file".as_slice()));
    assert!(!back.contains_key("old.txt"));
    assert_eq!(back.get("newdir/x.txt").map(|v| v.as_slice()), Some(b"x".as_slice()));
    assert_eq!(
        back.get("newdir/deep/y.txt").map(|v| v.as_slice()),
        Some(b"y".as_slice())
    );
    assert!(
        back.keys().all(|k| !k.starts_with("olddir")),
        "old prefix gone: {:?}",
        back.keys()
    );
    assert!(unzip_accepts(&path));
}

#[test]
fn mkdir_writes_an_explicit_directory_entry() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = write_zip(tmp.path(), "a", &build_zip(&[stored("keep.txt", b"k".to_vec())]));

    let changeset = Changeset {
        mkdirs: vec!["fresh".to_string()],
        ..Default::default()
    };
    apply(&path, &changeset, &NoHooks).expect("apply mkdir");

    let back = read_back(&path);
    assert!(
        back.contains_key("fresh/"),
        "explicit dir entry present: {:?}",
        back.keys()
    );
    assert!(unzip_accepts(&path));
}

#[test]
fn mkfile_writes_a_zero_byte_entry() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = write_zip(tmp.path(), "a", &build_zip(&[stored("keep.txt", b"k".to_vec())]));

    let changeset = Changeset {
        adds: vec![add_bytes("empty.txt", b"")],
        ..Default::default()
    };
    apply(&path, &changeset, &NoHooks).expect("apply mkfile");

    let back = read_back(&path);
    assert_eq!(back.get("empty.txt").map(|v| v.as_slice()), Some(b"".as_slice()));
    assert!(unzip_accepts(&path));
}

// ---- Data-safety: cancel + crash ---------------------------------------------

/// Hooks that report cancelled once at least one progress tick has landed, and
/// record whether any temp existed while the edit ran.
struct CancelAfterFirstTick {
    ticked: AtomicBool,
}

impl MutationHooks for CancelAfterFirstTick {
    fn is_cancelled(&self) -> bool {
        self.ticked.load(Ordering::SeqCst)
    }
    fn on_progress(&self, _progress: MutationProgress) {
        self.ticked.store(true, Ordering::SeqCst);
    }
}

#[test]
fn cancel_midway_leaves_the_original_intact_and_no_temp_behind() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let original = build_zip(&[
        stored("one.txt", b"one".to_vec()),
        stored("two.txt", b"two".to_vec()),
        stored("three.txt", b"three".to_vec()),
    ]);
    let path = write_zip(tmp.path(), "a", &original);

    let hooks = CancelAfterFirstTick {
        ticked: AtomicBool::new(false),
    };
    // A big add gives the edit something to be interrupted during.
    let changeset = Changeset {
        adds: vec![add_bytes("big.bin", &vec![7u8; 512 * 1024])],
        ..Default::default()
    };
    let result = apply(&path, &changeset, &hooks);
    assert!(matches!(result, Err(MutationError::Cancelled)), "got {result:?}");

    // The original archive is byte-for-byte unchanged.
    assert_eq!(
        std::fs::read(&path).expect("re-read original"),
        original,
        "original untouched by a cancelled edit"
    );
    // No `.cmdr-tmp-` sibling lingers after a cancel.
    let leftovers: Vec<_> = std::fs::read_dir(tmp.path())
        .expect("read dir")
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains(".cmdr-tmp-"))
        .collect();
    assert!(leftovers.is_empty(), "cancel must remove the temp, found {leftovers:?}");
}

#[test]
fn a_leftover_temp_is_reaped_on_the_next_edit_and_the_original_survives() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let original = build_zip(&[stored("keep.txt", b"keep".to_vec())]);
    let path = write_zip(tmp.path(), "a", &original);

    // Simulate an abandoned build from a prior crash: a stale sibling temp.
    let stale = tmp.path().join("a.zip.cmdr-tmp-deadbeef");
    std::fs::write(&stale, b"garbage from a crashed edit").expect("write stale temp");

    // A successful edit reaps the stale temp as part of its run.
    apply(
        &path,
        &Changeset {
            adds: vec![add_bytes("added.txt", b"x")],
            ..Default::default()
        },
        &NoHooks,
    )
    .expect("apply over a leftover");

    assert!(!stale.exists(), "the stale temp is reaped on the next edit");
    let back = read_back(&path);
    assert!(back.contains_key("keep.txt") && back.contains_key("added.txt"));
}

// ---- Data-safety: merge invariant --------------------------------------------

#[test]
fn deleting_one_entry_keeps_the_others_byte_for_byte() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // Deflated content so a decompress/recompress would change the stored bytes;
    // raw_copy_file must keep them identical.
    let payload = b"the quick brown fox jumps over the lazy dog, repeated for entropy".repeat(20);
    let path = write_zip(
        tmp.path(),
        "a",
        &build_zip(&[
            deflated("survivor.dat", payload.clone()),
            stored("victim.txt", b"delete me".to_vec()),
        ]),
    );
    let raw_before = raw_entry_bytes(&path, "survivor.dat");

    apply(
        &path,
        &Changeset {
            deletes: vec!["victim.txt".to_string()],
            ..Default::default()
        },
        &NoHooks,
    )
    .expect("apply delete");

    let raw_after = raw_entry_bytes(&path, "survivor.dat");
    assert_eq!(
        raw_before, raw_after,
        "retained entry's raw compressed bytes are identical"
    );
    assert_eq!(
        read_back(&path).get("survivor.dat").map(|v| v.as_slice()),
        Some(payload.as_slice())
    );
}

/// Reads one entry's RAW (still-compressed) bytes, so a test can assert a
/// retained entry was copied verbatim (not decompressed and recompressed).
fn raw_entry_bytes(path: &Path, name: &str) -> Vec<u8> {
    let file = File::open(path).expect("open");
    let mut archive = ZipArchive::new(file).expect("parse");
    let idx = (0..archive.len())
        .find(|&i| archive.by_index_raw(i).expect("raw").name() == name)
        .expect("entry present");
    let mut raw = archive.by_index_raw(idx).expect("raw entry");
    let mut buf = Vec::new();
    raw.read_to_end(&mut buf).expect("read raw bytes");
    buf
}

// ---- Data-safety: metadata preservation --------------------------------------

#[test]
fn an_edit_preserves_the_archive_mode_mtime_and_xattrs() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().expect("tempdir");
    let path = write_zip(tmp.path(), "a", &build_zip(&[stored("keep.txt", b"keep".to_vec())]));

    // Stamp identity metadata a plain copy would keep: a non-default mode, an old
    // mtime, and (macOS) a real Finder-tag xattr.
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).expect("chmod");
    let old_mtime = filetime::FileTime::from_unix_time(1_600_000_000, 0);
    filetime::set_file_mtime(&path, old_mtime).expect("set mtime");
    #[cfg(target_os = "macos")]
    let tag_xattr = "com.apple.metadata:_kMDItemUserTags";
    #[cfg(not(target_os = "macos"))]
    let tag_xattr = "user.cmdr_test_tag";
    let tag_value = b"marker-value".to_vec();
    // Setting an arbitrary xattr can fail on exotic filesystems; only assert
    // preservation when the stamp itself took.
    let xattr_stamped = xattr::set(&path, tag_xattr, &tag_value).is_ok();

    apply(
        &path,
        &Changeset {
            adds: vec![add_bytes("added.txt", b"x")],
            ..Default::default()
        },
        &NoHooks,
    )
    .expect("apply edit");

    let meta = std::fs::metadata(&path).expect("stat result");
    assert_eq!(
        meta.permissions().mode() & 0o777,
        0o640,
        "mode preserved across the edit"
    );
    assert_eq!(
        filetime::FileTime::from_last_modification_time(&meta),
        old_mtime,
        "mtime preserved across the edit"
    );
    if xattr_stamped {
        assert_eq!(
            xattr::get(&path, tag_xattr).expect("read xattr").as_deref(),
            Some(tag_value.as_slice()),
            "extended attribute (Finder tag on macOS) preserved across the edit"
        );
    }
}

// ---- Data-safety: encrypted-entry handling -----------------------------------
//
// `zip`'s raw copy reconstructs an entry's options from `ZipFile::options()`,
// which does NOT carry the traditional-PKWARE encryption GP flag. So a retained
// encrypted entry would keep its ciphertext bytes but lose the "encrypted"
// header bit — semantically corrupt (a reader would hand back ciphertext as
// plaintext). The mutator refuses any edit that would RETAIN an encrypted entry,
// leaving the original untouched. Deleting an encrypted entry is fine (it isn't
// retained). This is a deviation from the plan's "raw_copy retains encrypted
// entries byte-for-byte" assumption, resolved in favor of data safety.

#[test]
fn an_edit_that_would_retain_an_encrypted_entry_is_refused_and_leaves_the_original_intact() {
    use super::super::test_fixtures::set_first_entry_encrypted;

    let tmp = tempfile::tempdir().expect("tempdir");
    let mut bytes = build_zip(&[
        stored("secret.bin", b"pretend-encrypted-payload".to_vec()),
        stored("plain.txt", b"plain".to_vec()),
    ]);
    set_first_entry_encrypted(&mut bytes);
    let path = write_zip(tmp.path(), "a", &bytes);
    let original_on_disk = std::fs::read(&path).expect("read original");

    // An unrelated add would retain the encrypted entry -> refused.
    let result = apply(
        &path,
        &Changeset {
            adds: vec![add_bytes("note.txt", b"note")],
            ..Default::default()
        },
        &NoHooks,
    );
    assert!(
        matches!(result, Err(MutationError::EncryptedEntryRetained { .. })),
        "got {result:?}"
    );
    // The original archive is byte-for-byte untouched, and no temp lingers.
    assert_eq!(
        std::fs::read(&path).expect("re-read"),
        original_on_disk,
        "original untouched by a refused edit"
    );
    let leftovers = std::fs::read_dir(tmp.path())
        .expect("read dir")
        .flatten()
        .any(|e| e.file_name().to_string_lossy().contains(".cmdr-tmp-"));
    assert!(!leftovers, "a refused edit creates no temp");
}

#[test]
fn deleting_an_encrypted_entry_is_allowed() {
    use super::super::test_fixtures::set_first_entry_encrypted;

    let tmp = tempfile::tempdir().expect("tempdir");
    let mut bytes = build_zip(&[
        stored("secret.bin", b"pretend-encrypted-payload".to_vec()),
        stored("plain.txt", b"plain".to_vec()),
    ]);
    set_first_entry_encrypted(&mut bytes);
    let path = write_zip(tmp.path(), "a", &bytes);

    // Deleting the encrypted entry doesn't retain it, so the edit proceeds.
    apply(
        &path,
        &Changeset {
            deletes: vec!["secret.bin".to_string()],
            ..Default::default()
        },
        &NoHooks,
    )
    .expect("delete of an encrypted entry is allowed");

    let back = read_back(&path);
    assert!(!back.contains_key("secret.bin"), "encrypted entry removed");
    assert_eq!(back.get("plain.txt").map(|v| v.as_slice()), Some(b"plain".as_slice()));
}

// ---- Data-safety: pause reaches mid-add --------------------------------------

/// Hooks that park in `wait_if_paused` until the test resumes them, recording
/// once they've parked so the test can observe the parked state.
struct ParkUntilResumed {
    resumed: std::sync::Mutex<bool>,
    cvar: std::sync::Condvar,
    parked: AtomicBool,
}

impl MutationHooks for ParkUntilResumed {
    fn wait_if_paused(&self) {
        let mut resumed = self.resumed.lock().expect("lock");
        if !*resumed {
            self.parked.store(true, Ordering::SeqCst);
            while !*resumed {
                resumed = self.cvar.wait(resumed).expect("condvar wait");
            }
        }
    }
}

#[test]
fn a_paused_add_parks_then_completes_on_resume() {
    use std::sync::Arc;

    let tmp = tempfile::tempdir().expect("tempdir");
    let original = build_zip(&[stored("keep.txt", b"keep".to_vec())]);
    let path = write_zip(tmp.path(), "a", &original);
    let path_for_thread = path.clone();

    let hooks = Arc::new(ParkUntilResumed {
        resumed: std::sync::Mutex::new(false),
        cvar: std::sync::Condvar::new(),
        parked: AtomicBool::new(false),
    });
    let hooks_for_thread = Arc::clone(&hooks);

    let handle = std::thread::spawn(move || {
        let changeset = Changeset {
            adds: vec![add_bytes("big.bin", &vec![9u8; 512 * 1024])],
            ..Default::default()
        };
        apply(&path_for_thread, &changeset, &*hooks_for_thread)
    });

    // Wait until the edit has parked (bounded, no arbitrary sleep budget).
    let mut waited = 0;
    while !hooks.parked.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(1));
        waited += 1;
        assert!(waited < 2000, "edit never parked");
    }
    // While parked, the original archive is untouched (nothing renamed yet).
    assert_eq!(
        std::fs::read(&path).expect("read while parked"),
        original,
        "original untouched while paused"
    );

    // Resume and let it finish.
    {
        let mut resumed = hooks.resumed.lock().expect("lock");
        *resumed = true;
        hooks.cvar.notify_all();
    }
    handle.join().expect("join").expect("edit completes after resume");
    assert!(
        read_back(&path).contains_key("big.bin"),
        "the paused add landed after resume"
    );
}

// ---- Progress accounting -----------------------------------------------------

/// Records every progress snapshot the mutator reports.
struct RecordingHooks {
    snapshots: std::sync::Mutex<Vec<MutationProgress>>,
}

impl RecordingHooks {
    fn new() -> Self {
        Self {
            snapshots: std::sync::Mutex::new(Vec::new()),
        }
    }
    fn snapshots(&self) -> Vec<MutationProgress> {
        self.snapshots.lock().expect("lock").clone()
    }
}

impl MutationHooks for RecordingHooks {
    fn on_progress(&self, progress: MutationProgress) {
        self.snapshots.lock().expect("lock").push(progress);
    }
}

#[test]
fn progress_reaches_the_totals_and_never_goes_backwards() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // One retained STORED entry (compressed size == 5 bytes of "keepy"), plus one
    // added file of exactly 100 bytes, so the totals are predictable.
    let path = write_zip(tmp.path(), "a", &build_zip(&[stored("keep.txt", b"keepy".to_vec())]));
    let added = vec![b'z'; 100];

    let hooks = RecordingHooks::new();
    apply(
        &path,
        &Changeset {
            adds: vec![add_bytes("added.bin", &added)],
            ..Default::default()
        },
        &hooks,
    )
    .expect("apply add");

    let snaps = hooks.snapshots();
    assert!(!snaps.is_empty(), "progress was reported");

    // Totals: 2 entries (1 retained + 1 add); bytes = retained compressed (5, stored)
    // + added (100) = 105.
    let last = *snaps.last().expect("a final snapshot");
    assert_eq!(last.entries_total, 2, "entries_total counts retained + added");
    assert_eq!(last.bytes_total, 105, "bytes_total = retained compressed + added");
    assert_eq!(last.entries_done, 2, "every entry is accounted for at the end");
    assert_eq!(last.bytes_done, 105, "every byte is accounted for at the end");

    // Monotonic non-decreasing on both axes (catches a `-=` / `*=` on the accumulators).
    for pair in snaps.windows(2) {
        assert!(
            pair[1].entries_done >= pair[0].entries_done,
            "entries_done never decreases: {:?} -> {:?}",
            pair[0],
            pair[1]
        );
        assert!(
            pair[1].bytes_done >= pair[0].bytes_done,
            "bytes_done never decreases: {:?} -> {:?}",
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn entries_changed_counts_added_deleted_and_renamed_not_retained() {
    // Three kept entries plus one to delete and one to rename; the changeset also
    // adds a file and a dir. `entries_changed` must be the affected count (4: one
    // delete, one rename, one add, one mkdir), NOT the written-entry total (6:
    // four retained + one add + one mkdir). This is the user-facing "files
    // processed" count that fixed the "Delete complete: 2 files" off-by count.
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = write_zip(
        tmp.path(),
        "a",
        &build_zip(&[
            stored("keep1.txt", b"1".to_vec()),
            stored("keep2.txt", b"2".to_vec()),
            stored("keep3.txt", b"3".to_vec()),
            stored("del.txt", b"d".to_vec()),
            stored("ren.txt", b"r".to_vec()),
        ]),
    );

    let hooks = RecordingHooks::new();
    apply(
        &path,
        &Changeset {
            adds: vec![add_bytes("new.txt", b"n")],
            mkdirs: vec!["newdir".to_string()],
            deletes: vec!["del.txt".to_string()],
            renames: vec![("ren.txt".to_string(), "ren2.txt".to_string())],
        },
        &hooks,
    )
    .expect("apply mixed changeset");

    let last = *hooks.snapshots().last().expect("a final snapshot");
    assert_eq!(
        last.entries_changed, 4,
        "entries_changed = 1 delete + 1 rename + 1 add + 1 mkdir"
    );
    assert_eq!(
        last.entries_total, 6,
        "entries_total (written) = 4 retained + 1 add + 1 mkdir — deliberately larger than entries_changed"
    );
}

#[test]
fn a_local_path_add_streams_its_bytes_and_counts_them() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = write_zip(tmp.path(), "a", &build_zip(&[stored("keep.txt", b"keep".to_vec())]));

    // A local source file larger than one add chunk, so the streaming loop runs
    // multiple iterations (exercises the read-until-zero loop, not a single read).
    let source_path = tmp.path().join("source.bin");
    let payload = vec![b'q'; 300 * 1024];
    std::fs::write(&source_path, &payload).expect("write source");

    let hooks = RecordingHooks::new();
    apply(
        &path,
        &Changeset {
            adds: vec![AddEntry {
                inner_path: "into/added.bin".to_string(),
                source: AddSource::LocalPath(source_path),
            }],
            ..Default::default()
        },
        &hooks,
    )
    .expect("apply local-path add");

    // The full payload landed (a broken read-loop would truncate or empty it).
    assert_eq!(
        read_back(&path).get("into/added.bin").map(|v| v.len()),
        Some(payload.len()),
        "the streamed file's full length landed"
    );
    // The streamed bytes were counted into progress.
    let last = *hooks.snapshots().last().expect("a final snapshot");
    assert_eq!(last.bytes_done, last.bytes_total, "streamed bytes fully accounted for");
    assert!(
        last.bytes_done >= payload.len() as u64,
        "at least the added bytes counted"
    );
}

#[test]
fn a_local_path_add_carries_the_source_files_mtime() {
    use crate::file_system::volume::backends::archive::{ArchiveIndex, LocalFileSource};

    let tmp = tempfile::tempdir().expect("tempdir");
    let path = write_zip(tmp.path(), "a", &build_zip(&[stored("keep.txt", b"keep".to_vec())]));

    // A source file stamped with a known UTC mtime. MS-DOS time has 2-second
    // granularity; this value is already an even second, so the round-trip is exact.
    let src = tmp.path().join("src.txt");
    std::fs::write(&src, b"payload").expect("write source");
    let mtime_secs: i64 = 1_600_000_000; // 2020-09-13T12:26:40Z
    filetime::set_file_mtime(&src, filetime::FileTime::from_unix_time(mtime_secs, 0)).expect("set mtime");

    apply(
        &path,
        &Changeset {
            adds: vec![AddEntry {
                inner_path: "added.txt".to_string(),
                source: AddSource::LocalPath(src),
            }],
            ..Default::default()
        },
        &NoHooks,
    )
    .expect("apply local-path add");

    // Re-parse via the app's index: the added entry reports the SOURCE mtime
    // (within DOS granularity), not the archive's write time.
    let source = LocalFileSource::open(&path).expect("open archive");
    let index = ArchiveIndex::parse(&source).expect("parse index");
    let reported = index
        .get("added.txt")
        .and_then(|n| n.modified)
        .expect("added entry has a modification time");
    assert!(
        (reported - mtime_secs).abs() <= 2,
        "the added entry's mtime {reported} must match the source's {mtime_secs} within DOS-time granularity"
    );
}
