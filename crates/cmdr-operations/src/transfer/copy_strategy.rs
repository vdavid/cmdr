//! Copy strategy selection for file operations: which mechanism moves the bytes
//! ([`select_local_copy_strategy`]) and running a chosen one ([`copy_file_using`]).
//!
//! **Every arm stages.** The bytes go to a `.cmdr-tmp-*` sibling and take the
//! destination's real name by one same-directory rename, via
//! `overwrite::stage_and_land_file`. ❌ Never add a mechanism that writes
//! straight to the destination: a local destination path must never hold a
//! partial file. Rationale and the no-clobber rule: `DETAILS.md` § "Local
//! copies stage".
//!
//! The only reason to use platform-native copy APIs (`copyfile(3)`, `copy_file_range(2)`) is
//! filesystem-level cloning (APFS clonefile, btrfs/XFS reflink): instant, zero-cost copies
//! that create a copy-on-write pointer instead of copying bytes. In all other cases, our chunked
//! copy is equivalent in speed and strictly better for progress reporting and cancellation.
//!
//! Strategy (macOS):
//! - Same APFS volume → `copyfile(3)` with `COPYFILE_CLONE` for instant clonefile
//! - Everything else → chunked copy (1 MB chunks, cancellation between chunks)
//!
//! Strategy (Linux):
//! - Local, non-network → `copy_file_range(2)` (kernel handles reflink on btrfs/XFS)
//! - Network → chunked copy
//!
//! We evaluated `copyfile` on non-APFS filesystems (HFS+, exFAT, FAT32, NTFS-3G) and found no
//! practical benefit: no clonefile support, and no metadata edge either, since `copyfile` without
//! a clone doesn't set birth time and the chunked arm carries birth and flags itself
//! (`cmdr_fs::metadata`). NTFS-3G is FUSE-based and has the same buffering issues as network
//! mounts. See `DETAILS.md` § "Copy strategy selection".

#[cfg(target_os = "windows")]
use std::fs;
use std::path::Path;
use std::sync::Arc;

#[cfg(target_os = "linux")]
use super::linux_copy::copy_single_file_linux;
#[cfg(target_os = "macos")]
use super::macos_copy::{CopyProgressContext, copy_single_file_native};

#[cfg(target_os = "windows")]
use super::super::error_classification::IoResultExt;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::LandedMetadata;

use super::super::host::Announced;
use super::super::overwrite::{PostLanding, Staged, stage_and_land_file};
use super::super::state::WriteOperationState;
use super::super::types::WriteOperationError;
use super::chunked_copy::ChunkedCopyProgressFn;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use super::chunked_copy::chunked_copy_with_metadata;
#[cfg(target_os = "linux")]
use super::chunked_copy::is_network_filesystem;

// ============================================================================
// macOS: APFS clonefile detection
// ============================================================================

/// Returns true if source and dest are on the same APFS volume (clonefile is possible).
///
/// Checks two things:
/// 1. Same volume via `st_dev` (device ID from `stat`), same approach as `is_same_filesystem`
/// 2. Filesystem type is APFS via `statfs.f_fstypename`
///
/// Handles non-existent destination paths by checking the parent directory.
#[cfg(target_os = "macos")]
fn is_same_apfs_volume(source: &Path, dest: &Path) -> bool {
    use cmdr_fs::file_identity::{Sameness, same_device};

    // Check same volume via device ID (works even when dest doesn't exist: we check parent)
    let src_meta = match std::fs::metadata(source) {
        Ok(m) => m,
        Err(_) => return false,
    };
    let dest_check_path = if dest.exists() {
        dest.to_path_buf()
    } else {
        match dest.parent() {
            Some(p) if p.exists() => p.to_path_buf(),
            _ => return false,
        }
    };
    let dest_meta = match std::fs::metadata(&dest_check_path) {
        Ok(m) => m,
        Err(_) => return false,
    };
    if same_device(&src_meta, &dest_meta) != Sameness::Same {
        return false;
    }

    // Same volume: now check if it's APFS (only APFS supports clonefile)
    is_apfs(source)
}

/// Returns true if the path is on an APFS volume.
#[cfg(target_os = "macos")]
fn is_apfs(path: &Path) -> bool {
    use std::ffi::CString;

    let c_path = match CString::new(path.to_string_lossy().as_bytes()) {
        Ok(p) => p,
        Err(_) => return false,
    };
    // SAFETY: `libc::statfs` is a C struct that's valid fully zeroed; this is the out-buffer.
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c_path` is a valid NUL-terminated C string from `path`, and `&mut stat` is a valid
    // pointer to the correctly-typed out-buffer the kernel fills on success.
    if unsafe { libc::statfs(c_path.as_ptr(), &mut stat) } != 0 {
        return false;
    }
    // SAFETY: `statfs` returned 0, so the kernel initialized `stat.f_fstypename`, a NUL-terminated
    // `[c_char; 16]`; reading it as a `CStr` is valid for the lifetime of `stat`.
    let fstype = unsafe { std::ffi::CStr::from_ptr(stat.f_fstypename.as_ptr()).to_string_lossy() };
    fstype == "apfs"
}

// ============================================================================
// Strategy selection
// ============================================================================

/// Outcome of a single-file copy: bytes written, whether something other than
/// the end-of-op pass already answers for the destination's data, and the
/// rename-blocking metadata a [`PostLanding::HandBack`] caller still owes the
/// file.
///
/// `data_covered` is `true` when the operation's barriers carry the file's sync
/// (chunked copy's `DeviceBarriers::note_written`: synced inline on a network
/// mount, or waiting for the next pass, which the closing flush runs) or when a
/// flush is moot because the data shares copy-on-write extents with the source
/// (APFS clonefile / btrfs-XFS reflink). The caller skips those in the
/// end-of-op data sync so a long chunked batch isn't synced twice. `false`
/// means the bytes may still live only in the page cache (Linux
/// `copy_file_range` without reflink, the `std::fs::copy` fallback), so the
/// caller must flush the destination before reporting completion.
#[derive(Debug)]
pub(super) struct StrategyCopyOutcome {
    pub bytes: u64,
    pub data_covered: bool,
    /// Empty unless the caller asked for [`PostLanding::HandBack`] and the file
    /// has an ACL or flags that would refuse its next rename.
    pub handed_back: LandedMetadata,
}

/// How the bytes of one local file get from source to destination.
///
/// Naming the choice (rather than branching inline) splits "which mechanism
/// fits this source/destination pair" from "run it", so the landing discipline
/// around the write is written once instead of once per branch — and so a test
/// can exercise a mechanism this machine's filesystems would never select.
/// Each platform carries only the variants it can actually pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LocalCopyStrategy {
    /// `copyfile(3)` with `COPYFILE_CLONE`: an instant APFS clone, only
    /// possible within one APFS volume.
    #[cfg(target_os = "macos")]
    AppleClone,
    /// A userspace 1 MiB read/write loop that checks cancellation between
    /// chunks. The fallback wherever a kernel-side copy isn't available or
    /// isn't cancellable in time (network mounts, non-APFS or cross-volume
    /// destinations).
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    Chunked,
    /// `copy_file_range(2)`: kernel-side, and a reflink on btrfs/XFS.
    #[cfg(target_os = "linux")]
    KernelCopyRange,
    /// `std::fs::copy` (`CopyFileExW` underneath on Windows).
    #[cfg(target_os = "windows")]
    StdCopy,
}

/// Picks the mechanism for copying `source` to `dest`.
///
/// The only reason to prefer a platform-native API is filesystem-level cloning;
/// everywhere else the chunked loop is equivalent in speed and strictly better
/// for progress and cancellation. See the module docs.
pub(super) fn select_local_copy_strategy(source: &Path, dest: &Path) -> LocalCopyStrategy {
    #[cfg(target_os = "macos")]
    {
        if is_same_apfs_volume(source, dest) {
            LocalCopyStrategy::AppleClone
        } else {
            LocalCopyStrategy::Chunked
        }
    }
    #[cfg(target_os = "linux")]
    {
        if is_network_filesystem(source) || is_network_filesystem(dest) {
            LocalCopyStrategy::Chunked
        } else {
            LocalCopyStrategy::KernelCopyRange
        }
    }
    #[cfg(target_os = "windows")]
    {
        let _ = (source, dest);
        LocalCopyStrategy::StdCopy
    }
}

/// Copies file contents using the best strategy for the source/destination combination.
pub(super) fn copy_file_with_strategy(
    state: &Arc<WriteOperationState>,
    source: &Path,
    dest: Announced<'_>,
    needs_safe_overwrite: bool,
    post_landing: PostLanding,
    progress_callback: Option<ChunkedCopyProgressFn>,
) -> Result<StrategyCopyOutcome, WriteOperationError> {
    let strategy = select_local_copy_strategy(source, dest.path());
    log::debug!(
        "copy_file_with_strategy: {strategy:?} (src={}, dest={})",
        LogPrivate::file_path(source),
        LogPrivate::file_path(dest.path())
    );
    copy_file_using(
        strategy,
        state,
        source,
        dest,
        needs_safe_overwrite,
        post_landing,
        progress_callback,
    )
}

/// [`copy_file_with_strategy`] with the mechanism chosen by the caller.
///
/// Every arm keeps rename-blocking metadata off the temp: the clone arm's
/// landing clears what the clone carried over (`overwrite::stage_and_land_file`
/// step 3), and the chunked and Linux arms hand their source's ACL (and flags)
/// to the landing instead of writing them onto the temp.
pub(super) fn copy_file_using(
    strategy: LocalCopyStrategy,
    state: &Arc<WriteOperationState>,
    source: &Path,
    dest: Announced<'_>,
    needs_safe_overwrite: bool,
    post_landing: PostLanding,
    progress_callback: Option<ChunkedCopyProgressFn>,
) -> Result<StrategyCopyOutcome, WriteOperationError> {
    // Every arm below writes through `stage_and_land_file`: the bytes go to a
    // `.cmdr-tmp-*` sibling and take the real name by one same-directory rename.
    // ❌ Don't add an arm that writes straight to `dest` — that is the whole
    // hazard this exists to remove (`overwrite.rs`).
    match strategy {
        #[cfg(target_os = "macos")]
        LocalCopyStrategy::AppleClone => {
            let context = CopyProgressContext::with_cancellation(state.intent().clone());
            let landed = stage_and_land_file(state, dest, needs_safe_overwrite, post_landing, |target| {
                copy_single_file_native(source, target, false, Some(&context)).map(Staged::bytes)
            })?;
            // Clonefile shares CoW extents with the source: flushing is moot.
            Ok(StrategyCopyOutcome {
                bytes: landed.bytes,
                data_covered: true,
                handed_back: landed.handed_back,
            })
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        LocalCopyStrategy::Chunked => {
            // Chunked copy notes the file in the operation's barriers, which
            // sync it (inline on a network mount, else at the next pass).
            let landed = stage_and_land_file(state, dest, needs_safe_overwrite, post_landing, |target| {
                chunked_copy_with_metadata(source, target, state, progress_callback)
            })?;
            Ok(StrategyCopyOutcome {
                bytes: landed.bytes,
                data_covered: true,
                handed_back: landed.handed_back,
            })
        }
        #[cfg(target_os = "linux")]
        LocalCopyStrategy::KernelCopyRange => {
            // `copy_file_range(2)` doesn't flush (and reflink shares CoW extents,
            // but we can't cheaply tell here), so the caller flushes the dest.
            // The source's POSIX ACL goes on after the landing, as on every arm.
            let landed = stage_and_land_file(state, dest, needs_safe_overwrite, post_landing, |target| {
                let bytes = copy_single_file_linux(source, target, false, state, progress_callback)?;
                Ok(Staged {
                    bytes,
                    landed: LandedMetadata::of_source_acl(source),
                })
            })?;
            Ok(StrategyCopyOutcome {
                bytes: landed.bytes,
                data_covered: false,
                handed_back: landed.handed_back,
            })
        }
        #[cfg(target_os = "windows")]
        LocalCopyStrategy::StdCopy => {
            let _ = progress_callback; // Unused on this platform
            // The std fallback doesn't flush; the caller's end-of-op pass does.
            let landed = stage_and_land_file(state, dest, needs_safe_overwrite, post_landing, |target| {
                fs::copy(source, target).with_path(source).map(Staged::bytes)
            })?;
            Ok(StrategyCopyOutcome {
                bytes: landed.bytes,
                data_covered: false,
                handed_back: landed.handed_back,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestOperations;
    use cmdr_fs::testing::TestDir;
    use std::fs;
    use std::time::Duration;

    fn create_temp_dir(name: &str) -> TestDir {
        TestDir::new(&format!("copy_strategy_test_{}", name))
    }

    /// A running operation to copy under. Its `intent` is the cancel token and
    /// its liveness owns whatever the copy stages.
    fn running_state(engine: &TestOperations) -> Arc<WriteOperationState> {
        Arc::new(engine.new_operation_state(Duration::from_millis(50)))
    }

    #[test]
    fn test_copy_file_with_strategy_basic() {
        let engine = TestOperations::new();
        let temp_dir = create_temp_dir("basic");
        let src = temp_dir.join("source.txt");
        let dst = temp_dir.join("dest.txt");

        fs::write(&src, "Hello, copy strategy!").unwrap();

        let result = copy_file_with_strategy(
            &running_state(&engine),
            &src,
            Announced::assumed(&dst),
            false,
            PostLanding::Apply,
            None,
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap().bytes, 21);
        assert!(dst.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "Hello, copy strategy!");
    }

    #[test]
    fn test_copy_file_with_strategy_safe_overwrite() {
        let engine = TestOperations::new();
        let temp_dir = create_temp_dir("safe_overwrite");
        let src = temp_dir.join("source.txt");
        let dst = temp_dir.join("dest.txt");

        fs::write(&src, "New content").unwrap();
        fs::write(&dst, "Old content").unwrap();

        let result = copy_file_with_strategy(
            &running_state(&engine),
            &src,
            Announced::assumed(&dst),
            true,
            PostLanding::Apply,
            None,
        );

        assert!(result.is_ok());
        assert!(dst.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "New content");
    }

    #[test]
    fn test_copy_file_with_strategy_preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let engine = TestOperations::new();

        let temp_dir = create_temp_dir("perms");
        let src = temp_dir.join("source.sh");
        let dst = temp_dir.join("dest.sh");

        fs::write(&src, "#!/bin/bash").unwrap();
        fs::set_permissions(&src, fs::Permissions::from_mode(0o755)).unwrap();

        let result = copy_file_with_strategy(
            &running_state(&engine),
            &src,
            Announced::assumed(&dst),
            false,
            PostLanding::Apply,
            None,
        );

        assert!(result.is_ok());
        let dst_perms = fs::metadata(&dst).unwrap().permissions().mode();
        assert_eq!(dst_perms & 0o777, 0o755);
    }

    // ----------------------------------------------------------------------
    // is_apfs / is_same_apfs_volume: mutation-driven survivors.
    //
    // Both helpers were previously only covered indirectly via
    // copy_file_with_strategy. cargo-mutants showed survivors for
    // `is_apfs → true / → false` and the != → == device-id mutant in
    // is_same_apfs_volume. These tests pin the behavior directly.
    // ----------------------------------------------------------------------

    #[cfg(target_os = "macos")]
    #[test]
    fn is_apfs_returns_true_for_typical_macos_paths() {
        // System root and the test's tmp dir are both on APFS in any modern
        // macOS dev / CI box. If both came back as not-APFS, the `== "apfs"`
        // → `!= "apfs"` mutant would survive.
        let temp_dir = create_temp_dir("is-apfs-true");
        assert!(
            is_apfs(&temp_dir),
            "tempfs path on macOS dev box should be APFS. If this fails on a non-APFS bot, gate the test on a precheck."
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn is_apfs_returns_false_for_nonexistent_path() {
        // Kills: replace is_apfs → true. statfs fails → early return false.
        assert!(!is_apfs(Path::new("/nonexistent-volume-xyzzy-12345/file")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn is_same_apfs_volume_true_for_same_apfs_dir_pair() {
        // Source exists, dest doesn't (typical copy precondition): the function
        // falls back to checking the dest's parent. Both should resolve to the
        // same st_dev on APFS, returning true. Kills the != → == mutant on the
        // dev-id comparison.
        let temp_dir = create_temp_dir("same-apfs-pair");
        let src = temp_dir.join("a.txt");
        fs::write(&src, "x").unwrap();
        let dst = temp_dir.join("b.txt"); // doesn't exist
        assert!(
            is_same_apfs_volume(&src, &dst),
            "two paths in the same tmp dir should be on the same APFS volume"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn is_same_apfs_volume_false_when_source_missing() {
        // Kills: replace is_same_apfs_volume → true.
        let temp_dir = create_temp_dir("missing-source");
        let src = temp_dir.join("does-not-exist.txt");
        let dst = temp_dir.join("dest.txt");
        assert!(!is_same_apfs_volume(&src, &dst));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn is_same_apfs_volume_false_when_dest_parent_missing() {
        // Pins the `Some(p) if p.exists() → false` fallback. If the match
        // guard is mutated to `true`, the function would attempt to stat a
        // nonexistent parent.
        let temp_dir = create_temp_dir("missing-parent");
        let src = temp_dir.join("src.txt");
        fs::write(&src, "x").unwrap();
        let dst = Path::new("/nonexistent-parent-xyzzy/child.txt");
        assert!(!is_same_apfs_volume(&src, dst));
    }

    /// #1145: a chunked copy of many small files pays ONE device barrier, the
    /// closing flush's, never one per file (it was an `F_FULLFSYNC` each, ~62 ms
    /// a file on a spinning disk). The interval is pushed out of reach so only
    /// the close counts; on Linux the per-file `fdatasync` already flushed the
    /// drive's cache, so no barrier runs at all.
    #[test]
    fn a_chunked_many_small_file_copy_pays_one_barrier_at_the_close() {
        use crate::durability::flush_created_destinations;
        use crate::event_sinks::CollectorEventSink;
        use crate::types::WriteOperationType;

        const FILES: usize = 100;
        let engine = TestOperations::new();
        let temp_dir = create_temp_dir("barrier-count");
        let (src_dir, dst_dir) = (temp_dir.join("src"), temp_dir.join("dst"));
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dst_dir).unwrap();
        let mut state = engine.new_operation_state(Duration::from_millis(50));
        state.barriers = cmdr_fs::durable::DeviceBarriers::with_interval(Duration::MAX);
        let state = Arc::new(state);

        let mut landed = Vec::new();
        for i in 0..FILES {
            let (src, dst) = (src_dir.join(format!("f{i}.txt")), dst_dir.join(format!("f{i}.txt")));
            fs::write(&src, b"small").unwrap();
            let outcome = copy_file_using(
                LocalCopyStrategy::Chunked,
                &state,
                &src,
                Announced::assumed(&dst),
                false,
                PostLanding::Apply,
                None,
            )
            .unwrap();
            assert!(
                outcome.data_covered,
                "the chunked arm's files are the barriers' to sync"
            );
            landed.push(dst);
        }
        assert_eq!(state.barriers.barriers_issued(), 0, "no barrier while copying");
        let synced_while_copying = state.barriers.synced_paths();
        if cfg!(target_os = "macos") {
            assert_eq!(
                synced_while_copying,
                Vec::<std::path::PathBuf>::new(),
                "a new file costs no sync of its own"
            );
        } else {
            assert_eq!(
                synced_while_copying.len(),
                FILES,
                "Linux's `fdatasync` per file is its barrier"
            );
        }

        let flushed = flush_created_destinations(
            &CollectorEventSink::new(),
            "op-barrier-count",
            WriteOperationType::Copy,
            &state,
            FILES,
            FILES,
            0,
            0,
            &landed,
            &[],
            &landed.iter().cloned().collect(),
        );

        assert_eq!(flushed, Ok(()));
        let expected = if cfg!(target_os = "macos") { 1 } else { 0 };
        assert_eq!(
            state.barriers.barriers_issued(),
            expected,
            // allowed-pluralize-noun: FILES is a 100 constant, never 1
            "{FILES} files on one device cost one closing barrier"
        );
        if cfg!(target_os = "macos") {
            let mut synced = state.barriers.synced_paths();
            synced.sort();
            landed.sort();
            assert_eq!(
                synced, landed,
                "the closing pass syncs each file once, by its final name"
            );
        }
    }

    /// A landing that REPLACES the user's file hands its bytes to the drive
    /// before the rename: a new file's sync can wait for the pass, but renaming
    /// unsynced bytes over the user's file means an OS crash can leave neither
    /// version. The temp's name exists only before the landing, so a sync under
    /// it is a sync before it.
    #[test]
    fn a_chunked_copy_that_replaces_a_file_syncs_before_its_landing() {
        let engine = TestOperations::new();
        let temp_dir = create_temp_dir("replace-sync");
        let (src, dst) = (temp_dir.join("new.txt"), temp_dir.join("theirs.txt"));
        fs::write(&src, b"new bytes").unwrap();
        fs::write(&dst, b"the user's bytes").unwrap();
        let mut state = engine.new_operation_state(Duration::from_millis(50));
        state.barriers = cmdr_fs::durable::DeviceBarriers::with_interval(Duration::MAX);
        let state = Arc::new(state);

        copy_file_using(
            LocalCopyStrategy::Chunked,
            &state,
            &src,
            Announced::assumed(&dst),
            true,
            PostLanding::Apply,
            None,
        )
        .unwrap();

        let synced = state.barriers.synced_paths();
        let before_landing: Vec<&std::path::PathBuf> = synced.iter().filter(|path| **path != dst).collect();
        assert_eq!(before_landing.len(), 1, "one sync under the temp's name: {synced:?}");
        assert_eq!(
            before_landing[0].parent(),
            dst.parent(),
            "the temp is the landing's sibling"
        );
        assert!(!synced.contains(&dst), "nothing left for the pass to sync: {synced:?}");
        assert_eq!(fs::read(&dst).unwrap(), b"new bytes");
    }

    /// The same for the clone arm, whose temp shares the source's extents: the
    /// sync before a replacing landing is cheap there, and no arm skips it.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_cloned_copy_that_replaces_a_file_syncs_before_its_landing() {
        let engine = TestOperations::new();
        let temp_dir = create_temp_dir("replace-sync-clone");
        let (src, dst) = (temp_dir.join("new.txt"), temp_dir.join("theirs.txt"));
        fs::write(&src, b"new bytes").unwrap();
        fs::write(&dst, b"the user's bytes").unwrap();
        let state = running_state(&engine);

        copy_file_using(
            LocalCopyStrategy::AppleClone,
            &state,
            &src,
            Announced::assumed(&dst),
            true,
            PostLanding::Apply,
            None,
        )
        .unwrap();

        let synced = state.barriers.synced_paths();
        assert_eq!(synced.len(), 1, "{synced:?}");
        assert_ne!(synced[0], dst, "synced under the temp's name, before the landing");
        assert_eq!(fs::read(&dst).unwrap(), b"new bytes");
    }

    #[test]
    fn test_copy_file_with_strategy_empty_file() {
        let engine = TestOperations::new();
        let temp_dir = create_temp_dir("empty");
        let src = temp_dir.join("empty.txt");
        let dst = temp_dir.join("dest.txt");

        fs::write(&src, "").unwrap();

        let result = copy_file_with_strategy(
            &running_state(&engine),
            &src,
            Announced::assumed(&dst),
            false,
            PostLanding::Apply,
            None,
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap().bytes, 0);
        assert!(dst.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "");
    }
}
