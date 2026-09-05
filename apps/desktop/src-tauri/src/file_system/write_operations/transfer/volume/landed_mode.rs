//! What permission bits a cross-volume copy leaves on a file it wrote to a
//! LOCAL destination.
//!
//! **Why this exists.** A `run.sh` inside a git snapshot or a release zip is
//! executable, and every backend that knows its mode reports it on
//! `FileEntry::permissions`. Nothing carried that across until here: the bytes
//! go through `Volume::write_from_stream`, which creates a plain new file, so a
//! script copied out of a branch or extracted from a zip landed at `0o644` and
//! the user had to `chmod` it themselves. The volume reports the bit; applying
//! it is the copy engine's job, and this module is where it happens.
//!
//! **What travels.** The source's low nine bits, folded through the mode the
//! destination filesystem just gave the fresh temp ([`landed_mode`]). setuid,
//! setgid, and sticky never travel: a routed volume's mode is metadata out of a
//! repo object or an archive header, which is not authority enough for one of
//! those. The local-FS-to-local-FS copy keeps them (macOS `copyfile` with
//! `COPYFILE_STAT`), and that asymmetry is deliberate — there both sides ARE the
//! filesystem.
//!
//! **Where it happens.** On the STAGED temp, before the rename that gives the
//! file its name, so the visible file never appears with one mode and flips to
//! another. A mode that can't be set is logged at debug and dropped: the bytes
//! are the copy, and a FAT destination that ignores `chmod` is no reason to fail
//! one.

use std::path::Path;
use std::sync::Arc;

use crate::file_system::volume::Volume;

/// The permission bits a landed file should wear, or `None` when it should keep
/// the ones it already has.
///
/// `source_mode` is what the source volume reported (`FileEntry::permissions`,
/// where `0` means "this backend has no permission concept"); `created_mode` is
/// what the destination filesystem gave the fresh file it just made.
///
/// The fold is the one `git checkout` and every unzip apply, and it is what
/// keeps this from widening anything: `created_mode` already has the user's
/// umask baked into it, so intersecting with it re-applies that umask, and the
/// execute bits are allowed exactly where the matching read bit survived. A
/// `0o755` source lands `0o755` under the usual `022` umask and `0o700` under a
/// `077` one, a `0o600` source stays private, and the `0o666` a Windows-made zip
/// reports (a DOS read-only flag wearing a mode's clothes, not a mode anybody
/// set) folds back to exactly the `0o644` a plain new file would have had.
pub(super) fn landed_mode(source_mode: u32, created_mode: u32) -> Option<u32> {
    let wanted = source_mode & 0o777;
    if wanted == 0 {
        // Either the backend has no permission concept, or the file genuinely
        // has no bits set. Both read as "nothing to carry": a landed file at
        // mode 0 is one the user can't open, and no copy should produce that.
        return None;
    }
    let created = created_mode & 0o777;
    // Execute is allowed wherever read survived the umask — `chmod +x`'s own
    // rule against a umask, and the reason a `0o755` source doesn't land
    // group-executable under a `077` one.
    let permitted = created | ((created & 0o444) >> 2);
    let landed = wanted & permitted;
    (landed != created).then_some(landed)
}

/// Puts the source's mode on the file the copy just wrote, when the destination
/// is a real local filesystem and the source had a mode to report.
///
/// Best-effort by design, and silent on failure past a debug line: the bytes are
/// the copy. `written` is the path the bytes went to — the staged `.cmdr-tmp-*`
/// under the usual staging, so the rename that follows hands the user a file
/// that has never worn the wrong mode.
///
/// `source_mode` is the mode the CALLER already had in hand (the merge walker
/// lists its children, so every deep file's mode is free). `None` means it had
/// no listing, which is the top-level dispatch's position; this then asks the
/// source for one, and that stat is the only round trip this module ever adds.
/// It is spent once per top-level FILE, only when the destination is local, only
/// when the SOURCE says it has modes at all (`Volume::reports_posix_mode`), and
/// only after that file's bytes have already crossed. So a 10,000-file pull off
/// an SMB share, which has no modes, spends nothing.
pub(super) async fn apply_source_mode(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    source_mode: Option<u32>,
    dest_volume: &Arc<dyn Volume>,
    written: &Path,
) {
    // Only a real local filesystem has a mode to set. A remote backend's write
    // path owns whatever permissions its server applies, and inventing a `chmod`
    // over SMB or MTP would be a claim this layer has no standing to make.
    let Some(root) = dest_volume.local_path() else {
        return;
    };

    let mode = match source_mode {
        Some(mode) => mode,
        // No listing in hand — the top-level dispatch's position. Ask the source,
        // but ONLY when it has something to answer with: over SMB or MTP that
        // stat is a round trip per selected file, spent to be told `0`.
        // `reports_posix_mode` costs nothing and settles it. (A backend that has
        // modes still answers `0` for an entry that recorded none; this only says
        // asking is worth a trip.)
        None if source_volume.reports_posix_mode() => match source_volume.get_metadata(source_path).await {
            Ok(entry) => entry.permissions,
            Err(e) => {
                log::debug!(
                    target: "copy",
                    "landed mode: couldn't read the mode of {} ({e}); leaving {} as created",
                    source_path.display(),
                    written.display(),
                );
                return;
            }
        },
        None => return,
    };
    if mode & 0o777 == 0 {
        return;
    }

    // The same anchoring `LocalPosixVolume::resolve` applies, so this is the
    // path `write_from_stream` actually wrote.
    let absolute = cmdr_fs::volume::root_anchored(&root, written);
    // Two local syscalls, on a thread that has just finished streaming a whole
    // file: cheaper than the hop `spawn_blocking` would cost, and this is a
    // tokio worker rather than the main thread.
    apply_mode_to_local_file(&absolute, mode);
}

/// `chmod`s one local path to the mode [`landed_mode`] derives, or leaves it
/// alone. Split out so the fold can be exercised against a real file without a
/// volume in the picture.
fn apply_mode_to_local_file(absolute: &Path, source_mode: u32) {
    use std::os::unix::fs::PermissionsExt;

    let created = match std::fs::metadata(absolute) {
        Ok(meta) => meta.permissions().mode(),
        Err(e) => {
            log::debug!(
                target: "copy",
                "landed mode: couldn't stat {} ({e}); leaving it as created",
                absolute.display(),
            );
            return;
        }
    };
    let Some(landed) = landed_mode(source_mode, created) else {
        return;
    };
    if let Err(e) = std::fs::set_permissions(absolute, std::fs::Permissions::from_mode(landed)) {
        // A destination that doesn't do modes (FAT, exFAT, some network mounts)
        // refuses this, and that is not a failed copy.
        log::debug!(
            target: "copy",
            "landed mode: couldn't set {landed:o} on {} ({e}); the bytes are there either way",
            absolute.display(),
        );
    }
}

#[cfg(test)]
#[path = "landed_mode_tests.rs"]
mod tests;
