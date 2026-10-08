//! Pure type mapping: SFTP metadata into `FileEntry`, and a `statvfs` answer into `SpaceInfo`.
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::SpaceInfo;
use openssh_sftp_client::fs::Statvfs;
use openssh_sftp_client::metadata::{MetaData, RawFileType};

/// Builds a [`FileEntry`] from one SFTP stat answer.
///
/// ❗ `app_path` is the APP spelling, prefix and all
/// (`sftp://ada@nas.local:22/srv/data/photos`), ❌ never the bare server path.
/// A pane holds what a listing hands it and passes it straight back, and the
/// app anchors it against the volume root on the way (`root_anchored`), so a
/// bare server path would come back doubled. `RemoteRoot::to_app_path` is where
/// it is made.
pub(super) fn metadata_to_file_entry(name: &str, app_path: &str, meta: &MetaData) -> FileEntry {
    let mut entry = entry_of_type(name, app_path, meta.file_type().map(|t| t.as_raw()));
    entry.size = if entry.is_directory { None } else { meta.len() };
    entry.modified_at = meta.modified().and_then(unix_secs);
    // SFTP v3 carries access and modify times and no creation time, so this stays
    // `None` rather than repeating the modify time and calling it a birth date.
    entry
}

/// The kind half of the mapping, split out because only the server can build a
/// `MetaData` that carries a file type.
///
/// ❗ `permissions` carries the `st_mode` FILE-TYPE bits and nothing else. The
/// type is what lets a walker skip a FIFO, socket, or device without opening
/// it (a read on a FIFO blocks the single-threaded `sftp-server` every
/// operation on this volume shares). The permission bits stay unset: copies
/// carry the low nine as a mode (`landed_mode`), and SFTP doesn't report one.
fn entry_of_type(name: &str, app_path: &str, file_type: Option<RawFileType>) -> FileEntry {
    let is_directory = file_type == Some(RawFileType::Directory);
    let is_symlink = file_type == Some(RawFileType::Symlink);
    let mut entry = FileEntry::new(name.to_string(), app_path.to_string(), is_directory, is_symlink);
    // `RawFileType` is `repr(u32)` over the `S_IF*` constants themselves.
    entry.permissions = file_type.map_or(0, |raw| raw as u32);
    entry
}

/// The free and total space of the filesystem a `statvfs@openssh.com` answer describes.
///
/// Read the way `statvfs(3)` defines it, the same as the local backend does:
/// block counts are in `frsize` units (`bsize` only when a server sends a zero
/// `frsize`), the room left is `bavail` (what a non-root account may use), and
/// what's stored is everything that isn't `bfree`. ❗ So `used` is NOT the
/// complement of `available`: root-reserved blocks are in neither, which is why
/// this builds [`SpaceInfo::Bounded`] itself rather than through
/// [`SpaceInfo::bounded`]. Every product saturates, because these numbers come
/// off the wire.
pub(super) fn statvfs_to_space_info(stat: &Statvfs) -> SpaceInfo {
    let unit = if stat.frsize == 0 { stat.bsize } else { stat.frsize };
    let total_bytes = stat.blocks.saturating_mul(unit);
    let free_bytes = stat.bfree.saturating_mul(unit);
    SpaceInfo::Bounded {
        total_bytes,
        available_bytes: stat.bavail.saturating_mul(unit).min(total_bytes),
        used_bytes: total_bytes.saturating_sub(free_bytes),
    }
}

/// Seconds since the epoch, matching `FileEntry`'s own unit.
fn unix_secs(stamp: openssh_sftp_client::UnixTimeStamp) -> Option<u64> {
    stamp
        .as_system_time()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    const S_IFMT: u32 = 0o170_000;

    #[test]
    fn special_files_carry_their_file_type_bits() {
        // A planner that can't tell a FIFO from an empty file opens it, and a
        // read on a FIFO blocks the single-threaded `sftp-server` that every
        // operation on the volume shares.
        for (raw, bits) in [
            (RawFileType::FIFO, 0o010_000),
            (RawFileType::Socket, 0o140_000),
            (RawFileType::CharacterDevice, 0o020_000),
            (RawFileType::BlockDevice, 0o060_000),
        ] {
            let entry = entry_of_type("x", "sftp://h/x", Some(raw));
            assert_eq!(entry.permissions & S_IFMT, bits, "{raw:?}");
            assert!(!entry.is_directory && !entry.is_symlink);
        }
    }

    fn statvfs(frsize: u64, bsize: u64, blocks: u64, bfree: u64, bavail: u64) -> Statvfs {
        Statvfs {
            bsize,
            frsize,
            blocks,
            bfree,
            bavail,
            files: 0,
            ffree: 0,
            favail: 0,
            fsid: 0,
            flag: 0,
            namemax: 255,
        }
    }

    #[test]
    fn space_counts_in_fragments_and_keeps_the_reserved_blocks_out_of_both_figures() {
        // 50 blocks are reserved for root: free to nobody we can be, holding nothing.
        let space = statvfs_to_space_info(&statvfs(4096, 1_048_576, 1000, 300, 250));
        assert_eq!(
            space,
            SpaceInfo::Bounded {
                total_bytes: 4_096_000,
                available_bytes: 1_024_000,
                used_bytes: 2_867_200,
            },
            "`blocks` count in `frsize` units, ❌ never `bsize`, and `used` is what isn't `bfree`"
        );
    }

    #[test]
    fn a_zero_fragment_size_falls_back_to_the_block_size() {
        let space = statvfs_to_space_info(&statvfs(0, 512, 1000, 400, 400));
        assert_eq!(
            space.total_bytes(),
            Some(512_000),
            "a zero unit would report every server as full"
        );
        assert_eq!(space.available_bytes(), Some(204_800));
    }

    #[test]
    fn a_nonsense_answer_saturates_instead_of_wrapping() {
        let space = statvfs_to_space_info(&statvfs(u64::MAX, 0, 2, 3, 3));
        assert_eq!(space.total_bytes(), Some(u64::MAX));
        assert_eq!(
            space.used_bytes(),
            0,
            "more free than total is clamped, never a wrapped huge figure"
        );
    }

    #[test]
    fn the_permission_bits_stay_unset() {
        // Copies read the low nine bits as a mode to carry (`landed_mode`), and
        // SFTP doesn't report one yet: only the type travels.
        for raw in [RawFileType::RegularFile, RawFileType::Directory, RawFileType::Symlink] {
            assert_eq!(entry_of_type("x", "sftp://h/x", Some(raw)).permissions & 0o7777, 0);
        }
        assert_eq!(entry_of_type("x", "sftp://h/x", None).permissions, 0);
    }
}
