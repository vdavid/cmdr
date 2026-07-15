//! Synthetic external-drive fixtures for indexing tests (macOS only).
//!
//! Every test that needs a real local external filesystem (FAT32 / exFAT) uses
//! a **disposable disk image** created here, never a physical card. See the
//! "Testing external drives" note in [`DETAILS.md`] for the incident that makes
//! this non-negotiable: a `diskutil unmount` on a physical FAT32 card wedged
//! macOS 26's userspace FSKit `msdos` service mid-unmount and kernel-panicked
//! the machine.
//!
//! Safety rules this module enforces, in code:
//! - **Synthetic images only.** [`DiskImageFixture::attach`] always goes through
//!   `hdiutil create` + `hdiutil attach -nobrowse` on a fresh temp file.
//! - **Every `hdiutil` call is hard-timeout-guarded** ([`run_hdiutil_guarded`]):
//!   the child is SIGKILLed past the deadline, so a wedged FSKit service is
//!   killed, never awaited.
//! - **Attach once, detach once.** No mount/unmount cycling. Teardown lives in
//!   [`DiskImageFixture`]'s `Drop`, so it runs even on panic or early return:
//!   `hdiutil detach`, then a `hdiutil detach -force` fallback, each guarded.
//!
//! The human-run reference probes this ports are
//! `docs/specs/local-drive-indexing-probes/fat32-probe.sh` and
//! `fsevents-probe.swift`.
//!
//! [`DETAILS.md`]: ./DETAILS.md

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// Hard cap on any single `hdiutil` invocation. Past this the child is
/// SIGKILLed. Generous (a healthy create/attach/detach is ~1-2 s); the point is
/// to never sit forever on a wedged FSKit service, not to be tight.
const HDIUTIL_TIMEOUT: Duration = Duration::from_secs(30);

/// Image size. FAT32's floor is ~32 MB; 64 MB clears it with headroom while
/// staying quick to create and attach.
const IMAGE_SIZE: &str = "64m";

/// The filesystem to format a synthetic image with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskImageFilesystem {
    /// FAT32 (`hdiutil` `MS-DOS FAT32`). No journal; derived, unstable inodes.
    Fat32,
    /// exFAT (`hdiutil` `ExFAT`). Same non-journaled, inode-untrusted class.
    ExFat,
}

impl DiskImageFilesystem {
    /// The `-fs` argument `hdiutil create` expects.
    fn hdiutil_fs(self) -> &'static str {
        match self {
            DiskImageFilesystem::Fat32 => "MS-DOS FAT32",
            DiskImageFilesystem::ExFat => "ExFAT",
        }
    }
}

/// One entry in the known tree [`DiskImageFixture::populate_known_tree`] writes.
/// `size` is meaningless (and `0`) for directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownTreeEntry {
    /// Path relative to the mount point (no leading slash).
    pub rel_path: &'static str,
    pub is_dir: bool,
    pub size: u64,
}

/// An attached synthetic disk image. Dropping it detaches the image (guarded),
/// then deletes the backing temp file, so teardown is panic- and
/// early-return-safe.
///
/// Construct with [`DiskImageFixture::attach`]. The image lives under a private
/// temp dir; nothing is ever mounted browsable (`-nobrowse`).
pub struct DiskImageFixture {
    /// Backing temp dir holding the `.dmg`. Dropped *after* detach (struct Drop
    /// runs before field drops), so the image file outlives the detach.
    _work_dir: tempfile::TempDir,
    /// Whole-disk `/dev/diskN` node to detach (parent, not the `sN` partition).
    device: String,
    /// `/Volumes/…` mount point.
    mount_point: PathBuf,
}

impl DiskImageFixture {
    /// Create a synthetic image formatted `fs`, attach it `-nobrowse`, and parse
    /// out its device node and mount point.
    ///
    /// `volume_name` becomes the FAT/exFAT volume label (keep it short and
    /// space-free; it also shapes the `/Volumes/…` path).
    pub fn attach(fs: DiskImageFilesystem, volume_name: &str) -> io::Result<Self> {
        let work_dir = tempfile::tempdir()?;
        let image_path = work_dir.path().join("fixture.dmg");

        // create: -layout MBRSPUD matches the proven FAT32 probe; it also works
        // for exFAT. A single FAT/exFAT partition on an MBR scheme.
        let create = run_hdiutil_guarded(&[
            "create",
            "-size",
            IMAGE_SIZE,
            "-fs",
            fs.hdiutil_fs(),
            "-volname",
            volume_name,
            "-layout",
            "MBRSPUD",
            &image_path.to_string_lossy(),
        ])?;
        if !create.status.success() {
            return Err(hdiutil_error("create", &create));
        }

        // attach -nobrowse: never surface the image in Finder/other apps.
        let attach = run_hdiutil_guarded(&["attach", &image_path.to_string_lossy(), "-nobrowse"])?;
        if !attach.status.success() {
            return Err(hdiutil_error("attach", &attach));
        }
        let attach_out = String::from_utf8_lossy(&attach.stdout);

        let (device, mount_point) = parse_attach_output(&attach_out).ok_or_else(|| {
            io::Error::other(format!(
                "could not parse device + mount point from hdiutil attach:\n{attach_out}"
            ))
        })?;

        let fixture = Self {
            _work_dir: work_dir,
            device,
            mount_point,
        };

        // The image is attached from here on; any early return past this point
        // still detaches via Drop. Confirm the mount point is actually there.
        if !fixture.mount_point.is_dir() {
            return Err(io::Error::other(format!(
                "mount point {} is not a directory after attach",
                fixture.mount_point.display()
            )));
        }

        Ok(fixture)
    }

    /// The `/Volumes/…` mount point of the attached image.
    pub fn mount_point(&self) -> &Path {
        &self.mount_point
    }

    /// Populate a small, known directory tree under the mount: nested dirs, a
    /// couple of sized files, and an **empty** file (the empty file matters for
    /// later inode-sentinel work — on FAT/exFAT it gets a sentinel inode that
    /// changes once content is written).
    ///
    /// Returns the entries it wrote (relative paths, dir flag, byte size) so
    /// callers can assert against them.
    pub fn populate_known_tree(&self) -> io::Result<Vec<KnownTreeEntry>> {
        let entries = known_tree_layout();
        for entry in &entries {
            let full = self.mount_point.join(entry.rel_path);
            if entry.is_dir {
                std::fs::create_dir_all(&full)?;
            } else {
                if let Some(parent) = full.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&full, vec![b'a'; entry.size as usize])?;
            }
        }
        Ok(entries)
    }
}

impl Drop for DiskImageFixture {
    fn drop(&mut self) {
        // Detach once, guarded. On any failure/timeout, try -force (also
        // guarded). Never panic in Drop; log and move on. The backing temp dir
        // is dropped after this, deleting the image file.
        match run_hdiutil_guarded(&["detach", &self.device]) {
            Ok(out) if out.status.success() => return,
            Ok(out) => {
                log::warn!(
                    target: "indexing::external_drive_fixture",
                    "hdiutil detach {} failed ({}); forcing",
                    self.device,
                    out.status
                );
            }
            Err(e) => {
                log::warn!(
                    target: "indexing::external_drive_fixture",
                    "hdiutil detach {} errored ({e}); forcing",
                    self.device
                );
            }
        }

        match run_hdiutil_guarded(&["detach", "-force", &self.device]) {
            Ok(out) if out.status.success() => {}
            Ok(out) => log::warn!(
                target: "indexing::external_drive_fixture",
                "hdiutil detach -force {} failed ({}); device may still be attached",
                self.device,
                out.status
            ),
            Err(e) => log::warn!(
                target: "indexing::external_drive_fixture",
                "hdiutil detach -force {} errored ({e}); device may still be attached",
                self.device
            ),
        }
    }
}

/// The fixed tree [`DiskImageFixture::populate_known_tree`] writes. Kept as a
/// free function so tests can assert against it without an attached image.
pub fn known_tree_layout() -> Vec<KnownTreeEntry> {
    vec![
        KnownTreeEntry {
            rel_path: "docs",
            is_dir: true,
            size: 0,
        },
        KnownTreeEntry {
            rel_path: "docs/readme.txt",
            is_dir: false,
            size: 11,
        },
        KnownTreeEntry {
            rel_path: "docs/nested",
            is_dir: true,
            size: 0,
        },
        KnownTreeEntry {
            rel_path: "docs/nested/data.bin",
            is_dir: false,
            size: 4096,
        },
        KnownTreeEntry {
            rel_path: "photos",
            is_dir: true,
            size: 0,
        },
        KnownTreeEntry {
            rel_path: "photos/empty.dat",
            is_dir: false,
            size: 0,
        },
        KnownTreeEntry {
            rel_path: "top.bin",
            is_dir: false,
            size: 2048,
        },
    ]
}

/// Run `hdiutil <args>` with a hard [`HDIUTIL_TIMEOUT`]. Past the deadline the
/// child is SIGKILLed (`Child::kill` sends `SIGKILL` on Unix) and a `TimedOut`
/// error returned, so a wedged FSKit service is killed rather than awaited.
///
/// hdiutil's output is a handful of lines, well under the OS pipe buffer, so
/// reading it after the process exits can't deadlock on a full pipe.
fn run_hdiutil_guarded(args: &[&str]) -> io::Result<Output> {
    let mut child = Command::new("hdiutil")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let deadline = Instant::now() + HDIUTIL_TIMEOUT;
    loop {
        match child.try_wait()? {
            Some(status) => {
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                if let Some(mut out) = child.stdout.take() {
                    out.read_to_end(&mut stdout)?;
                }
                if let Some(mut err) = child.stderr.take() {
                    err.read_to_end(&mut stderr)?;
                }
                return Ok(Output { status, stdout, stderr });
            }
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill(); // SIGKILL on Unix
                    let _ = child.wait();
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        format!(
                            "hdiutil {} exceeded {HDIUTIL_TIMEOUT:?} and was killed",
                            args.first().unwrap_or(&"")
                        ),
                    ));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

/// Build an `io::Error` from a failed `hdiutil` invocation, quoting stderr.
fn hdiutil_error(stage: &str, out: &Output) -> io::Error {
    io::Error::other(format!(
        "hdiutil {stage} failed ({}): {}",
        out.status,
        String::from_utf8_lossy(&out.stderr).trim()
    ))
}

/// Parse `hdiutil attach` output into (whole-disk device node, mount point).
///
/// Output is tab-separated columns like:
/// ```text
/// /dev/disk4          \tFDisk_partition_scheme\t
/// /dev/disk4s1        \tWindows_FAT_32        \t/Volumes/CMDRTEST
/// ```
/// The first `/dev/…` node is the whole disk; [`normalize_whole_disk`] reduces
/// any partition node to it anyway, so detaching it tears down every child. The
/// mount point is the `/Volumes/…` field.
fn parse_attach_output(output: &str) -> Option<(String, PathBuf)> {
    let mut device: Option<String> = None;
    let mut mount_point: Option<PathBuf> = None;

    for line in output.lines() {
        let first = line.split_whitespace().next().unwrap_or("");

        if device.is_none() && first.starts_with("/dev/") {
            device = Some(first.to_string());
        }

        if mount_point.is_none()
            && let Some(idx) = line.find("/Volumes/")
        {
            mount_point = Some(PathBuf::from(line[idx..].trim_end()));
        }
    }

    match (device, mount_point) {
        (Some(d), Some(m)) => Some((normalize_whole_disk(&d), m)),
        _ => None,
    }
}

/// Reduce a `/dev/diskNsM` partition node to its whole-disk `/dev/diskN`
/// parent, so a detach tears down the whole image in one call. A node that is
/// already whole-disk passes through unchanged.
fn normalize_whole_disk(device: &str) -> String {
    // e.g. "/dev/disk4s1" -> "/dev/disk4"; "/dev/disk4" -> "/dev/disk4".
    if let Some(rest) = device.strip_prefix("/dev/disk") {
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() {
            return format!("/dev/disk{digits}");
        }
    }
    device.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::watcher::{DriveWatcher, FsChangeEvent};
    use std::collections::BTreeSet;
    use tokio::sync::mpsc;

    /// Longest we'll wait for a single live FSEvents callback. Below the 30 s
    /// nextest cap this module runs under, so a merely-slow event still lands
    /// while the cap stays a hang-catcher.
    const FSEVENT_WAIT: Duration = Duration::from_secs(15);

    #[test]
    fn parses_hdiutil_attach_output() {
        let sample = "/dev/disk4          \tFDisk_partition_scheme         \t\n\
                      /dev/disk4s1        \tWindows_FAT_32                 \t/Volumes/CMDRTEST\n";
        let (device, mount) = parse_attach_output(sample).expect("parse");
        assert_eq!(device, "/dev/disk4");
        assert_eq!(mount, PathBuf::from("/Volumes/CMDRTEST"));
    }

    #[test]
    fn normalizes_partition_node_to_whole_disk() {
        assert_eq!(normalize_whole_disk("/dev/disk9s1"), "/dev/disk9");
        assert_eq!(normalize_whole_disk("/dev/disk12"), "/dev/disk12");
        assert_eq!(normalize_whole_disk("/dev/disk3s2s1"), "/dev/disk3");
    }

    /// Smoke test (per `test-infra-smoke-first`): attach a FAT32 image, read the
    /// populated tree back via `std::fs`, then assert the mount point is gone
    /// after the guard's Drop-time detach.
    #[test]
    #[ignore = "attaches a real FAT32 disk image via hdiutil; run with --run-ignored"]
    fn fat32_fixture_attaches_populates_reads_and_detaches() {
        let mount = {
            let fixture = DiskImageFixture::attach(DiskImageFilesystem::Fat32, "CMDRFAT32").expect("attach FAT32");
            let mount = fixture.mount_point().to_path_buf();
            assert!(
                mount.starts_with("/Volumes/"),
                "mount point under /Volumes: {}",
                mount.display()
            );

            let written = fixture.populate_known_tree().expect("populate tree");

            // Read every entry back through std::fs and check kind + size.
            for entry in &written {
                let full = mount.join(entry.rel_path);
                let meta = std::fs::symlink_metadata(&full).unwrap_or_else(|e| panic!("stat {}: {e}", full.display()));
                assert_eq!(meta.is_dir(), entry.is_dir, "kind of {}", entry.rel_path);
                if !entry.is_dir {
                    assert_eq!(meta.len(), entry.size, "size of {}", entry.rel_path);
                }
            }

            // The empty file really is empty (sentinel-inode case for later work).
            assert_eq!(
                std::fs::metadata(mount.join("photos/empty.dat"))
                    .expect("empty stat")
                    .len(),
                0
            );

            mount
        }; // fixture dropped here -> guarded detach

        // Give the OS a moment to tear the mount down, then assert it's gone.
        // Detach is synchronous, but the /Volumes entry can linger briefly.
        for _ in 0..50 {
            if !mount.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(
            !mount.exists(),
            "mount point {} should be gone after detach",
            mount.display()
        );
    }

    /// exFAT variant of the attach/populate/detach smoke test, so the later
    /// inode-trust work can be exercised on both filesystems.
    #[test]
    #[ignore = "attaches a real exFAT disk image via hdiutil; run with --run-ignored"]
    fn exfat_fixture_attaches_populates_and_detaches() {
        let mount = {
            let fixture = DiskImageFixture::attach(DiskImageFilesystem::ExFat, "CMDREXFAT").expect("attach exFAT");
            let mount = fixture.mount_point().to_path_buf();
            let written = fixture.populate_known_tree().expect("populate tree");

            let got: BTreeSet<_> = std::fs::read_dir(&mount)
                .expect("read mount")
                .filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| !n.starts_with('.')) // skip .fseventsd, .Spotlight-V100, etc.
                .collect();
            // Top-level entries from the known tree must all be present.
            for entry in &written {
                if !entry.rel_path.contains('/') {
                    assert!(
                        got.contains(entry.rel_path),
                        "top-level {} present, got {got:?}",
                        entry.rel_path
                    );
                }
            }
            mount
        };

        for _ in 0..50 {
            if !mount.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(
            !mount.exists(),
            "mount point {} should be gone after detach",
            mount.display()
        );
    }

    /// Live-FSEvents regression probe (ports `fsevents-probe.swift` over the
    /// app's own `DriveWatcher`): FAT has no `.fseventsd` journal, so `sinceWhen`
    /// replay is impossible, but **live** FSEvents still fire. Arm a watcher on
    /// the mount, mutate it, and assert a live event is delivered. If a future
    /// macOS release broke live delivery on FAT, this fails loudly rather than
    /// silently making external indexes never update.
    #[tokio::test]
    #[ignore = "attaches a real FAT32 disk image via hdiutil; run with --run-ignored"]
    async fn live_fsevents_fire_on_fat32_despite_no_journal() {
        assert_live_fsevents_fire(DiskImageFilesystem::Fat32, "CMDRFATEV").await;
    }

    /// exFAT counterpart of the live-FSEvents probe.
    #[tokio::test]
    #[ignore = "attaches a real exFAT disk image via hdiutil; run with --run-ignored"]
    async fn live_fsevents_fire_on_exfat() {
        assert_live_fsevents_fire(DiskImageFilesystem::ExFat, "CMDREXFEV").await;
    }

    /// Shared body: attach `fs`, arm a `DriveWatcher` on its mount, then keep
    /// creating files until a live event referencing the mount arrives (redoing
    /// the mutation covers the just-armed window where the first event can be
    /// dropped, mirroring the repo's self-healing real-FSEvents tests).
    async fn assert_live_fsevents_fire(fs: DiskImageFilesystem, volume_name: &str) {
        let fixture = DiskImageFixture::attach(fs, volume_name).expect("attach image");
        let mount = fixture.mount_point().to_path_buf();

        let (tx, mut rx) = mpsc::channel::<FsChangeEvent>(256);
        // since_when = 0 -> "since now": live delivery only, no replay (FAT/exFAT
        // have no journal to replay from anyway).
        let mut watcher = DriveWatcher::start(&mount, 0, tx).expect("start watcher");

        let deadline = Instant::now() + FSEVENT_WAIT;
        let mount_str = mount.to_string_lossy().to_string();
        let mut seen = false;
        let mut seq = 0u32;

        while Instant::now() < deadline {
            // Produce an observable mutation on the mount on each pass.
            let probe = mount.join(format!("fsevent_probe_{seq}.txt"));
            std::fs::write(&probe, b"fsevent").expect("write probe file");
            seq += 1;

            // Wait briefly for an event referencing this mount.
            match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
                Ok(Some(event)) => {
                    if event.path.contains(&*mount_str) || event.path.contains("/Volumes/") {
                        seen = true;
                        break;
                    }
                    // Unrelated event; keep going.
                }
                Ok(None) => break,  // sender dropped
                Err(_) => continue, // no event yet; mutate again
            }
        }

        watcher.stop();
        assert!(
            seen,
            "expected a live FSEvents callback for a mutation on {mount_str} (no journal, but live events must fire)"
        );
    }

    /// End-to-end mount-relative scan on a REAL FAT32 image: the one flow no other
    /// test covers on an actual `msdos` filesystem. Attaches a synthetic FAT32
    /// image, populates a known tree, then drives the production local scan pipeline
    /// (`scan_volume` with the `MountRooted` exclusion scope and FAT's
    /// untrusted-inode flag, built from an `IndexPathSpace` exactly as `manager.rs`
    /// does for a `LocalExternal` drive) into a store, and asserts the drive's own
    /// index reflects the tree.
    ///
    /// This pins the exclusion + mount-relative core against a real mount: with the
    /// boot-disk-scope exclusions, a `/Volumes/X`-rooted scan excluded its own
    /// subtree and false-completed with 0 entries; here the full tree must land
    /// under `ROOT_ID` by mount-relative name with recursive sizes aggregated. It
    /// also verifies FAT's derived inodes are nulled in the index, the behavior
    /// unique to a real FAT/exFAT mount.
    #[test]
    #[ignore = "attaches a real FAT32 disk image via hdiutil; run with --run-ignored"]
    fn fat32_mount_relative_scan_indexes_the_tree_with_sizes_and_null_inodes() {
        use crate::indexing::IndexPathSpace;
        use crate::indexing::scanner::{ScanConfig, scan_volume};
        use crate::indexing::store::{CURRENT_EPOCH_KEY, IndexStore, ROOT_ID};
        use crate::indexing::writer::{IndexWriter, WriteMessage};

        // The index DB lives on a normal temp dir, never on the FAT mount (which
        // can't host a reliable SQLite WAL). Seed current_epoch = 1 so the scan
        // stamps real listed epochs, matching a production fresh scan.
        let db_dir = tempfile::tempdir().expect("temp db dir");
        let db_path = db_dir.path().join("external-scan.db");
        IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");
        writer
            .send(WriteMessage::UpdateMeta {
                key: CURRENT_EPOCH_KEY.to_string(),
                value: "1".to_string(),
            })
            .expect("seed epoch");
        writer.flush_blocking().expect("flush epoch seed");

        let fixture = DiskImageFixture::attach(DiskImageFilesystem::Fat32, "CMDRSCAN").expect("attach FAT32");
        let known = fixture.populate_known_tree().expect("populate tree");
        let mount = fixture.mount_point().to_path_buf();

        // Build the scan config the way the manager does for a LocalExternal drive.
        // Inode-trust is resolved from the mount's REAL filesystem, exactly as
        // `local_external_index::classify` does in production: a FAT mount detects
        // as inode-untrusted, so the space carries that fact.
        let fs = crate::file_system::filesystem_kind::detect_filesystem_for_path(&mount);
        let inodes_trustworthy = fs.kind.has_stable_inodes();
        assert!(
            !inodes_trustworthy,
            "a real FAT32 mount must detect as inode-untrusted (fs kind {:?})",
            fs.kind
        );
        let space = IndexPathSpace::mount_rooted(mount.to_string_lossy().to_string())
            .with_inodes_trustworthy(inodes_trustworthy);
        let config = ScanConfig {
            root: mount.clone(),
            scope: space.exclusion_scope(),
            inodes_trustworthy: space.inodes_trustworthy(),
            ..ScanConfig::default()
        };

        // Run the scan to completion: the scanner thread walks the mount, then sends
        // the mark + ComputeAllAggregates messages before returning, so joining it
        // and flushing the writer yields a fully aggregated index.
        let (_handle, join) = scan_volume(config, &writer).expect("start scan");
        let summary = join.join().expect("scan thread panicked").expect("scan ok");
        assert!(!summary.was_cancelled, "scan ran to completion, not cancelled");
        writer.flush_blocking().expect("flush aggregates");

        // Known-tree minimums (macOS may add AppleDouble `._*` sidecars on FAT, so
        // assert lower bounds, never exact counts): 4 files, 3 dirs, and the summed
        // logical bytes of the known files.
        let known_files = known.iter().filter(|e| !e.is_dir).count() as u64;
        let known_dirs = known.iter().filter(|e| e.is_dir).count() as u64;
        let known_bytes: u64 = known.iter().filter(|e| !e.is_dir).map(|e| e.size).sum();

        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        let root_stats = IndexStore::get_dir_stats_by_id(&conn, ROOT_ID)
            .expect("dir stats query")
            .expect("root has aggregated stats (NOT a false-complete empty scan)");
        assert!(
            root_stats.recursive_file_count >= known_files,
            "all {known_files} known files indexed under the mount root (got {})",
            root_stats.recursive_file_count
        );
        assert!(
            root_stats.recursive_dir_count >= known_dirs,
            "all {known_dirs} known dirs indexed (got {})",
            root_stats.recursive_dir_count
        );
        assert!(
            root_stats.recursive_logical_size >= known_bytes,
            "recursive size >= {known_bytes} known bytes (got {})",
            root_stats.recursive_logical_size
        );

        // Mount-relative resolution: `docs` is a real child of ROOT_ID (resolved by
        // mount-relative name, not walked from an absolute /Volumes/... chain), and
        // its subtree size covers its two files (readme 11 + nested/data.bin 4096).
        let docs_id = IndexStore::resolve_component(&conn, ROOT_ID, "docs")
            .expect("resolve docs")
            .expect("docs resolves as a child of the mount root");
        let docs_stats = IndexStore::get_dir_stats_by_id(&conn, docs_id)
            .expect("docs stats query")
            .expect("docs has stats");
        assert!(
            docs_stats.recursive_logical_size >= 4107,
            "docs subtree covers readme + nested/data.bin (got {})",
            docs_stats.recursive_logical_size
        );

        // FAT inode nulling: a real FAT file's derived inode is untrustworthy, so
        // the scanner stores `inode: None`, keeping the live rename pre-pass inert.
        // Verify on a real msdos file, not a simulated one.
        let top_id = IndexStore::resolve_component(&conn, ROOT_ID, "top.bin")
            .expect("resolve top.bin")
            .expect("top.bin resolves under the mount root");
        let top_row = IndexStore::get_entry_by_id(&conn, top_id)
            .expect("entry query")
            .expect("top.bin row present");
        assert!(
            top_row.inode.is_none(),
            "FAT32's derived inode must be nulled in the index (got {:?})",
            top_row.inode
        );

        // fixture drops here -> guarded hdiutil detach (the writer holds no handle
        // on the mount; the DB is on a separate temp dir).
    }
}
