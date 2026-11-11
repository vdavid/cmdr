//! An edit keeps what the archive is beyond its visible entries: the archive
//! comment, the bytes before the zip data (a self-extractor stub), and a
//! symlink the archive was opened through. One it can't keep, the earlier of
//! two entries sharing a name, makes it refuse the edit untouched (#542).

use std::path::{Path, PathBuf};

use super::*;
use crate::test_fixtures::{build_zip, build_zipcrypto_zip, cloak_first_central_name, plain_entry, stored};

fn write_zip(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("write the fixture zip");
    path
}

fn delete(path: &Path, inner: &str) -> Result<(), MutationError> {
    let changeset = Changeset {
        deletes: vec![inner.to_string()],
        ..Default::default()
    };
    apply(path, &changeset, &NoHooks)
}

fn entry_names(path: &Path) -> Vec<String> {
    let archive = ZipArchive::new(File::open(path).expect("open the zip")).expect("the zip parses");
    let mut names: Vec<String> = archive
        .file_names()
        .map(|name| name.expect("a name").into_owned())
        .collect();
    names.sort();
    names
}

fn leftover_temps(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .expect("list the dir")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".cmdr-tmp-"))
        .collect()
}

#[test]
fn an_edit_keeps_the_archive_comment() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut writer = ZipWriter::new(io::Cursor::new(Vec::new()));
    for name in ["keep.txt", "drop.txt"] {
        writer
            .start_file(name, SimpleFileOptions::default())
            .expect("start an entry");
        writer.write_all(b"bytes").expect("write an entry");
    }
    writer
        .set_raw_comment(Box::from(&b"Release 1.2, built 2026-10-20"[..]))
        .expect("set the comment");
    let path = write_zip(tmp.path(), "a.zip", &writer.finish().expect("finish").into_inner());

    delete(&path, "drop.txt").expect("the edit applies");

    let archive = ZipArchive::new(File::open(&path).expect("open the zip")).expect("the zip parses");
    assert_eq!(archive.comment(), b"Release 1.2, built 2026-10-20");
    assert_eq!(entry_names(&path), vec!["keep.txt"]);
}

/// The stub before the zip data is what makes a self-extractor run; the edit
/// writes it back first and the zip after it, with offsets that count it.
#[test]
fn an_edit_keeps_the_bytes_before_the_zip_data() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let stub = b"#!/bin/sh\necho 'a self-extractor stub'\nexit 0\n".repeat(3);
    let mut bytes = stub.clone();
    bytes.extend(build_zip(&[
        stored("keep.txt", "keep me"),
        stored("drop.txt", "drop me"),
    ]));
    let path = write_zip(tmp.path(), "sfx.zip", &bytes);

    delete(&path, "drop.txt").expect("the edit applies");

    let after = std::fs::read(&path).expect("read the result");
    assert!(after.starts_with(&stub), "the stub is still at the start");
    assert_eq!(entry_names(&path), vec!["keep.txt"]);
    let source = std::sync::Arc::new(crate::BytesSource::new(after));
    let index = crate::ArchiveIndex::parse(source, crate::ArchiveFormat::Zip, None).expect("the reader parses it");
    assert!(index.get("keep.txt").is_some(), "the reader still finds the kept entry");
    let unzip_ok = std::process::Command::new("unzip")
        .arg("-t")
        .arg(&path)
        .output()
        .map_or(true, |output| output.status.success());
    assert!(unzip_ok, "unzip -t accepts the result");
}

/// The `zip` crate folds a duplicate name into one entry, so a rewrite through
/// it can't keep the earlier copy. Refused before any temp exists.
#[test]
fn an_archive_with_a_duplicate_name_is_refused_untouched() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let bytes = build_zipcrypto_zip(
        &[
            plain_entry("twin.txt", "earlier twin"),
            plain_entry("other.txt", "other"),
            plain_entry("twin.txt", "later twin"),
        ],
        "unused",
    );
    let path = write_zip(tmp.path(), "twins.zip", &bytes);

    let result = delete(&path, "other.txt");

    assert!(
        matches!(&result, Err(MutationError::DuplicateEntryName { name: Some(name) }) if name == "twin.txt"),
        "got {result:?}"
    );
    assert_eq!(
        std::fs::read(&path).expect("read the original"),
        bytes,
        "the original is untouched"
    );
    assert!(leftover_temps(tmp.path()).is_empty(), "no temp is left behind");
}

/// The refusal stands on the `zip` crate's own count of central-directory
/// records, so it holds when the reader can't parse the archive and the planner
/// has no names of its own (it gets an empty list then). It can't name the twin.
#[test]
fn a_duplicate_name_is_refused_even_without_the_readers_names() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let bytes = build_zipcrypto_zip(
        &[plain_entry("twin.txt", "earlier"), plain_entry("twin.txt", "later")],
        "unused",
    );
    let path = write_zip(tmp.path(), "twins.zip", &bytes);
    let src = ZipArchive::new(File::open(&path).expect("open the zip")).expect("the zip parses");

    let result = crate::mutation::plan::refuse_a_folded_duplicate(&src, &[]);

    assert!(
        matches!(&result, Err(MutationError::DuplicateEntryName { name: None })),
        "got {result:?}"
    );
}

/// The rename replaces the file the link points at, so the edit lands where the
/// user's archive lives and the link stays a link.
#[cfg(unix)]
#[test]
fn an_edit_through_a_symlink_changes_the_target_and_keeps_the_link() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let real_dir = tmp.path().join("real");
    std::fs::create_dir(&real_dir).expect("make the target dir");
    let target = write_zip(
        &real_dir,
        "report.zip",
        &build_zip(&[stored("keep.txt", "keep me"), stored("drop.txt", "drop me")]),
    );
    let link = tmp.path().join("report.zip");
    cmdr_fs::testing::symlink(&target, &link).expect("make the link");

    delete(&link, "drop.txt").expect("the edit applies");

    let link_meta = std::fs::symlink_metadata(&link).expect("stat the link");
    assert!(link_meta.file_type().is_symlink(), "the link is still a link");
    assert_eq!(std::fs::read_link(&link).expect("read the link"), target);
    assert_eq!(entry_names(&target), vec!["keep.txt"], "the target holds the edit");
    assert!(leftover_temps(tmp.path()).is_empty() && leftover_temps(&real_dir).is_empty());
}

/// `a.txt`, `b.txt`, and `c.txt`, with `b.txt`'s central record pointed at
/// `a.txt`'s local header, as a malformed or hostile zip can: two names, one
/// offset. Both crates list all three.
fn shared_header_zip(dir: &Path) -> PathBuf {
    let mut bytes = build_zip(&[
        stored("a.txt", "alpha"),
        stored("b.txt", "bravo"),
        stored("c.txt", "charlie"),
    ]);
    let record = (0..bytes.len() - 51)
        .find(|&i| bytes[i..].starts_with(b"PK\x01\x02") && &bytes[i + 46..i + 51] == b"b.txt")
        .expect("b.txt's central record");
    bytes[record + 42..record + 46].copy_from_slice(&0u32.to_le_bytes());
    write_zip(dir, "shared.zip", &bytes)
}

/// Pairing the reader's names with the writer's entries by header offset alone
/// gave both entries the later name, so deleting one took its twin too, and
/// deleting the other took nothing.
#[test]
fn entries_sharing_a_local_header_each_delete_by_their_own_name() {
    for spare_hidden in [false, true] {
        for (target, left) in [("a.txt", ["b.txt", "c.txt"]), ("b.txt", ["a.txt", "c.txt"])] {
            let tmp = tempfile::tempdir().expect("tempdir");
            let path = shared_header_zip(tmp.path());
            let changeset = Changeset {
                deletes: vec![target.to_string()],
                deletes_spare_hidden: spare_hidden,
                ..Default::default()
            };

            apply(&path, &changeset, &NoHooks).expect("the edit applies");

            assert_eq!(
                entry_names(&path),
                left,
                "deleting {target} (spare hidden: {spare_hidden})"
            );
        }
    }
}

// ---- An entry under a file entry ----------------------------------------------

fn add(inner: &str) -> AddEntry {
    AddEntry {
        inner_path: inner.to_string(),
        source: AddSource::Bytes(b"new".to_vec()),
    }
}

/// Every reader drops an entry whose parent is a file, so an edit that would
/// write one is refused before any temp exists, whichever side it writes.
#[test]
fn an_edit_that_would_put_an_entry_under_a_file_is_refused_untouched() {
    let cases: [(&[&str], Changeset, &str, &str); 3] = [
        (
            &["notes", "keep.txt"],
            Changeset {
                adds: vec![add("notes/a.txt")],
                ..Default::default()
            },
            "notes/a.txt",
            "notes",
        ),
        (
            &["notes"],
            Changeset {
                mkdirs: vec!["notes/sub".to_string()],
                ..Default::default()
            },
            "notes/sub",
            "notes",
        ),
        (
            &["d/x.txt"],
            Changeset {
                adds: vec![add("d")],
                ..Default::default()
            },
            "d/x.txt",
            "d",
        ),
    ];
    for (names, changeset, want_entry, want_file) in cases {
        let tmp = tempfile::tempdir().expect("tempdir");
        let entries: Vec<_> = names.iter().map(|name| stored(*name, "bytes")).collect();
        let bytes = build_zip(&entries);
        let path = write_zip(tmp.path(), "a.zip", &bytes);

        let result = apply(&path, &changeset, &NoHooks);

        assert!(
            matches!(&result, Err(MutationError::EntryUnderFile { entry, file }) if entry == want_entry && file == want_file),
            "{names:?}: got {result:?}"
        );
        assert_eq!(std::fs::read(&path).expect("read the original"), bytes, "{names:?}");
        assert!(
            leftover_temps(tmp.path()).is_empty(),
            "{names:?}: no temp is left behind"
        );
    }
}

/// The refusal is about what the edit WRITES: a zip that already holds an entry
/// under a file stays editable, and replacing a file with a folder of its name
/// (the file deleted in the same edit) is fine.
#[test]
fn an_edit_that_writes_no_entry_under_a_file_applies() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = write_zip(
        tmp.path(),
        "malformed.zip",
        &build_zip(&[stored("notes", "file"), stored("notes/hidden.txt", "hidden")]),
    );
    let changeset = Changeset {
        adds: vec![add("other.txt")],
        ..Default::default()
    };
    apply(&path, &changeset, &NoHooks).expect("an unrelated add applies");

    let path = write_zip(tmp.path(), "replace.zip", &build_zip(&[stored("notes", "file")]));
    let changeset = Changeset {
        deletes: vec!["notes".to_string()],
        mkdirs: vec!["notes".to_string()],
        adds: vec![add("notes/a.txt")],
        ..Default::default()
    };
    apply(&path, &changeset, &NoHooks).expect("a file replaced by a folder applies");
    assert_eq!(entry_names(&path), ["notes/", "notes/a.txt"]);
}

/// Zip cloaking (GHSA-m665-v7pm-7grj): a kept entry's local header names
/// something other than its central-directory record. The rewrite writes a fresh
/// local header from the central name, so the edited zip no longer carries the
/// disagreement for a local-header reader (`zip`'s streaming extractor) to act on.
#[test]
fn an_edit_writes_a_cloaked_entrys_local_header_under_its_listed_name() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut bytes = build_zip(&[stored("cron_worker.sh", "payload"), stored("other.txt", "o")]);
    cloak_first_central_name(&mut bytes, b"doc_report.pdf");
    let path = write_zip(tmp.path(), "cloaked.zip", &bytes);
    let mut before = ZipArchive::new(File::open(&path).expect("open the zip")).expect("the zip parses");
    assert!(before.validate_header_consistency().is_err(), "the fixture is cloaked");

    delete(&path, "other.txt").expect("the delete applies");

    assert_eq!(entry_names(&path), ["doc_report.pdf"]);
    let mut after = ZipArchive::new(File::open(&path).expect("open the zip")).expect("the zip parses");
    after
        .validate_header_consistency()
        .expect("local and central headers agree after the edit");
    let written = std::fs::read(&path).expect("read the result");
    assert!(!written.windows(14).any(|w| w == b"cron_worker.sh"));
}
