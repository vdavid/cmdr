//! What a local copy or move keeps of a file's and a folder's metadata, one cell
//! per (path, class). The contract and its invariants:
//! `volume/DETAILS.md` § "Copies keep the source's metadata".
//!
//! A keep cell asserts the class lands. A cell still pinning a bug or a loss
//! asserts today's outcome and names what would flip it. Every cell that flipped
//! was first written against the target and seen red for the documented reason
//! (the Locked moves: <https://github.com/getcmdr/cmdr/issues/408>).
//!
//! The fixture is `cmdr_fs::testing::metadata_fixture` (macOS). Elsewhere: the
//! extract cell in `volume/copy_extract_out_tests.rs`, the APFS → HFS+ cell in
//! `real_image.rs`, and the SMB cells in `operations_host/backend_suites/network_dates_test_support.rs`.

#[cfg(target_os = "macos")]
mod macos {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    use cmdr_fs::staging::STAGING_TEMP_MARKER;
    use cmdr_fs::testing::metadata_fixture::{
        ACCESSED_SECS, BIRTH_SECS, FINDER_INFO_XATTR, Follow, HIDDEN_FLAG, LOCKED_FLAG, MODIFIED_SECS, MetadataFixture,
        RESOURCE_FORK_XATTR, TAGS_XATTR, accessed_secs, add_acl_entry, birth_secs, flags, has_acl, mode, modified_secs,
        set_flags, set_mode, set_times, set_xattr, xattrs,
    };

    use crate::event_sinks::CollectorEventSink;
    use crate::host::Announced;
    use crate::locked_sources::test_seam::pretend_system_locked;
    use crate::overwrite::PostLanding;
    use crate::state::WriteOperationState;
    use crate::testing::TestOperations;
    use crate::transfer::copy::copy_files_with_progress_inner;
    use crate::transfer::copy_strategy::{
        LocalCopyStrategy, StrategyCopyOutcome, copy_file_using, select_local_copy_strategy,
    };
    use crate::transfer::move_op::test_support::{
        make_state, run_cross_fs_move, run_cross_fs_move_answering, run_cross_fs_move_with, run_same_fs_move,
        run_same_fs_move_answering,
    };
    use crate::types::{
        ConflictResolution, LockedKept, LockedSourcesAnswer, WriteOperationConfig, WriteOperationError,
    };
    use cmdr_fs::ignore_poison::IgnorePoison;

    fn state(engine: &TestOperations) -> Arc<WriteOperationState> {
        Arc::new(engine.new_operation_state(Duration::from_millis(50)))
    }

    /// `dst/` inside the fixture, so source and destination share one APFS volume.
    fn dst_dir(fixture: &MetadataFixture) -> PathBuf {
        let dst = fixture.join("dst");
        std::fs::create_dir_all(&dst).expect("create dst");
        dst
    }

    fn copy_one(
        engine: &TestOperations,
        strategy: LocalCopyStrategy,
        source: &Path,
        dest: &Path,
    ) -> Result<StrategyCopyOutcome, WriteOperationError> {
        copy_file_using(
            strategy,
            &state(engine),
            source,
            Announced::assumed(dest),
            false,
            PostLanding::Apply,
            None,
        )
    }

    /// Asserts the pair takes the clone arm on this machine, so a clone cell can't
    /// pass for the wrong reason.
    fn assert_clone_arm(source: &Path, dest: &Path) {
        assert_eq!(
            select_local_copy_strategy(source, dest),
            LocalCopyStrategy::AppleClone,
            "the fixture's temp dir must be one APFS volume for a clone cell to mean anything"
        );
    }

    /// The `.cmdr-tmp-*` items left in `dir`.
    fn staging_temps(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.to_string_lossy().contains(STAGING_TEMP_MARKER))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn copy_tree(engine: &TestOperations, sources: &[PathBuf], dst: &Path) {
        let events = Arc::new(CollectorEventSink::new());
        let result = copy_files_with_progress_inner(
            &*events,
            "op-metadata-matrix",
            &state(engine),
            sources,
            dst,
            &WriteOperationConfig::default(),
        );
        assert!(result.is_ok(), "the copy itself succeeds: {result:?}");
    }

    // ── Clone arm (same APFS volume) ────────────────────────────────────────

    /// The clone carries `uchg` onto the temp and the landing rename refuses it
    /// (`EPERM`), so the landing clears it, lands, and puts it back on the landed
    /// file. The clone's birth time survives all three steps.
    #[test]
    fn a_locked_file_cloned_within_one_volume_lands_locked_with_its_birth_time() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_clone_locked");
        let source = fixture.locked_file("locked.txt");
        let dst = dst_dir(&fixture);
        let dest = dst.join("locked.txt");
        assert_clone_arm(&source, &dest);

        copy_one(&engine, LocalCopyStrategy::AppleClone, &source, &dest).expect("the clone lands");

        assert_eq!(flags(&dest) & LOCKED_FLAG, LOCKED_FLAG, "the landed file is Locked");
        assert_eq!(birth_secs(&dest), BIRTH_SECS);
        assert_eq!(modified_secs(&dest), MODIFIED_SECS);
        assert_eq!(staging_temps(&dst), Vec::<PathBuf>::new(), "no temp stays behind");
    }

    /// `COPYFILE_ACL` puts the `deny delete` ACL on the temp and the landing
    /// rename refuses it (`EACCES`), so the landing strips it, lands, and puts it
    /// back on the landed file.
    #[test]
    fn a_deny_delete_file_cloned_within_one_volume_lands_with_its_acl() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_clone_acl");
        let source = fixture.deny_delete_file("acl.txt");
        let dst = dst_dir(&fixture);
        let dest = dst.join("acl.txt");
        assert_clone_arm(&source, &dest);

        copy_one(&engine, LocalCopyStrategy::AppleClone, &source, &dest).expect("the clone lands");

        assert!(has_acl(&dest), "the landed file wears the ACL");
        assert_eq!(birth_secs(&dest), BIRTH_SECS);
        assert_eq!(staging_temps(&dst), Vec::<PathBuf>::new(), "no temp stays behind");
    }

    /// The landing rename is the probe: a file with nothing to clear lands with
    /// one `copyfile` and one `rename`, and never reaches the metadata helper.
    #[test]
    fn a_plain_file_through_the_clone_arm_never_calls_the_metadata_helper() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_clone_plain");
        let source = fixture.rich_file("f.txt");
        let dst = dst_dir(&fixture);
        let dest = dst.join("f.txt");
        assert_clone_arm(&source, &dest);
        let before = cmdr_fs::metadata::helper_calls_on_this_thread();

        copy_one(&engine, LocalCopyStrategy::AppleClone, &source, &dest).expect("the clone lands");

        assert_eq!(cmdr_fs::metadata::helper_calls_on_this_thread(), before);
    }

    /// A keep cell: the clone carries every class of a file with no rename-blocker.
    #[test]
    fn a_clone_keeps_every_class_of_a_file_without_a_rename_blocker() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_clone_rich");
        let source = fixture.rich_file("f.txt");
        let dst = dst_dir(&fixture);
        let dest = dst.join("f.txt");
        assert_clone_arm(&source, &dest);

        copy_one(&engine, LocalCopyStrategy::AppleClone, &source, &dest).expect("the clone lands");

        assert_eq!(birth_secs(&dest), BIRTH_SECS);
        assert_eq!(modified_secs(&dest), MODIFIED_SECS);
        assert_eq!(mode(&dest), 0o750);
        assert_eq!(flags(&dest) & HIDDEN_FLAG, HIDDEN_FLAG);
        assert_eq!(xattrs(&dest), xattrs(&source));
    }

    // ── Chunked arm (every other local pair) ────────────────────────────────

    /// The source's ACL goes on after the landing rename, which a `deny delete`
    /// ACL on the temp would refuse (`EACCES`).
    #[test]
    fn a_deny_delete_file_copied_chunked_lands_with_its_acl() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_chunked_acl");
        let source = fixture.deny_delete_file("acl.txt");
        let dst = dst_dir(&fixture);
        let dest = dst.join("acl.txt");

        copy_one(&engine, LocalCopyStrategy::Chunked, &source, &dest).expect("the copy lands");

        assert!(has_acl(&dest), "the landed file wears the ACL");
        assert_eq!(staging_temps(&dst), Vec::<PathBuf>::new(), "no temp stays behind");
    }

    /// Birth and mtime go on in ONE `fsetattrlist`, so APFS can't drag birth down
    /// to the earlier mtime.
    #[test]
    fn a_chunked_copy_keeps_the_files_birth_time() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_chunked_birth");
        let source = fixture.rich_file("f.txt");
        let dest = dst_dir(&fixture).join("f.txt");

        copy_one(&engine, LocalCopyStrategy::Chunked, &source, &dest).expect("the copy lands");

        assert_eq!(birth_secs(&dest), BIRTH_SECS);
        assert_eq!(modified_secs(&dest), MODIFIED_SECS);
    }

    /// The other relation: a file whose birth is LATER than its mtime (a download
    /// that kept its server date) keeps both exactly.
    #[test]
    fn a_chunked_copy_keeps_a_birth_time_later_than_the_mtime() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_chunked_birth_after");
        let source = fixture.join("downloaded.txt");
        std::fs::write(&source, b"born after its mtime").expect("write the source");
        set_times(&source, MODIFIED_SECS, BIRTH_SECS, ACCESSED_SECS, Follow::Yes);
        let dest = dst_dir(&fixture).join("downloaded.txt");

        copy_one(&engine, LocalCopyStrategy::Chunked, &source, &dest).expect("the copy lands");

        assert_eq!(birth_secs(&dest), MODIFIED_SECS);
        assert_eq!(modified_secs(&dest), BIRTH_SECS);
    }

    #[test]
    fn a_chunked_copy_keeps_hidden() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_chunked_hidden");
        let source = fixture.rich_file("f.txt");
        let dest = dst_dir(&fixture).join("f.txt");

        copy_one(&engine, LocalCopyStrategy::Chunked, &source, &dest).expect("the copy lands");

        assert_eq!(flags(&dest) & HIDDEN_FLAG, HIDDEN_FLAG);
    }

    /// `uchg` goes on after the landing rename, which it would refuse (`EPERM`).
    #[test]
    fn a_locked_file_copied_chunked_lands_locked() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_chunked_locked");
        let source = fixture.locked_file("locked.txt");
        let dst = dst_dir(&fixture);
        let dest = dst.join("locked.txt");

        copy_one(&engine, LocalCopyStrategy::Chunked, &source, &dest).expect("the copy lands");

        assert_eq!(flags(&dest) & LOCKED_FLAG, LOCKED_FLAG, "the landed file is Locked");
        assert_eq!(birth_secs(&dest), BIRTH_SECS);
        assert_eq!(staging_temps(&dst), Vec::<PathBuf>::new(), "no temp stays behind");
    }

    /// The temp is created owner-only and takes the source's mode only once its
    /// bytes are in, so a private file is never readable by others mid-copy.
    #[test]
    fn a_private_files_temp_is_never_readable_by_others_mid_copy() {
        use crate::transfer::chunked_copy::chunk_park;

        let engine = TestOperations::new();

        let fixture = MetadataFixture::new("matrix_chunked_private_temp");
        let source = fixture.join("private.txt");
        std::fs::write(&source, vec![7u8; 4096]).expect("write the source");
        std::fs::set_permissions(&source, std::os::unix::fs::PermissionsExt::from_mode(0o600))
            .expect("chmod the source");
        let dst = dst_dir(&fixture);
        let dest = dst.join("private.txt");

        let park = chunk_park::park_after(&source, 0);
        let copy = std::thread::spawn({
            let (source, dest) = (source.clone(), dest.clone());
            move || copy_one(&engine, LocalCopyStrategy::Chunked, &source, &dest).map(|_| ())
        });
        park.wait_until_parked(Duration::from_secs(10))
            .expect("the copy parks mid-file");
        let temps = staging_temps(&dst);
        let modes: Vec<u32> = temps.iter().map(|t| mode(t)).collect();
        park.release();
        copy.join().expect("the copy thread").expect("the copy lands");

        assert_eq!(temps.len(), 1, "one temp while the bytes stream: {temps:?}");
        assert_eq!(modes[0] & 0o077, 0, "the temp is owner-only: {:o}", modes[0]);
        assert_eq!(mode(&dest), 0o600);
    }

    /// A keep cell: what the chunked arm carries today.
    #[test]
    fn a_chunked_copy_keeps_xattrs_mode_mtime_and_atime() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_chunked_keeps");
        let source = fixture.rich_file("f.txt");
        let dest = dst_dir(&fixture).join("f.txt");

        copy_one(&engine, LocalCopyStrategy::Chunked, &source, &dest).expect("the copy lands");

        assert_eq!(xattrs(&dest), xattrs(&source));
        assert_eq!(mode(&dest), 0o750);
        assert_eq!(modified_secs(&dest), MODIFIED_SECS);
        assert_eq!(accessed_secs(&dest), ACCESSED_SECS);
    }

    // ── Symlinks ────────────────────────────────────────────────────────────

    /// A copied link takes the source link's OWN dates (`FSOPT_NOFOLLOW`), and its
    /// target's stay untouched.
    #[test]
    fn a_copied_symlink_keeps_its_own_dates() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_symlink");
        let target = fixture.rich_file("f.txt");
        let link = fixture.symlink("link", &target);
        let dst = dst_dir(&fixture);

        copy_tree(&engine, &[link], &dst);

        let landed = dst.join("link");
        assert!(std::fs::symlink_metadata(&landed).expect("lstat").is_symlink());
        assert_eq!(birth_secs(&landed), BIRTH_SECS);
        assert_eq!(modified_secs(&landed), MODIFIED_SECS);
        assert_eq!(birth_secs(&target), BIRTH_SECS, "the target keeps its own");
    }

    // ── Folders (the native folder pass, strategy-independent) ──────────────

    /// Copies the fixture's rich folder into `dst/`, answering (source, landed).
    fn copy_rich_folder(engine: &TestOperations, fixture: &MetadataFixture) -> (PathBuf, PathBuf) {
        let folder = fixture.rich_folder("dir");
        let dst = dst_dir(fixture);
        copy_tree(engine, std::slice::from_ref(&folder), &dst);
        let landed = dst.join("dir");
        assert!(landed.join("child.txt").is_file(), "the folder's child lands");
        (folder, landed)
    }

    /// Birth and mtime go on in ONE `setattrlist`, once the folder's children
    /// are in, so neither a child nor APFS's birth quirk moves them.
    #[test]
    fn a_copied_folder_keeps_its_birth_time_and_mtime() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_folder_dates");
        let (_, landed) = copy_rich_folder(&engine, &fixture);
        assert_eq!(birth_secs(&landed), BIRTH_SECS);
        assert_eq!(modified_secs(&landed), MODIFIED_SECS);
    }

    /// A `0700` folder stays private (it used to land at the umask default).
    #[test]
    fn a_copied_folder_keeps_its_mode() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_folder_mode");
        let (_, landed) = copy_rich_folder(&engine, &fixture);
        assert_eq!(mode(&landed), 0o700);
    }

    #[test]
    fn a_copied_folder_keeps_its_tags_comment_and_finder_info() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_folder_xattrs");
        let (source, landed) = copy_rich_folder(&engine, &fixture);
        assert_eq!(xattrs(&landed), xattrs(&source));
    }

    #[test]
    fn a_copied_folder_keeps_hidden() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_folder_hidden");
        let (_, landed) = copy_rich_folder(&engine, &fixture);
        assert_eq!(flags(&landed) & HIDDEN_FLAG, HIDDEN_FLAG);
    }

    /// The ACL goes on last, after the folder's children landed.
    #[test]
    fn a_copied_folder_keeps_its_acl() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_folder_acl");
        let (_, landed) = copy_rich_folder(&engine, &fixture);
        assert!(has_acl(&landed));
    }

    /// A custom folder icon is two things: the FinderInfo bit on the folder and
    /// an `Icon\r` file inside it whose resource fork holds the image.
    #[test]
    fn a_folder_with_a_custom_icon_lands_with_its_finder_info_and_icon_file() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_folder_icon");
        let source = fixture.rich_folder("dir");
        let icon = source.join("Icon\r");
        std::fs::write(&icon, b"").expect("write the icon file");
        set_xattr(&icon, RESOURCE_FORK_XATTR, b"icns bytes");
        let dst = dst_dir(&fixture);

        copy_tree(&engine, std::slice::from_ref(&source), &dst);

        let landed = dst.join("dir");
        assert!(
            xattrs(&landed).iter().any(|(name, _)| name == FINDER_INFO_XATTR),
            "FinderInfo kept"
        );
        assert_eq!(
            xattrs(&landed.join("Icon\r")),
            vec![(RESOURCE_FORK_XATTR.to_string(), b"icns bytes".to_vec())],
            "the icon file keeps its resource fork"
        );
    }

    /// Its mode and Locked go on only once its children are in: a `0500` or
    /// Locked folder refuses new entries.
    #[test]
    fn a_locked_read_only_folder_is_copied_full_and_lands_locked() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_folder_locked");
        let source = fixture.locked_read_only_folder("vault");
        let dst = dst_dir(&fixture);

        copy_tree(&engine, std::slice::from_ref(&source), &dst);

        let landed = dst.join("vault");
        assert!(landed.join("a.txt").is_file() && landed.join("inner/b.txt").is_file());
        assert_eq!(mode(&landed), 0o500);
        assert_eq!(flags(&landed) & LOCKED_FLAG, LOCKED_FLAG);
        assert_eq!(birth_secs(&landed), BIRTH_SECS);
    }

    /// A merge adds to the user's folder and leaves it its own metadata.
    #[test]
    fn a_folder_the_copy_merged_into_keeps_its_own_metadata() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_folder_merge");
        let source = fixture.rich_folder("dir");
        let dst = dst_dir(&fixture);
        let existing = dst.join("dir");
        std::fs::create_dir(&existing).expect("create the user's folder");
        let before = (mode(&existing), flags(&existing), birth_secs(&existing));

        copy_tree(&engine, std::slice::from_ref(&source), &dst);

        assert!(existing.join("child.txt").is_file());
        assert_eq!((mode(&existing), flags(&existing), birth_secs(&existing)), before);
        assert_eq!(xattrs(&existing), Vec::new());
        assert!(!has_acl(&existing));
    }

    /// A copy that fails partway dates nothing, and a folder it made is never
    /// wider than its source: born owner-only for a private source, and as open
    /// as the umask allows for a shared one.
    #[test]
    fn a_failed_copy_leaves_its_folders_undated_and_never_wider_than_their_source() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_folder_failed");
        let private = fixture.join("private");
        let shared = fixture.join("shared");
        for (folder, folder_mode) in [(&private, 0o700), (&shared, 0o755)] {
            std::fs::create_dir(folder).expect("create the folder");
            std::fs::write(folder.join("a.txt"), b"a").expect("write a.txt");
            set_mode(folder, folder_mode);
            set_times(folder, BIRTH_SECS, MODIFIED_SECS, ACCESSED_SECS, Follow::Yes);
        }
        // Unreadable, so the copy fails on it after `private/a.txt` landed.
        let unreadable = shared.join("z.txt");
        std::fs::write(&unreadable, b"z").expect("write z.txt");
        set_mode(&unreadable, 0o000);
        let dst = dst_dir(&fixture);
        let umask_default = {
            let fresh = fixture.join("a fresh folder");
            std::fs::create_dir(&fresh).expect("create a reference folder");
            mode(&fresh)
        };

        let result = copy_files_with_progress_inner(
            &*Arc::new(CollectorEventSink::new()),
            "op-metadata-matrix-failed",
            &state(&engine),
            &[private, shared],
            &dst,
            &WriteOperationConfig::default(),
        );

        assert!(result.is_err(), "the unreadable file fails the copy");
        assert!(dst.join("private/a.txt").is_file(), "what landed stays");
        assert_ne!(modified_secs(&dst.join("private")), MODIFIED_SECS, "undated");
        assert_eq!(mode(&dst.join("private")), 0o700);
        assert_eq!(mode(&dst.join("shared")), 0o755 & umask_default);
    }

    // ── Moves ───────────────────────────────────────────────────────────────

    /// Every item under `root` that wears `uchg` or an ACL, `root` excluded.
    fn rename_blocked_items(root: &Path) -> Vec<PathBuf> {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else { return };
            for entry in entries.flatten() {
                let path = entry.path();
                if flags(&path) & LOCKED_FLAG != 0 || has_acl(&path) {
                    out.push(path.clone());
                }
                if entry.file_type().is_ok_and(|t| t.is_dir()) {
                    walk(&path, out);
                }
            }
        }
        let mut found = Vec::new();
        walk(root, &mut found);
        found
    }

    /// The names directly in `dir`, sorted.
    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .expect("list")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// The staged copies land unlocked and ACL-free INSIDE the staging tree, and
    /// take their Locked flag and ACL only once Phase 3 has renamed them into
    /// place. The move asked first, and on Move anyway Phase 4 unlocks each
    /// original right before deleting it, so neither refuses.
    #[test]
    fn a_cross_fs_move_of_a_folder_holding_a_locked_and_a_deny_delete_file_lands_both() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_move_folder");
        let folder = fixture.join("album");
        std::fs::create_dir(&folder).expect("create the folder");
        fixture.locked_file("album/locked.txt");
        fixture.deny_delete_file("album/acl.txt");
        let dst = dst_dir(&fixture);

        let events = run_cross_fs_move(
            &engine,
            std::slice::from_ref(&folder),
            &dst,
            ConflictResolution::Stop,
            "op-matrix-move-folder",
        )
        .expect("the move completes");

        let landed = dst.join("album");
        assert_eq!(flags(&landed.join("locked.txt")) & LOCKED_FLAG, LOCKED_FLAG);
        assert_eq!(birth_secs(&landed.join("locked.txt")), BIRTH_SECS);
        assert!(has_acl(&landed.join("acl.txt")), "the landed file wears the ACL");
        assert!(!folder.exists(), "every original is gone, the folder too");
        assert_eq!(names_in(&dst), vec!["album".to_string()], "the staging folder is gone");
        let asked = events.locked_sources.lock_ignore_poison().clone();
        assert_eq!(asked.len(), 1, "asked once for the whole move");
        assert_eq!(asked[0].locked_count, 2);
    }

    /// Asked before the first byte is staged; on Move anyway the copy lands
    /// Locked and Phase 4 unlocks the original right before deleting it.
    #[test]
    fn a_cross_fs_move_of_a_locked_file_lands_locked_once_the_person_says_move_anyway() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_move_locked");
        let source = fixture.locked_file("locked.txt");
        let dst = dst_dir(&fixture);
        let state = make_state(&engine, 200);
        let events = Arc::new(CollectorEventSink::new());
        let at_the_question = Arc::new(std::sync::Mutex::new(None));
        {
            let (state, dst, seen) = (Arc::clone(&state), dst.clone(), Arc::clone(&at_the_question));
            events.answer_locked_with(move |_| {
                *seen.lock_ignore_poison() = Some(names_in(&dst));
                state.locked_sources.answer(LockedSourcesAnswer::MoveAnyway);
            });
        }

        run_cross_fs_move_with(
            &state,
            events,
            std::slice::from_ref(&source),
            &dst,
            ConflictResolution::Stop,
            "op-matrix-move-locked",
        )
        .expect("the move completes");

        let landed = dst.join("locked.txt");
        assert_eq!(flags(&landed) & LOCKED_FLAG, LOCKED_FLAG, "the landed file is Locked");
        assert_eq!(birth_secs(&landed), BIRTH_SECS);
        assert!(!source.exists(), "the original is gone");
        assert_eq!(
            *at_the_question.lock_ignore_poison(),
            Some(Vec::<String>::new()),
            "asked before anything reached the destination"
        );
    }

    /// Phase 3 fails (the destination refuses new files, so the staged files
    /// can't be renamed into it), and the staging cleanup removes the whole
    /// staging tree: nothing in it wears a rename-blocker, because those only go
    /// on after Phase 3.
    #[test]
    fn a_cross_fs_move_whose_phase_3_fails_leaves_a_removable_staging_tree() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_move_phase3");
        let locked = fixture.locked_file("locked.txt");
        let acl = fixture.deny_delete_file("acl.txt");
        let dst = dst_dir(&fixture);
        // The staging folder is a subdirectory, which this still allows.
        add_acl_entry(&dst, "everyone deny add_file");

        let result = run_cross_fs_move(
            &engine,
            &[locked.clone(), acl.clone()],
            &dst,
            ConflictResolution::Stop,
            "op-matrix-move-phase3",
        );

        // The refusal names a staged item, so Phase 2 did stage both.
        assert!(
            matches!(
                &result,
                Err(WriteOperationError::IoError { path, .. }) if path.contains(cmdr_fs::staging::STAGING_DIR_PREFIX)
            ),
            "Phase 3 can't land a file: {:?}",
            result.err()
        );
        assert!(locked.exists() && acl.exists(), "both originals stay");
        cmdr_fs::testing::wait_until(Duration::from_secs(5), "the staging tree's removal", || {
            std::fs::read_dir(&dst).is_ok_and(|mut entries| entries.next().is_none())
        });
        assert_eq!(rename_blocked_items(&dst), Vec::<PathBuf>::new());
    }

    /// `name/` holding `child.txt`: tags, mode `0700`, the fixture's dates, and
    /// Hidden. Nothing on it refuses Phase 4's removal of the original.
    fn tagged_private_folder(fixture: &MetadataFixture, name: &str) -> PathBuf {
        let folder = fixture.join(name);
        std::fs::create_dir(&folder).expect("create the folder");
        std::fs::write(folder.join("child.txt"), b"child").expect("write the child");
        set_xattr(&folder, TAGS_XATTR, b"bplist00 Red");
        set_mode(&folder, 0o700);
        set_times(&folder, BIRTH_SECS, MODIFIED_SECS, ACCESSED_SECS, Follow::Yes);
        set_flags(&folder, HIDDEN_FLAG);
        folder
    }

    /// The staged folder takes its xattrs and dates in staging, and its ACL and
    /// flags once Phase 3 has renamed it into place.
    #[test]
    fn a_cross_fs_move_of_a_folder_keeps_its_metadata() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_move_folder_metadata");
        let folder = tagged_private_folder(&fixture, "album");
        // Refuses neither Phase 3's rename nor Phase 4's removals.
        add_acl_entry(&folder, "everyone deny add_file");
        let dst = dst_dir(&fixture);

        run_cross_fs_move(
            &engine,
            std::slice::from_ref(&folder),
            &dst,
            ConflictResolution::Stop,
            "op-matrix-move-folder-metadata",
        )
        .expect("the move completes");

        let landed = dst.join("album");
        assert!(landed.join("child.txt").is_file());
        assert_eq!(birth_secs(&landed), BIRTH_SECS);
        assert_eq!(modified_secs(&landed), MODIFIED_SECS);
        assert_eq!(mode(&landed), 0o700);
        assert_eq!(flags(&landed) & HIDDEN_FLAG, HIDDEN_FLAG);
        assert!(xattrs(&landed).iter().any(|(name, _)| name == TAGS_XATTR));
        assert!(has_acl(&landed));
        assert!(!folder.exists(), "the original is gone");
    }

    /// A `0500` folder can't be renamed to another parent and a Locked one can't
    /// be filled, so both go on after Phase 3. On Move anyway Phase 4 unlocks
    /// the original folder, but its MODE still refuses taking its children out
    /// (a permission no Locked answer covers), so the move stops there with the
    /// original kept and Locked again.
    #[test]
    fn a_cross_fs_move_of_a_locked_read_only_folder_lands_it_locked_and_full() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_move_folder_locked");
        let source = fixture.locked_read_only_folder("vault");
        let dst = dst_dir(&fixture);

        let result = run_cross_fs_move(
            &engine,
            std::slice::from_ref(&source),
            &dst,
            ConflictResolution::Stop,
            "op-matrix-move-folder-locked",
        );

        let landed = dst.join("vault");
        assert!(landed.join("a.txt").is_file() && landed.join("inner/b.txt").is_file());
        assert_eq!(mode(&landed), 0o500);
        assert_eq!(flags(&landed) & LOCKED_FLAG, LOCKED_FLAG);
        assert_eq!(birth_secs(&landed), BIRTH_SECS);
        assert!(
            matches!(result, Err(WriteOperationError::PermissionDenied { .. })),
            "Phase 4 can't empty a Locked original: {:?}",
            result.err()
        );
        assert!(source.join("a.txt").exists(), "the original stays");
        assert_eq!(flags(&source) & LOCKED_FLAG, LOCKED_FLAG, "and is Locked again");
        assert_eq!(names_in(&dst), vec!["vault".to_string()], "the staging folder is gone");
    }

    /// On Skip the Locked file stays out of the copy whole: it never reaches the
    /// destination, keeps its folder in the source, and the completion counts
    /// it. Everything else in the folder moves.
    #[test]
    fn a_cross_fs_move_skipping_a_locked_file_moves_the_rest_of_its_folder() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_move_folder_skip");
        let folder = fixture.join("album");
        std::fs::create_dir(&folder).expect("create the folder");
        let locked = fixture.locked_file("album/locked.txt");
        std::fs::write(folder.join("plain.txt"), b"plain").expect("write");
        let dst = dst_dir(&fixture);

        let events = run_cross_fs_move_answering(
            &make_state(&engine, 200),
            std::slice::from_ref(&folder),
            &dst,
            ConflictResolution::Stop,
            "op-matrix-move-folder-skip",
            LockedSourcesAnswer::Skip,
        )
        .expect("the move completes");

        assert!(dst.join("album/plain.txt").is_file());
        assert!(
            !dst.join("album/locked.txt").exists(),
            "the Locked file stayed out of the copy"
        );
        assert!(!folder.join("plain.txt").exists(), "the rest moved");
        assert_eq!(
            flags(&locked) & LOCKED_FLAG,
            LOCKED_FLAG,
            "the original stays, still Locked"
        );
        let complete = events.complete.lock_ignore_poison().clone();
        assert_eq!(complete.len(), 1);
        assert_eq!(complete[0].files_skipped, 1);
        assert_eq!(complete[0].files_processed, 1);
        assert_eq!(
            complete[0].locked_kept,
            Some(LockedKept {
                skipped: 1,
                system_locked: 0
            })
        );
        assert_eq!(names_in(&dst), vec!["album".to_string()], "the staging folder is gone");
    }

    /// A system-locked item can't be unlocked by any answer, so the move never
    /// asks about it: it stays out, the rest moves, and the completion counts it.
    #[test]
    fn a_cross_fs_move_leaves_a_system_locked_file_without_asking() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_move_system_locked");
        let pinned = fixture.join("pinned.txt");
        std::fs::write(&pinned, b"pinned").expect("write");
        pretend_system_locked(&pinned);
        let plain = fixture.join("plain.txt");
        std::fs::write(&plain, b"plain").expect("write");
        let dst = dst_dir(&fixture);

        let events = run_cross_fs_move_with(
            &make_state(&engine, 200),
            Arc::new(CollectorEventSink::new()),
            &[pinned.clone(), plain.clone()],
            &dst,
            ConflictResolution::Stop,
            "op-matrix-move-system-locked",
        )
        .expect("the move completes without an answer");

        assert!(
            events.locked_sources.lock_ignore_poison().is_empty(),
            "nobody was asked"
        );
        assert!(pinned.exists() && !dst.join("pinned.txt").exists());
        assert!(dst.join("plain.txt").is_file() && !plain.exists());
        assert_eq!(
            events.complete.lock_ignore_poison()[0].locked_kept,
            Some(LockedKept {
                skipped: 0,
                system_locked: 1
            })
        );
    }

    /// A move merging into the user's folder renames the staged children into
    /// it and leaves the folder its own metadata.
    #[test]
    fn a_cross_fs_move_merging_into_a_folder_leaves_it_its_own_metadata() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_move_folder_merge");
        let folder = tagged_private_folder(&fixture, "album");
        let dst = dst_dir(&fixture);
        let existing = dst.join("album");
        std::fs::create_dir(&existing).expect("create the user's folder");
        let before = (mode(&existing), flags(&existing), birth_secs(&existing));

        run_cross_fs_move(
            &engine,
            std::slice::from_ref(&folder),
            &dst,
            ConflictResolution::Stop,
            "op-matrix-move-folder-merge",
        )
        .expect("the move completes");

        assert!(existing.join("child.txt").is_file());
        assert_eq!((mode(&existing), flags(&existing), birth_secs(&existing)), before);
        assert_eq!(xattrs(&existing), Vec::new());
    }

    /// Phase 3 skips the folder (a file squats its name), so the staged copy of a
    /// Locked `0500` folder is discarded, and so is the staging folder: in
    /// staging the copy wore neither.
    #[test]
    fn a_cross_fs_move_whose_phase_3_skips_a_folder_discards_its_staged_copy() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_move_folder_phase3");
        let source = fixture.locked_read_only_folder("vault");
        let dst = dst_dir(&fixture);
        std::fs::write(dst.join("vault"), b"squatter").expect("squat the folder's name");

        let result = run_cross_fs_move(
            &engine,
            std::slice::from_ref(&source),
            &dst,
            ConflictResolution::Skip,
            "op-matrix-move-folder-phase3",
        );

        assert!(result.is_ok(), "{:?}", result.err());
        let names: Vec<String> = std::fs::read_dir(&dst)
            .expect("list dst")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["vault".to_string()], "only the squatter stays");
        assert!(dst.join("vault").is_file());
        assert!(source.join("inner/b.txt").exists(), "the original stays");
    }

    /// `rename(2)` refuses a Locked item (`EPERM`), so the move asks first, and
    /// on Move anyway unlocks it, renames it, and locks it again where it landed.
    #[test]
    fn a_same_fs_move_of_a_locked_file_lands_locked_once_the_person_says_move_anyway() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_same_fs_locked");
        let source = fixture.locked_file("locked.txt");
        let dst = dst_dir(&fixture);

        let events = run_same_fs_move(
            &engine,
            std::slice::from_ref(&source),
            &dst,
            ConflictResolution::Stop,
            "op-matrix-same-fs-locked",
        )
        .expect("the move completes");

        let landed = dst.join("locked.txt");
        assert!(!source.exists(), "the original moved");
        assert_eq!(
            flags(&landed) & LOCKED_FLAG,
            LOCKED_FLAG,
            "and is Locked where it landed"
        );
        assert_eq!(birth_secs(&landed), BIRTH_SECS);
        let asked = events.locked_sources.lock_ignore_poison().clone();
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].locked_count, 1);
        assert_eq!(asked[0].locked_sample, vec![source.display().to_string()]);
    }

    /// A `deny delete` ACL refuses the rename too (`EACCES`); Move anyway carries
    /// the ACL to where the item lands.
    #[test]
    fn a_same_fs_move_of_a_deny_delete_file_lands_with_its_acl() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_same_fs_acl");
        let source = fixture.deny_delete_file("acl.txt");
        let dst = dst_dir(&fixture);

        run_same_fs_move(
            &engine,
            std::slice::from_ref(&source),
            &dst,
            ConflictResolution::Stop,
            "op-matrix-same-fs-acl",
        )
        .expect("the move completes");

        assert!(!source.exists());
        assert!(has_acl(&dst.join("acl.txt")), "the landed file wears the ACL");
    }

    /// Merging into the user's folder renames each child on its own, so a Locked
    /// child is asked about and, on Move anyway, lands Locked inside it.
    #[test]
    fn a_same_fs_merge_moves_a_locked_child_into_the_users_folder() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_same_fs_merge_locked");
        let folder = fixture.join("album");
        std::fs::create_dir(&folder).expect("create the folder");
        fixture.locked_file("album/locked.txt");
        let dst = dst_dir(&fixture);
        std::fs::create_dir(dst.join("album")).expect("create the user's folder");

        let events = run_same_fs_move(
            &engine,
            std::slice::from_ref(&folder),
            &dst,
            ConflictResolution::Stop,
            "op-matrix-same-fs-merge-locked",
        )
        .expect("the move completes");

        assert_eq!(flags(&dst.join("album/locked.txt")) & LOCKED_FLAG, LOCKED_FLAG);
        assert!(!folder.exists(), "the emptied source folder went");
        assert_eq!(events.locked_sources.lock_ignore_poison()[0].locked_count, 1);
    }

    /// A folder that lands by one rename takes its Locked contents along
    /// untouched, so nothing inside it is asked about.
    #[test]
    fn a_same_fs_move_of_a_folder_holding_a_locked_file_asks_nothing() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_same_fs_folder_locked_inside");
        let folder = fixture.join("album");
        std::fs::create_dir(&folder).expect("create the folder");
        fixture.locked_file("album/locked.txt");
        let dst = dst_dir(&fixture);

        let events = run_same_fs_move_answering(
            &engine,
            std::slice::from_ref(&folder),
            &dst,
            ConflictResolution::Stop,
            "op-matrix-same-fs-folder-locked-inside",
            LockedSourcesAnswer::Skip,
        )
        .expect("the move completes");

        assert!(
            events.locked_sources.lock_ignore_poison().is_empty(),
            "nobody was asked"
        );
        assert_eq!(flags(&dst.join("album/locked.txt")) & LOCKED_FLAG, LOCKED_FLAG);
        assert!(!folder.exists());
    }

    /// On Skip the Locked file stays exactly where it is, Locked, and the rest
    /// moves; the completion counts it as skipped.
    #[test]
    fn a_same_fs_move_skipping_a_locked_file_moves_the_rest() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_same_fs_skip");
        let locked = fixture.locked_file("locked.txt");
        let plain = fixture.join("plain.txt");
        std::fs::write(&plain, b"plain").expect("write");
        let dst = dst_dir(&fixture);

        let events = run_same_fs_move_answering(
            &engine,
            &[locked.clone(), plain.clone()],
            &dst,
            ConflictResolution::Stop,
            "op-matrix-same-fs-skip",
            LockedSourcesAnswer::Skip,
        )
        .expect("the move completes");

        assert_eq!(
            flags(&locked) & LOCKED_FLAG,
            LOCKED_FLAG,
            "the Locked file stays, still Locked"
        );
        assert!(!dst.join("locked.txt").exists());
        assert!(dst.join("plain.txt").is_file() && !plain.exists(), "the rest moved");
        let complete = events.complete.lock_ignore_poison().clone();
        assert_eq!(complete[0].files_processed, 1);
        assert_eq!(complete[0].files_skipped, 1);
        assert_eq!(
            complete[0].locked_kept,
            Some(LockedKept {
                skipped: 1,
                system_locked: 0
            })
        );
    }

    /// No answer can move a system-locked item, so nobody is asked: it stays.
    #[test]
    fn a_same_fs_move_leaves_a_system_locked_file_without_asking() {
        let engine = TestOperations::new();
        let fixture = MetadataFixture::new("matrix_same_fs_system_locked");
        let pinned = fixture.join("pinned.txt");
        std::fs::write(&pinned, b"pinned").expect("write");
        pretend_system_locked(&pinned);
        let dst = dst_dir(&fixture);

        let events = run_same_fs_move(
            &engine,
            std::slice::from_ref(&pinned),
            &dst,
            ConflictResolution::Stop,
            "op-matrix-same-fs-system-locked",
        )
        .expect("the move completes");

        assert!(
            events.locked_sources.lock_ignore_poison().is_empty(),
            "nobody was asked"
        );
        assert!(pinned.exists() && !dst.join("pinned.txt").exists());
        assert_eq!(
            events.complete.lock_ignore_poison()[0].locked_kept,
            Some(LockedKept {
                skipped: 0,
                system_locked: 1
            })
        );
    }
}

/// Linux has no birth to set, no BSD flags, and no `com.apple.*`: what a copy
/// owes there is `user.*` xattrs and the mode (spec § "Platforms").
#[cfg(target_os = "linux")]
mod linux {
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    use crate::host::Announced;
    use crate::overwrite::PostLanding;
    use crate::testing::TestOperations;
    use crate::transfer::copy_strategy::{LocalCopyStrategy, copy_file_using};
    use cmdr_fs::testing::TestDir;

    const XATTR_NAME: &str = "user.app";
    const XATTR_VALUE: &[u8] = b"app value";

    /// A `0750` file carrying `user.app`, and where its copy goes.
    fn seed(dir: &TestDir) -> (PathBuf, PathBuf) {
        let source = dir.join("f.txt");
        std::fs::write(&source, b"linux file").expect("write the source");
        xattr::set(&source, XATTR_NAME, XATTR_VALUE).expect("this filesystem takes `user.*` xattrs");
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o750)).expect("chmod the source");
        let dst = dir.join("dst");
        std::fs::create_dir(&dst).expect("create dst");
        (source, dst.join("f.txt"))
    }

    fn copy(strategy: LocalCopyStrategy, source: &Path, dest: &Path) {
        let engine = TestOperations::new();
        let state = Arc::new(engine.new_operation_state(Duration::from_millis(50)));
        copy_file_using(
            strategy,
            &state,
            source,
            Announced::assumed(dest),
            false,
            PostLanding::Apply,
            None,
        )
        .expect("the copy lands");
    }

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).expect("stat").permissions().mode() & 0o7777
    }

    /// A keep cell: the chunked arm lists and copies every xattr.
    #[test]
    fn a_chunked_copy_keeps_user_xattrs_and_mode() {
        let dir = TestDir::new("matrix_linux_chunked");
        let (source, dest) = seed(&dir);

        copy(LocalCopyStrategy::Chunked, &source, &dest);

        assert_eq!(
            xattr::get(&dest, XATTR_NAME).expect("read").as_deref(),
            Some(XATTR_VALUE)
        );
        assert_eq!(mode(&dest), 0o750);
    }

    /// `copy_file_range` moves bytes only; the arm adds the `user.*` xattrs, the
    /// mode, and the dates.
    #[test]
    fn a_kernel_copy_range_copy_keeps_user_xattrs() {
        let dir = TestDir::new("matrix_linux_kernel_xattrs");
        let (source, dest) = seed(&dir);

        copy(LocalCopyStrategy::KernelCopyRange, &source, &dest);

        assert_eq!(
            xattr::get(&dest, XATTR_NAME).expect("read").as_deref(),
            Some(XATTR_VALUE)
        );
    }

    /// A keep cell: the arm sets the source's mode on what it wrote.
    #[test]
    fn a_kernel_copy_range_copy_keeps_the_mode() {
        let dir = TestDir::new("matrix_linux_kernel_mode");
        let (source, dest) = seed(&dir);

        copy(LocalCopyStrategy::KernelCopyRange, &source, &dest);

        assert_eq!(mode(&dest), 0o750);
    }

    /// A folder takes its mode and `user.*` xattrs once its children landed.
    #[test]
    fn a_copied_folder_keeps_its_mode_and_user_xattrs() {
        use crate::event_sinks::CollectorEventSink;
        use crate::transfer::copy::copy_files_with_progress_inner;
        use crate::types::WriteOperationConfig;

        let engine = TestOperations::new();

        let dir = TestDir::new("matrix_linux_folder");
        let folder = dir.join("album");
        std::fs::create_dir(&folder).expect("create the folder");
        std::fs::write(folder.join("child.txt"), b"child").expect("write the child");
        xattr::set(&folder, XATTR_NAME, XATTR_VALUE).expect("this filesystem takes `user.*` xattrs");
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).expect("chmod the folder");
        let dst = dir.join("dst");
        std::fs::create_dir(&dst).expect("create dst");

        let result = copy_files_with_progress_inner(
            &*Arc::new(CollectorEventSink::new()),
            "op-matrix-linux-folder",
            &Arc::new(engine.new_operation_state(Duration::from_millis(50))),
            &[folder],
            &dst,
            &WriteOperationConfig::default(),
        );

        assert!(result.is_ok(), "{result:?}");
        let landed = dst.join("album");
        assert!(landed.join("child.txt").is_file());
        assert_eq!(mode(&landed), 0o700);
        assert_eq!(
            xattr::get(&landed, XATTR_NAME).expect("read").as_deref(),
            Some(XATTR_VALUE)
        );
    }
}
