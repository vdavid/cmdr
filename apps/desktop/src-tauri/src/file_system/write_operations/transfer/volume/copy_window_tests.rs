//! How wide the concurrent driver's window opens for a `source → dest` pair.
//!
//! `transfer_concurrency` is one line of arithmetic, and the reason it gets its
//! own suite is that a plain `min()` over the two caps quietly hands a NETWORK
//! transfer's window to a CPU-core heuristic: `LocalPosixVolume` reports
//! `clamp(logical_cpus / 2, 4, 16)`, which is 8 on a 16-core M3 Max and 4 on an
//! 8-core Air, and that wins the `min()` against `network.smbConcurrency` on
//! every Mac Cmdr ships to. Measured cost of that on a QNAP over gigabit: a
//! 500-file copy takes 4.700 s at window 4 where it would take 3.522 s at the
//! setting's own default of 10, spreads disjoint
//! (`docs/notes/transfer-concurrency-window-bench-2026-08-02.md`).
//!
//! The other half of this suite is the case that must NOT change: `MtpVolume`
//! reports `max_concurrent_ops() == 1`, and that 1 is what routes a phone to the
//! serial driver. It is a real transport limit, not a heuristic, so it has to
//! keep winning.

use super::*;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::ListingProgress;

/// The four methods `Volume` has no default for. The window formula reads none
/// of them, so a test double answers "nothing here" and stays about the one
/// thing it exists to pin.
macro_rules! no_reads {
    () => {
        fn list_directory<'a>(
            &'a self,
            _path: &'a Path,
            _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
            Box::pin(async { Err(VolumeError::NotSupported) })
        }
        fn get_metadata<'a>(
            &'a self,
            _path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
            Box::pin(async { Err(VolumeError::NotSupported) })
        }
        fn exists<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            Box::pin(async { false })
        }
        fn is_directory<'a>(
            &'a self,
            _path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
            Box::pin(async { Err(VolumeError::NotSupported) })
        }
    };
}

/// A `Volume` that answers only the two questions the window formula asks.
///
/// Everything else takes the trait default, so a test can't accidentally depend
/// on some unrelated capability.
struct CapVolume {
    name: String,
    cap: usize,
    local: bool,
}

impl CapVolume {
    /// A volume whose cap is a LOCAL heuristic (what `LocalPosixVolume` is).
    fn local(name: &str, cap: usize) -> Self {
        Self {
            name: name.to_owned(),
            cap,
            local: true,
        }
    }

    /// A volume whose cap is a real transport limit (SMB's setting, MTP's 1).
    fn remote(name: &str, cap: usize) -> Self {
        Self {
            name: name.to_owned(),
            cap,
            local: false,
        }
    }
}

impl Volume for CapVolume {
    fn name(&self) -> &str {
        &self.name
    }
    fn root(&self) -> &Path {
        Path::new("/")
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn max_concurrent_ops(&self) -> usize {
        self.cap
    }
    fn operations_are_local(&self) -> bool {
        self.local
    }
    no_reads!();
}

/// The defect this whole exercise is about: on an 8-core Mac the local source
/// caps at 4 and the user's `network.smbConcurrency` of 10 never applies.
#[test]
fn a_local_source_cap_does_not_bound_a_network_destination() {
    let source = CapVolume::local("8-core Mac", 4);
    let dest = CapVolume::remote("NAS at smbConcurrency=10", 10);

    assert_eq!(transfer_concurrency(&source, &dest), 10);
}

/// Same defect, other direction: a copy FROM the NAS is bounded by the same
/// core-count heuristic on the receiving end.
#[test]
fn a_local_destination_cap_does_not_bound_a_network_source() {
    let source = CapVolume::remote("NAS at smbConcurrency=10", 10);
    let dest = CapVolume::local("8-core Mac", 4);

    assert_eq!(transfer_concurrency(&source, &dest), 10);
}

/// ❌ The one that must never regress. `MtpVolume::max_concurrent_ops()` is 1
/// because MTP is a single USB bulk transport, and that 1 is what makes
/// `use_concurrent_path` false and routes a phone to the serial driver. A local
/// peer must not be able to widen it.
#[test]
fn an_mtp_destination_still_forces_the_serial_driver() {
    let source = CapVolume::local("16-core Mac", 16);
    let dest = CapVolume::remote("phone over USB", 1);

    assert_eq!(transfer_concurrency(&source, &dest), 1);
}

#[test]
fn an_mtp_source_still_forces_the_serial_driver() {
    let source = CapVolume::remote("phone over USB", 1);
    let dest = CapVolume::local("16-core Mac", 16);

    assert_eq!(transfer_concurrency(&source, &dest), 1);
}

/// Two transports, two real limits: the smaller one is the honest answer.
#[test]
fn two_remote_volumes_still_take_the_smaller_cap() {
    let source = CapVolume::remote("share A", 4);
    let dest = CapVolume::remote("share B", 10);

    assert_eq!(transfer_concurrency(&source, &dest), 4);
}

/// Local→local is the case nothing in the 2026-08-02 sweep measured, so it
/// keeps today's behavior exactly: the smaller CPU heuristic wins. (In
/// production `copy_between_volumes` short-circuits both-local copies to the
/// native local-FS path before the window is ever computed; this pins the
/// formula anyway, so a future caller can't be surprised by it.)
#[test]
fn local_to_local_keeps_the_smaller_cap() {
    let source = CapVolume::local("internal disk", 4);
    let dest = CapVolume::local("external disk", 16);

    assert_eq!(transfer_concurrency(&source, &dest), 4);
}

/// The 32 ceiling is the driver's own bound and outranks either side.
#[test]
fn the_ceiling_still_binds_however_high_a_backend_reports() {
    let source = CapVolume::local("a Mac", 16);
    let dest = CapVolume::remote("an extravagant server", 4096);

    assert_eq!(transfer_concurrency(&source, &dest), MAX_TRANSFER_CONCURRENCY);
}

/// A backend that hasn't declared itself keeps bounding its peer: the default
/// answer to `operations_are_local` is the conservative one.
#[test]
fn a_backend_that_declares_nothing_still_bounds_its_peer() {
    struct Undeclared;
    impl Volume for Undeclared {
        fn name(&self) -> &str {
            "undeclared"
        }
        fn root(&self) -> &Path {
            Path::new("/")
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn max_concurrent_ops(&self) -> usize {
            2
        }
        no_reads!();
    }

    let source = CapVolume::local("a Mac", 16);
    assert_eq!(transfer_concurrency(&source, &Undeclared), 2);
}
