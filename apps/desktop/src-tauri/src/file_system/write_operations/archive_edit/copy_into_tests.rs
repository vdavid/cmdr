//! Non-interactive copy/move INTO a zip: the basic directory-tree copy, the
//! pre-resolved conflict policies (Rename, conditional overwrite, move-delete
//! gating), the data-safety handling of unrepresentable source entries
//! (symlinks / special files), and the remote-parent copy-into path.

use super::test_support::*;

#[tokio::test]
async fn copy_into_adds_a_local_directory_tree_and_skips_conflicts() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::route_archive_copy_into;

    let tmp = tempfile::tempdir().expect("tempdir");
    // The archive already holds `payload/existing.txt`, so a Skip-policy copy of a
    // colliding file leaves it untouched while adding the new ones.
    let archive = tmp.path().join("a.zip");
    {
        let file = std::fs::File::create(&archive).expect("create zip");
        let mut writer = ZipWriter::new(file);
        writer
            .start_file("payload/existing.txt", SimpleFileOptions::default())
            .expect("start");
        writer.write_all(b"OLD").expect("write");
        writer.finish().expect("finish");
    }

    // A local source tree: payload/{existing.txt, fresh.txt, sub/deep.txt}.
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("payload/sub")).expect("mkdir src");
    std::fs::write(src_root.join("payload/existing.txt"), b"NEW").expect("w1");
    std::fs::write(src_root.join("payload/fresh.txt"), b"fresh").expect("w2");
    std::fs::write(src_root.join("payload/sub/deep.txt"), b"deep").expect("w3");

    // A local-FS source volume rooted at src_root (drives `local_path()`).
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.clone()));

    let events = Arc::new(CollectorEventSink::new());
    // Destination is the archive ROOT, so the source dir `payload` lands as `payload/`.
    let dest = archive.clone();
    route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("payload")],
        dest,
        "root".to_string(),
        ConflictResolution::Skip,
        0,
        false,
    )
    .await
    .expect("start copy-into");

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "copy-into should complete"
    );
    // The colliding file kept its OLD bytes (Skip), the new files were added.
    assert_eq!(
        read_entry(&archive, "payload/existing.txt").as_deref(),
        Some(b"OLD".as_slice())
    );
    assert_eq!(
        read_entry(&archive, "payload/fresh.txt").as_deref(),
        Some(b"fresh".as_slice())
    );
    assert_eq!(
        read_entry(&archive, "payload/sub/deep.txt").as_deref(),
        Some(b"deep".as_slice())
    );
}

// ---- Non-interactive policy coverage (Rename / conditional / move gating) --

/// Runs a non-interactive copy/move INTO `archive` of local dir `src_rel` under a
/// pre-resolved `policy`, landing at the archive root. Returns the collector.
async fn run_policy_copy_into(
    archive: &Path,
    src_root: &Path,
    src_rel: &str,
    policy: ConflictResolution,
    is_move: bool,
) -> Arc<CollectorEventSink> {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::route_archive_copy_into;

    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.to_path_buf()));
    let events = Arc::new(CollectorEventSink::new());
    route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from(src_rel)],
        archive.to_path_buf(),
        "root".to_string(),
        policy,
        0,
        is_move,
    )
    .await
    .expect("start policy copy-into");
    events
}

#[tokio::test]
async fn rename_policy_picks_the_next_free_numbered_name() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    // Both `existing.txt` AND `existing (1).txt` are taken, so Rename must land on
    // `existing (2).txt` — pinning the extension placement AND the skip-taken loop.
    write_multi_zip(
        &archive,
        &[
            ("d/existing.txt", b"OLD"),
            ("d/existing (1).txt", b"OLD1"),
            ("d/.env", b"OLD_ENV"),
        ],
    );

    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir src");
    std::fs::write(src_root.join("d/existing.txt"), b"NEW").expect("w1");
    // A DOTFILE has no stem before its dot, so the whole name (incl. the leading
    // dot) is the stem and the ` (n)` suffix goes at the END: `.env (1)`, not
    // ` (1).env`. Pins the extension-placement guard.
    std::fs::write(src_root.join("d/.env"), b"ENV").expect("w2");

    let events = run_policy_copy_into(&archive, &src_root, "d", ConflictResolution::Rename, false).await;

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "rename copy-into should complete"
    );
    // Originals untouched; the incoming file lands under the next free name with
    // the extension kept BEFORE the ` (n)` suffix.
    assert_eq!(
        read_entry(&archive, "d/existing.txt").as_deref(),
        Some(b"OLD".as_slice())
    );
    assert_eq!(
        read_entry(&archive, "d/existing (1).txt").as_deref(),
        Some(b"OLD1".as_slice())
    );
    assert_eq!(
        read_entry(&archive, "d/existing (2).txt").as_deref(),
        Some(b"NEW".as_slice()),
        "Rename must pick the next free ` (n)` name and keep the extension in place"
    );
    assert_eq!(
        read_entry(&archive, "d/.env").as_deref(),
        Some(b"OLD_ENV".as_slice()),
        "the original dotfile is kept"
    );
    assert_eq!(
        read_entry(&archive, "d/.env (1)").as_deref(),
        Some(b"ENV".as_slice()),
        "a renamed dotfile keeps its whole name as the stem: `.env (1)`, not ` (1).env`"
    );
}

#[tokio::test]
async fn overwrite_smaller_overwrites_only_a_strictly_smaller_entry() {
    // Case A: the archive entry is LARGER than the source → skip (kept).
    let tmp = tempfile::tempdir().expect("tempdir");
    let archive_a = tmp.path().join("a.zip");
    write_multi_zip(&archive_a, &[("d/f.txt", b"BIGDATA")]); // 7 bytes
    let src_a = tmp.path().join("srca");
    std::fs::create_dir_all(src_a.join("d")).expect("mkdir");
    std::fs::write(src_a.join("d/f.txt"), b"hi").expect("w"); // 2 bytes
    let events_a = run_policy_copy_into(&archive_a, &src_a, "d", ConflictResolution::OverwriteSmaller, false).await;
    assert!(wait_until(|| !events_a.complete.lock_ignore_poison().is_empty()).await);
    assert_eq!(
        read_entry(&archive_a, "d/f.txt").as_deref(),
        Some(b"BIGDATA".as_slice()),
        "a larger destination must NOT be overwritten under OverwriteSmaller"
    );

    // Case B: the archive entry is SMALLER than the source → overwrite.
    let archive_b = tmp.path().join("b.zip");
    write_multi_zip(&archive_b, &[("d/f.txt", b"hi")]); // 2 bytes
    let src_b = tmp.path().join("srcb");
    std::fs::create_dir_all(src_b.join("d")).expect("mkdir");
    std::fs::write(src_b.join("d/f.txt"), b"BIGDATA").expect("w"); // 7 bytes
    let events_b = run_policy_copy_into(&archive_b, &src_b, "d", ConflictResolution::OverwriteSmaller, false).await;
    assert!(wait_until(|| !events_b.complete.lock_ignore_poison().is_empty()).await);
    assert_eq!(
        read_entry(&archive_b, "d/f.txt").as_deref(),
        Some(b"BIGDATA".as_slice()),
        "a strictly smaller destination must be overwritten under OverwriteSmaller"
    );

    // Case C: EQUAL sizes → skip (the comparison is strict `<`, not `<=`).
    let archive_c = tmp.path().join("c.zip");
    write_multi_zip(&archive_c, &[("d/f.txt", b"AB")]); // 2 bytes
    let src_c = tmp.path().join("srcc");
    std::fs::create_dir_all(src_c.join("d")).expect("mkdir");
    std::fs::write(src_c.join("d/f.txt"), b"XY").expect("w"); // 2 bytes, equal
    let events_c = run_policy_copy_into(&archive_c, &src_c, "d", ConflictResolution::OverwriteSmaller, false).await;
    assert!(wait_until(|| !events_c.complete.lock_ignore_poison().is_empty()).await);
    assert_eq!(
        read_entry(&archive_c, "d/f.txt").as_deref(),
        Some(b"AB".as_slice()),
        "an equal-size destination must NOT be overwritten (strict `<`, never `<=`)"
    );
}

#[tokio::test]
async fn overwrite_older_overwrites_only_a_strictly_older_entry() {
    use zip::DateTime;
    use zip::write::SimpleFileOptions;

    // Build a zip whose single entry carries a controlled modification date.
    fn write_zip_with_mtime(path: &Path, name: &str, content: &[u8], dt: DateTime) {
        let file = std::fs::File::create(path).expect("create zip");
        let mut writer = ZipWriter::new(file);
        writer
            .start_file(name, SimpleFileOptions::default().last_modified_time(dt))
            .expect("start");
        writer.write_all(content).expect("write");
        writer.finish().expect("finish");
    }
    let old = DateTime::from_date_and_time(2020, 1, 1, 0, 0, 0).expect("2020 date");
    let new = DateTime::from_date_and_time(2024, 1, 1, 0, 0, 0).expect("2024 date");
    let src_2020 = filetime::FileTime::from_unix_time(1_577_836_800, 0); // 2020-01-01Z
    let src_2024 = filetime::FileTime::from_unix_time(1_704_067_200, 0); // 2024-01-01Z

    let tmp = tempfile::tempdir().expect("tempdir");

    // Case A: archive entry is NEWER (2024) than the source (2020) → skip.
    let archive_a = tmp.path().join("a.zip");
    write_zip_with_mtime(&archive_a, "d/f.txt", b"KEEP", new);
    let src_a = tmp.path().join("srca");
    std::fs::create_dir_all(src_a.join("d")).expect("mkdir");
    std::fs::write(src_a.join("d/f.txt"), b"INCOMING").expect("w");
    filetime::set_file_mtime(src_a.join("d/f.txt"), src_2020).expect("mtime");
    let events_a = run_policy_copy_into(&archive_a, &src_a, "d", ConflictResolution::OverwriteOlder, false).await;
    assert!(wait_until(|| !events_a.complete.lock_ignore_poison().is_empty()).await);
    assert_eq!(
        read_entry(&archive_a, "d/f.txt").as_deref(),
        Some(b"KEEP".as_slice()),
        "a newer destination must NOT be overwritten under OverwriteOlder"
    );

    // Case B: archive entry is OLDER (2020) than the source (2024) → overwrite.
    let archive_b = tmp.path().join("b.zip");
    write_zip_with_mtime(&archive_b, "d/f.txt", b"KEEP", old);
    let src_b = tmp.path().join("srcb");
    std::fs::create_dir_all(src_b.join("d")).expect("mkdir");
    std::fs::write(src_b.join("d/f.txt"), b"INCOMING").expect("w");
    filetime::set_file_mtime(src_b.join("d/f.txt"), src_2024).expect("mtime");
    let events_b = run_policy_copy_into(&archive_b, &src_b, "d", ConflictResolution::OverwriteOlder, false).await;
    assert!(wait_until(|| !events_b.complete.lock_ignore_poison().is_empty()).await);
    assert_eq!(
        read_entry(&archive_b, "d/f.txt").as_deref(),
        Some(b"INCOMING".as_slice()),
        "a strictly older destination must be overwritten under OverwriteOlder"
    );

    // Case C: EQUAL mtimes → skip (the comparison is strict `<`, not `<=`). Derive
    // the source mtime from the archive entry's ACTUAL parsed value so the two are
    // bit-for-bit equal regardless of DOS-datetime timezone conversion.
    use crate::file_system::volume::backends::archive::{ArchiveFormat, ArchiveIndex, LocalFileSource};
    let archive_c = tmp.path().join("c.zip");
    write_zip_with_mtime(&archive_c, "d/f.txt", b"KEEP", old);
    let parsed_mtime = {
        let src = LocalFileSource::open(&archive_c).expect("open archive");
        let index = ArchiveIndex::parse(Arc::new(src), ArchiveFormat::Zip).expect("parse index");
        index.get("d/f.txt").and_then(|n| n.modified).expect("entry mtime")
    };
    let src_c = tmp.path().join("srcc");
    std::fs::create_dir_all(src_c.join("d")).expect("mkdir");
    std::fs::write(src_c.join("d/f.txt"), b"INCOMING").expect("w");
    filetime::set_file_mtime(
        src_c.join("d/f.txt"),
        filetime::FileTime::from_unix_time(parsed_mtime, 0),
    )
    .expect("mtime");
    let events_c = run_policy_copy_into(&archive_c, &src_c, "d", ConflictResolution::OverwriteOlder, false).await;
    assert!(wait_until(|| !events_c.complete.lock_ignore_poison().is_empty()).await);
    assert_eq!(
        read_entry(&archive_c, "d/f.txt").as_deref(),
        Some(b"KEEP".as_slice()),
        "an equal-mtime destination must NOT be overwritten (strict `<`, never `<=`)"
    );
}

#[tokio::test]
async fn move_into_deletes_the_source_only_on_a_clean_transfer() {
    // Clean move (no collision) → the top-level source is deleted after commit.
    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("placeholder.txt", b"x")]);
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir");
    std::fs::write(src_root.join("d/a.txt"), b"aaa").expect("w");

    let events = run_policy_copy_into(&archive, &src_root, "d", ConflictResolution::Overwrite, true).await;
    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
    assert_eq!(read_entry(&archive, "d/a.txt").as_deref(), Some(b"aaa".as_slice()));
    assert!(
        wait_until(|| !src_root.join("d").exists()).await,
        "a clean move INTO the archive must delete the source after commit"
    );
}

#[tokio::test]
async fn move_into_with_a_skipped_collision_keeps_the_source() {
    // A move where a collision is Skipped must NOT delete the source (its bytes
    // didn't fully land) — the move invariant. Pins `is_move && !any_skipped`.
    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("d/a.txt", b"OLD")]);
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir");
    std::fs::write(src_root.join("d/a.txt"), b"NEW").expect("w1"); // collides → Skip
    std::fs::write(src_root.join("d/b.txt"), b"bbb").expect("w2"); // lands

    let events = run_policy_copy_into(&archive, &src_root, "d", ConflictResolution::Skip, true).await;
    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
    // The non-colliding file landed, the colliding one kept its OLD bytes, and the
    // source survives because something was skipped.
    assert_eq!(read_entry(&archive, "d/b.txt").as_deref(), Some(b"bbb".as_slice()));
    assert_eq!(read_entry(&archive, "d/a.txt").as_deref(), Some(b"OLD".as_slice()));
    assert!(
        src_root.join("d/a.txt").exists() && src_root.join("d/b.txt").exists(),
        "a partial (skipped) move must NOT delete the source"
    );
}

#[tokio::test]
async fn copy_into_preserves_an_empty_source_directory() {
    // An empty source subdir must materialize as an explicit archive dir entry —
    // pins the `!index.exists(&inner) && planned.insert(...)` mkdir guard.
    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("placeholder.txt", b"x")]);
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d/empty_sub")).expect("mkdir empty subdir");
    std::fs::write(src_root.join("d/f.txt"), b"f").expect("w");

    let events = run_policy_copy_into(&archive, &src_root, "d", ConflictResolution::Skip, false).await;
    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
    // The empty directory survives as an explicit `d/empty_sub/` entry.
    let file = std::fs::File::open(&archive).expect("open");
    let mut zip = ZipArchive::new(file).expect("zip");
    assert!(
        zip.by_name("d/empty_sub/").is_ok(),
        "an empty source directory must be preserved as an explicit archive entry"
    );
}

// ---- Data safety: unrepresentable source entries (symlinks, special files) --
//
// A zip changeset can only carry real files and directories. A symlink or a
// special file (fifo/socket/device) the builder can't represent must NOT be
// silently dropped on a MOVE: dropping it while still deleting the source loses
// the user's data. The all-or-nothing move policy applies — any unrepresentable
// entry marks the batch skipped, so the source is preserved (the move degrades
// to a copy) and the skip is surfaced on the terminal event.

#[cfg(unix)]
#[tokio::test]
async fn move_into_a_top_level_symlink_preserves_the_source_and_surfaces_the_skip() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::route_archive_copy_into;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("placeholder.txt", b"x")]);

    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(&src_root).expect("mkdir src");
    std::fs::write(src_root.join("target.txt"), b"real").expect("target");
    std::os::unix::fs::symlink("target.txt", src_root.join("link")).expect("symlink");

    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.clone()));
    let events = Arc::new(CollectorEventSink::new());
    route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("link")],
        archive.clone(),
        "root".to_string(),
        ConflictResolution::Overwrite,
        0,
        true, // is_move
    )
    .await
    .expect("start move-into");

    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
    // DATA SAFETY: the source symlink survives (it can't be archived, so the move
    // deletes nothing), and it never entered the archive.
    assert!(
        std::fs::symlink_metadata(src_root.join("link")).is_ok(),
        "a moved symlink the archive can't represent must be preserved at the source"
    );
    assert!(
        read_entry(&archive, "link").is_none(),
        "the symlink must not enter the archive"
    );
    let complete = events.complete.lock_ignore_poison();
    assert!(
        complete[0].files_skipped >= 1,
        "the skipped symlink must be surfaced on the terminal event, got {}",
        complete[0].files_skipped
    );
}

#[cfg(unix)]
#[tokio::test]
async fn move_into_a_dir_containing_a_symlink_preserves_the_whole_source_tree() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::route_archive_copy_into;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("placeholder.txt", b"x")]);

    // A dir with a real file AND a symlink inside. WalkDir yields the symlink but
    // the archive can't represent it — the whole move must degrade to a copy.
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("d")).expect("mkdir src");
    std::fs::write(src_root.join("d/real.txt"), b"real").expect("real");
    std::os::unix::fs::symlink("real.txt", src_root.join("d/link")).expect("symlink");

    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.clone()));
    let events = Arc::new(CollectorEventSink::new());
    route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("d")],
        archive.clone(),
        "root".to_string(),
        ConflictResolution::Overwrite,
        0,
        true, // is_move
    )
    .await
    .expect("start move-into");

    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
    // DATA SAFETY: the source tree survives (a contained symlink can't be archived,
    // so the batch is all-or-nothing skipped and the source dir is NOT removed).
    assert!(
        std::fs::symlink_metadata(src_root.join("d/link")).is_ok(),
        "a symlink inside a moved dir must be preserved (the source dir is not deleted)"
    );
    assert!(
        src_root.join("d/real.txt").exists(),
        "the source dir survives intact when the move degrades to a copy"
    );
    // The real file still landed in the archive (copy semantics for what CAN be
    // represented); only the source deletion is suppressed.
    assert_eq!(
        read_entry(&archive, "d/real.txt").as_deref(),
        Some(b"real".as_slice()),
        "the representable file is still copied into the archive"
    );
    let complete = events.complete.lock_ignore_poison();
    assert!(
        complete[0].files_skipped >= 1,
        "the skipped symlink must be surfaced, got {}",
        complete[0].files_skipped
    );
}

#[cfg(unix)]
#[tokio::test]
async fn move_into_a_broken_symlink_preserves_the_source() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::route_archive_copy_into;

    let tmp = tempfile::tempdir().expect("tempdir");
    let archive = tmp.path().join("a.zip");
    write_multi_zip(&archive, &[("placeholder.txt", b"x")]);

    // A dangling symlink (its target doesn't exist): `symlink_metadata` still
    // succeeds (it's an lstat), so it classifies as neither file nor dir.
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(&src_root).expect("mkdir src");
    std::os::unix::fs::symlink("/nonexistent/target-xyz", src_root.join("broken")).expect("symlink");

    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.clone()));
    let events = Arc::new(CollectorEventSink::new());
    route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("broken")],
        archive.clone(),
        "root".to_string(),
        ConflictResolution::Overwrite,
        0,
        true, // is_move
    )
    .await
    .expect("start move-into");

    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
    assert!(
        std::fs::symlink_metadata(src_root.join("broken")).is_ok(),
        "a broken symlink must be preserved at the source, never silently deleted"
    );
    let complete = events.complete.lock_ignore_poison();
    assert!(
        complete[0].files_skipped >= 1,
        "the skipped broken symlink must be surfaced, got {}",
        complete[0].files_skipped
    );
}

// ---- Remote-parent copy-into (plan against the pulled-local copy, not a local open) --
//
// A REMOTE archive (direct SMB / MTP parent) has NO real local path, so a route
// that opens the `.zip` with `LocalFileSource::open(archive_path)` fails (MTP) or
// hits the OS mount the design routes around (direct SMB). This pins that the
// non-interactive copy-into plans against the pulled-local working copy.

#[tokio::test]
async fn copy_into_a_remote_archive_lands_the_file_via_the_pulled_local_copy() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::route_archive_copy_into;

    let tmp = tempfile::tempdir().expect("tempdir");
    // A local source file to copy INTO the remote archive.
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(&src_root).expect("mkdir src");
    std::fs::write(src_root.join("new.txt"), b"fresh").expect("w");
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.clone()));

    // The archive lives on a NON-local parent — `/device/bundle.zip` is not a real
    // local file, so planning MUST run against the pulled-local working copy.
    let archive_path = PathBuf::from("/device/bundle.zip");
    let (parent_id, parent) = register_remote_zip(&archive_path, &[("keep.txt", b"keep")]).await;

    let events = Arc::new(CollectorEventSink::new());
    let result = route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("new.txt")],
        archive_path.clone(), // dest is the archive root
        parent_id.clone(),
        ConflictResolution::Overwrite,
        0,
        false,
    )
    .await;
    assert!(
        result.is_ok(),
        "a non-interactive copy INTO a remote archive must start (plan against the pulled copy), got {result:?}"
    );

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "the remote copy-into should complete"
    );
    // The copied file landed in the re-read remote archive, next to the original.
    assert_eq!(
        read_remote_entry(parent.as_ref(), &archive_path, "keep.txt")
            .await
            .as_deref(),
        Some(b"keep".as_slice()),
        "the original entry survives the remote edit"
    );
    assert_eq!(
        read_remote_entry(parent.as_ref(), &archive_path, "new.txt")
            .await
            .as_deref(),
        Some(b"fresh".as_slice()),
        "the copied file must land in the remote archive"
    );

    get_volume_manager().unregister(&parent_id);
}
