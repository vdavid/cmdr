//! The walk leaves out AppleDouble sidecars on a volume whose kernel serves them
//! as xattrs, and only there. A `#[path]` child of `scan.rs`.
//!
//! No automated test may attach a FAT or exFAT image (the FSKit guardrail in
//! `crates/cmdr-index/src/indexing/tests/CLAUDE.md`), so the FAT cells play one
//! on APFS by injecting the volume probe. The rule itself is
//! `cmdr_fs::apple_double`, tested there. The last cell runs a whole copy on
//! APFS with the REAL probe, where a `._` file is the person's own.

use crate::host::testing::PANE_ORDER_LISTINGS;
use crate::testing::TestOperations;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cmdr_fs::apple_double::SidecarRule;

use super::super::scan_walker::{WalkContext, walk_sources_with_per_path};
use super::tests::NO_SIDECARS;
use crate::event_sinks::CollectorEventSink;
use crate::transfer::copy::copy_files_with_progress_inner;
use crate::types::WriteOperationConfig;
use cmdr_fs::testing::TestDir;

/// What one walk found: the file names it listed and the bytes it counted.
struct Walked {
    names: Vec<String>,
    file_count: usize,
    total_bytes: u64,
}

fn walk(sources: &[PathBuf], sidecars: &SidecarRule) -> Walked {
    let ctx = WalkContext::<'_, String> {
        listings: &PANE_ORDER_LISTINGS,
        park_while_paused: &|| {},
        progress_interval: Duration::from_secs(3600),
        is_cancelled: &|| false,
        on_io_error: &|_, e| e.to_string(),
        on_cancelled: &|| "Cancelled".to_string(),
        on_symlink_loop: &|path| format!("Symlink loop detected: {}", path.display()),
        on_mount_boundary: None,
        on_progress: &|_, _, _, _, _| {},
        on_file: None,
        online_only: None,
        sidecars,
        mac_only: cmdr_fs::mac_only_files::MacOnlyFiles::Include,
    };
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    let (mut total_bytes, mut dedup_bytes) = (0u64, 0u64);
    let per_path = walk_sources_with_per_path(
        sources,
        &mut files,
        &mut dirs,
        &mut total_bytes,
        &mut dedup_bytes,
        &mut Instant::now(),
        &mut HashSet::new(),
        &mut HashSet::new(),
        None,
        &ctx,
    )
    .expect("the fixture tree walks cleanly");
    let mut names: Vec<String> = files
        .iter()
        .map(|f| {
            f.path
                .file_name()
                .expect("a named entry")
                .to_string_lossy()
                .into_owned()
        })
        .chain(
            dirs.iter()
                .map(|d| format!("{}/", d.file_name().expect("a named dir").to_string_lossy())),
        )
        .collect();
    names.sort();
    Walked {
        names,
        file_count: per_path.iter().map(|(_, scan)| scan.file_count).sum(),
        total_bytes: per_path.iter().map(|(_, scan)| scan.total_bytes).sum(),
    }
}

/// A FAT volume as the walk sees it from APFS: every `._` name is up for pairing.
fn fat() -> SidecarRule {
    SidecarRule::with_probe(|_| true)
}

/// `folder/` holding `child.txt` with its 4,096-byte sidecar, a subfolder with
/// its own, and an orphan sidecar whose owner is gone.
fn fixture(scratch: &Path) -> PathBuf {
    let folder = scratch.join("folder");
    fs::create_dir_all(folder.join("sub")).expect("fixture tree");
    fs::write(folder.join("child.txt"), b"child").expect("the owner");
    fs::write(folder.join("._child.txt"), [0u8; 4096]).expect("its sidecar");
    fs::write(folder.join("._sub"), [0u8; 4096]).expect("the subfolder's sidecar");
    fs::write(folder.join("._orphan.txt"), b"orphan").expect("an orphan");
    fs::write(folder.join("sub/inner.txt"), b"inner").expect("a nested file");
    folder
}

/// The ERR found by the exFAT hand check: `._child.txt` landed as a real file.
#[test]
fn a_walk_on_a_fat_volume_leaves_out_paired_sidecars() {
    let scratch = TestDir::new("sidecar_walk_fat");
    let folder = fixture(&scratch);

    let walked = walk(&[folder], &fat());

    assert_eq!(
        walked.names,
        ["._orphan.txt", "child.txt", "folder/", "inner.txt", "sub/"]
    );
    assert_eq!(walked.file_count, 3, "child, inner, and the orphan");
    assert_eq!(walked.total_bytes, 5 + 5 + 6, "no sidecar bytes in the copy's total");
}

/// APFS: a person's own `._` files are files, paired or not.
#[test]
fn a_walk_on_a_native_xattr_volume_keeps_every_dot_underbar_file() {
    let scratch = TestDir::new("sidecar_walk_native");
    let folder = fixture(&scratch);

    let walked = walk(&[folder], &NO_SIDECARS);

    assert_eq!(
        walked.names,
        [
            "._child.txt",
            "._orphan.txt",
            "._sub",
            "child.txt",
            "folder/",
            "inner.txt",
            "sub/"
        ]
    );
    assert_eq!(walked.total_bytes, 5 + 5 + 6 + 4096 + 4096);
}

/// A sidecar the person SELECTED is theirs to copy: the rule only filters what a
/// listing turns up.
#[test]
fn a_selected_sidecar_still_goes() {
    let scratch = TestDir::new("sidecar_walk_selected");
    let folder = fixture(&scratch);

    let walked = walk(&[folder.join("._child.txt")], &fat());

    assert_eq!(walked.names, ["._child.txt"]);
    assert_eq!(walked.total_bytes, 4096);
}

#[test]
fn a_copy_on_apfs_keeps_dot_underbar_files_with_and_without_a_sibling() {
    let engine = TestOperations::new();
    let tmp = TestDir::new("sidecar_copy_apfs");
    let folder = tmp.join("src/folder");
    let dst = tmp.join("dst");
    fs::create_dir_all(&folder).expect("source folder");
    fs::create_dir_all(&dst).expect("destination");
    fs::write(folder.join("paired.txt"), b"paired").expect("an owner");
    fs::write(folder.join("._paired.txt"), b"mine, not a sidecar").expect("its look-alike");
    fs::write(folder.join("._alone"), b"no sibling").expect("an unpaired one");

    let events = Arc::new(CollectorEventSink::new());
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(50)));
    copy_files_with_progress_inner(
        &*events,
        "op-apfs-dot-underbar",
        &state,
        std::slice::from_ref(&folder),
        &dst,
        &WriteOperationConfig::default(),
    )
    .expect("the copy lands");

    let landed = dst.join("folder");
    assert_eq!(fs::read(landed.join("paired.txt")).expect("the owner"), b"paired");
    assert_eq!(
        fs::read(landed.join("._paired.txt")).expect("the paired one"),
        b"mine, not a sidecar"
    );
    assert_eq!(
        fs::read(landed.join("._alone")).expect("the unpaired one"),
        b"no sibling"
    );
    assert_eq!(events.complete.lock().expect("no poison")[0].files_processed, 3);
}
