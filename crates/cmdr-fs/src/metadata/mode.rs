//! The mode a ROUTED copy lands with: what a local destination puts on a file
//! or folder whose source is a volume (an archive, a git snapshot, a phone), as
//! opposed to another local filesystem.
//!
//! **Why a fold and not the source's bits.** A routed volume's mode is metadata
//! out of a repo object, an archive header, or a device's `stat`, which isn't
//! authority enough to widen anything or to set setuid, setgid, or sticky. So
//! the source's low nine bits are folded through the mode a fresh item would
//! get under the user's umask ([`routed_mode`]), the rule `git checkout` and
//! every unzip apply. A local-to-local copy keeps the exact bits instead: there
//! both sides ARE the filesystem.
//!
//! **Why the umask is read once, by a probe.** A staged temp is created
//! owner-only (`0600`) so a private file is never readable by others while its
//! bytes stream, which makes the temp's own mode useless as a umask proxy.
//! `umask(2)` can't be read without setting it, and setting it, even for an
//! instant, widens every file another thread creates meanwhile. So the first
//! caller makes one throwaway folder with mode `0777` and reads back what the
//! umask left of it.

use std::sync::OnceLock;

/// The permission bits a routed copy leaves on what it wrote, or `None` when
/// the source said nothing (`source_mode`'s low nine bits are 0: either the
/// backend has no permission concept, or the item genuinely has no bits set,
/// and an item nobody can open is no copy's answer).
///
/// `default_mode` is what a fresh item of the same kind gets under the user's
/// umask ([`default_file_mode`], [`default_folder_mode`]). The fold keeps the
/// source's bits that the umask lets through, and execute wherever the matching
/// read survived (`chmod +x`'s own rule): a `0o755` source lands `0o755` under a
/// `022` umask and `0o700` under a `077` one, a `0o600` source stays private,
/// and the `0o666` a Windows-made zip reports folds to `0o644`.
pub fn routed_mode(source_mode: u32, default_mode: u32) -> Option<u32> {
    let wanted = source_mode & 0o777;
    if wanted == 0 {
        return None;
    }
    let created = default_mode & 0o777;
    let permitted = created | ((created & 0o444) >> 2);
    Some(wanted & permitted)
}

/// The mode a new file gets under this process's umask (`0o666 & !umask`).
pub fn default_file_mode() -> u32 {
    0o666 & !umask()
}

/// The mode a new folder gets under this process's umask (`0o777 & !umask`).
pub fn default_folder_mode() -> u32 {
    0o777 & !umask()
}

/// The umask assumed when the probe can't run: the macOS and Linux default.
pub(super) const FALLBACK_UMASK: u32 = 0o022;

fn umask() -> u32 {
    static UMASK: OnceLock<u32> = OnceLock::new();
    *UMASK.get_or_init(super::platform::probe_umask)
}

#[cfg(test)]
#[path = "mode_tests.rs"]
mod tests;
