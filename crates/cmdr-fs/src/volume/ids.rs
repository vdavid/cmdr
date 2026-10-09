//! Volume ID helpers: the ONE funnel every volume ID is built through.
//!
//! A volume ID keys per-volume state that outlives the mount: `index-{id}.db`
//! (plus its `importance-` and `media-` siblings), `lastUsedPaths`, tab
//! `volumeId` fields, and the `VolumeManager` registry. So an ID has to be
//! *identity*: two volumes must never share one, and one volume must keep the
//! same ID across remount, rename, and reboot.
//!
//! ## The shape: `{scheme}-{slug}-{digest}`
//!
//! Every derived ID carries a scheme (which kind of identity it came from), a
//! lossy human-readable slug (for logs and for eyeballing a data dir), and a
//! 64-bit BLAKE3 digest over the canonical identity tuple. The digest carries
//! the uniqueness, the slug carries the readability: a display concern never
//! decides identity. The digest also bounds the length, which matters because
//! these become filename components (255 bytes on macOS and Linux).
//!
//! Schemes, best identity first:
//!
//! - `root`: the boot volume. Unique by definition, and special-cased in enough
//!   places ([`DEFAULT_VOLUME_ID`]) that it stays a bare literal.
//! - `vol-`: a local volume keyed by its filesystem UUID ([`local_volume_id`]).
//!   The real answer to "which disk is this": it survives a remount at a
//!   different mount point, a rename, and a reboot.
//! - `smb-`: an SMB mount keyed by (server, port, share) ([`smb_volume_id`]).
//! - `sftp-`: an SFTP server keyed by (host, port, username)
//!   ([`sftp_volume_id`]).
//! - `webdav-`: a WebDAV server keyed by (host, port, username)
//!   ([`webdav_volume_id`]).
//! - `s3-`: one place on an S3 account (a bucket, or the account root), keyed by
//!   (host, port, access key id, bucket) ([`s3_volume_id`]).
//! - `mtp-`: an MTP device keyed by its serial (`super::mtp_ids`).
//! - `path-`: the fallback when nothing better exists ([`path_volume_id`]),
//!   keyed by the mount path. Stable only as long as the mount path is.
//!
//! [`VolumeScheme::of`] reads the scheme back, and it's the only thing that
//! may: route on the enum, never on a `starts_with` of your own.
//!
//! ❌ Never build a volume ID by hand, and never build one by STRIPPING
//! characters. Stripping is a many-to-one map, so it hands two volumes the same
//! identity: `/Volumes/My Disk` and `/Volumes/My_Disk` both reduce to
//! `volumesmydisk`. Add a constructor here instead.

use super::DEFAULT_VOLUME_ID;

/// Hex chars of BLAKE3 that every derived ID ends in: 64 bits. A birthday
/// collision needs ~2^32 volumes on one machine, and a *chosen* collision (name
/// a USB stick so it steals the boot volume's index) needs ~2^64 work against a
/// cryptographic hash. Both are far past what a file manager has to defend.
const DIGEST_HEX_LEN: usize = 16;

/// Longest slug an ID carries. Enough to recognize a volume in a data-dir
/// listing without letting a deep mount path push `index-{id}.db` toward the
/// 255-byte filename limit.
const SLUG_MAX_CHARS: usize = 24;

/// The human-readable half of an ID: alphanumerics kept and lowercased,
/// everything else collapsed to a single `-`, trimmed, and capped at
/// [`SLUG_MAX_CHARS`]. Lossy ON PURPOSE (the digest is what distinguishes),
/// which is why nothing may key off it.
fn slug(source: &str) -> String {
    let mut out = String::with_capacity(SLUG_MAX_CHARS);
    let mut pending_dash = false;
    for ch in source.chars() {
        if out.chars().count() >= SLUG_MAX_CHARS {
            break;
        }
        if ch.is_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.extend(ch.to_lowercase());
        } else {
            pending_dash = true;
        }
    }
    out
}

/// The identity half of an ID: [`DIGEST_HEX_LEN`] hex chars of BLAKE3 over the
/// scheme and every canonical part.
///
/// Each part goes in length-prefixed, so no two different tuples can hash the
/// same bytes (without it, `("ab", "c")` and `("a", "bc")` would). The scheme
/// goes in first as domain separation, so an SMB share can't collide with a
/// mount path that happens to canonicalize identically.
fn digest(scheme: &str, parts: &[&str]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(scheme.as_bytes());
    for part in parts {
        hasher.update(&(part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    hasher.finalize().to_hex()[..DIGEST_HEX_LEN].to_string()
}

/// Assemble `{scheme}-{slug}-{digest}` (or `{scheme}-{digest}` when the slug
/// comes out empty, as it does for a mount path of only punctuation).
///
/// `slug_source` is cosmetic; `canonical_parts` is the identity. Pass every part
/// that distinguishes this volume from another, already case-folded wherever
/// folding is semantically right (see [`smb_volume_id`]).
fn derived_id(scheme: VolumeScheme, slug_source: &str, canonical_parts: &[&str]) -> String {
    let scheme = scheme.minted_tag();
    let digest = digest(scheme, canonical_parts);
    let slug = slug(slug_source);
    if slug.is_empty() {
        format!("{scheme}-{digest}")
    } else {
        format!("{scheme}-{slug}-{digest}")
    }
}

/// Build the ID for a local volume, preferring its filesystem UUID.
///
/// `uuid` is the volume UUID the platform reports (`NSURLVolumeUUIDStringKey` on
/// macOS, `/dev/disk/by-uuid` on Linux), or `None` where there isn't one:
/// tmpfs, most FUSE mounts, some disk images. `mount_path` is where it's mounted
/// right now, used for the fallback and for the slug.
///
/// With a UUID the ID is stable across mount points, so plugging the same disk
/// in while `/Volumes/Backup` is taken (macOS mounts it at `/Volumes/Backup 1`)
/// keeps its index and its `lastUsedPaths` entry instead of orphaning both and
/// forcing a rescan.
///
/// # Gotcha: a byte-for-byte volume clone reports the SAME UUID
///
/// Two clones mounted at once genuinely collide, and no UUID scheme can tell
/// them apart. `VolumeManager::register` catches it (same ID, two different
/// roots) and logs it rather than silently cross-wiring their state.
pub fn local_volume_id(uuid: Option<&str>, mount_path: &str) -> String {
    if mount_path == "/" {
        return DEFAULT_VOLUME_ID.to_string();
    }
    match uuid.map(str::trim).filter(|u| !u.is_empty()) {
        // UUIDs are case-insensitive hex, so fold before hashing: the same volume
        // must not get two IDs because two APIs disagree on case.
        Some(uuid) => {
            let folded = uuid.to_lowercase();
            derived_id(VolumeScheme::Local, &folded, &[&folded])
        }
        None => path_volume_id(mount_path),
    }
}

/// Build the fallback ID for a mount that reports no stable identity, keyed by
/// its mount path.
///
/// Only as stable as the path: rename the volume or let the OS mount it
/// somewhere else and the ID changes, orphaning that volume's index and saved
/// paths. Prefer [`local_volume_id`] with a real UUID wherever the platform has
/// one. The path goes into the digest verbatim (no Unicode normalization): it
/// comes from the kernel, which is self-consistent about how it spells a mount
/// point.
pub fn path_volume_id(mount_path: &str) -> String {
    if mount_path == "/" {
        return DEFAULT_VOLUME_ID.to_string();
    }
    derived_id(VolumeScheme::Path, mount_path, &[mount_path])
}

/// Build the ID for an SMB mount, keyed by the mount rather than the path shape.
///
/// Path-derived IDs would give two SMB shares with the same case-folded name on
/// different servers (a NAS sharing `Public`, a Docker container sharing
/// `public`) one ID, cross-contaminating `lastUsedPaths`, tab `volumeId` fields,
/// and every other per-volume state; the wrong-cased paths then flow into
/// `SmbVolume::list_directory` and the server answers
/// `STATUS_OBJECT_PATH_NOT_FOUND`. Keying by (server, port, share) prevents it
/// at the root.
///
/// # Case and normalization folding
///
/// Server and share are lowercased before hashing, which is canonicalization
/// rather than loss: DNS hostnames are case-insensitive, and so are SMB share
/// names (Windows and Samba default), so `Naspolya`/`naspolya` and
/// `Public`/`public` really are the same mount. The port is literal.
///
/// Both are NFC-folded first, for the same reason and against a worse failure.
/// macOS `statfs` spells an accented name decomposed while mDNS and the server's
/// share list spell it composed, so one visible share reaches the two upgrade
/// paths as two byte strings. Two IDs for one share splits its `index-{id}.db`,
/// its `lastUsedPaths`, and every tab's `volumeId` down whichever path happened
/// to register it first.
pub fn smb_volume_id(server: &str, port: u16, share: &str) -> String {
    use crate::name_fold::fold_name;

    let server = fold_name(server);
    let share = fold_name(share);
    let port = port.to_string();
    derived_id(
        VolumeScheme::Smb,
        &format!("{server}-{port}-{share}"),
        &[&server, &port, &share],
    )
}

/// Build the ID for an SFTP volume, keyed by the ACCOUNT on the server rather
/// than by the directory it's rooted at.
///
/// Two accounts on one host see different files under the same absolute paths,
/// and this ID keys durable state — `index-{id}.db`, `lastUsedPaths`, tab
/// `volumeId` fields — so folding them together would hand one account's index
/// to the other and send its saved paths somewhere they don't resolve. Getting
/// this wrong is a migration later rather than a bug fix, which is why the
/// username is in the tuple from the start.
///
/// The remote root is deliberately NOT in the tuple: re-rooting the same account
/// deeper into the same server is the same storage, and keying on it would strand
/// the index every time someone browses in from a different starting directory.
///
/// # Case folding
///
/// The host is lowercased, because DNS hostnames are case-insensitive. The
/// username is NOT: POSIX accounts are case-sensitive, so `Ada` and `ada` can be
/// two people. The port is literal.
pub fn sftp_volume_id(host: &str, port: u16, username: &str) -> String {
    let host = host.to_lowercase();
    let port = port.to_string();
    derived_id(
        VolumeScheme::Sftp,
        &format!("{host}-{port}-{username}"),
        &[&host, &port, username],
    )
}

/// Build the ID for a WebDAV server from its (host, port, username) triple.
///
/// The same tuple and the same reasons as [`sftp_volume_id`]: two accounts on one
/// server see different files under the same paths, so the username is part of
/// the identity; the base URL's path (the remote root) is addressing rather than
/// identity, so re-rooting the same account deeper into the same server keeps
/// its index and saved paths. The scheme is NOT in the tuple either: `http` and
/// `https` to one host and port are the same server, and the port already tells
/// the two default listeners apart.
///
/// # Case folding
///
/// The host is lowercased (DNS hostnames are case-insensitive); the username is
/// NOT (an account may be case-sensitive on the server). The port is literal.
pub fn webdav_volume_id(host: &str, port: u16, username: &str) -> String {
    let host = host.to_lowercase();
    let port = port.to_string();
    derived_id(
        VolumeScheme::Webdav,
        &format!("{host}-{port}-{username}"),
        &[&host, &port, username],
    )
}

/// Build the ID for one S3 PLACE: a bucket under an account, or (with no bucket)
/// the account's root, which lists the buckets.
///
/// The account is the endpoint plus the access key id, and the bucket is the
/// place under it (`apps/desktop/src/lib/servers/DETAILS.md` § "The model"):
/// a pin, a tab, and a switcher row each key on a place, so two buckets under
/// one key, and the root beside them, get an id each. Two keys on one endpoint
/// are two accounts, since each may see different buckets with different rights.
///
/// # Case folding
///
/// The host is lowercased (DNS). The access key id and the bucket are NOT: both
/// are case-sensitive (a legacy `us-east-1` bucket may carry capitals).
///
/// ❗ The slug is the bucket and the host (bucket first, so the slug's cap cuts
/// the host), ❌ never the key id: an id lands in logs and data-dir names, and
/// the digest already carries the key.
pub fn s3_volume_id(host: &str, port: u16, access_key_id: &str, bucket: Option<&str>) -> String {
    let host = host.to_lowercase();
    let port = port.to_string();
    let bucket = bucket.unwrap_or_default();
    derived_id(
        VolumeScheme::S3,
        &format!("{bucket}-{host}"),
        &[&host, &port, access_key_id, bucket],
    )
}

/// Build the ID for an MTP device from its (opaque, verbatim) serial.
///
/// Called by [`super::mtp_ids::device_id_for`], which owns the serial-vs-topology
/// choice; this is only the encoding half. Routing the serial through the funnel
/// is what keeps it out of the ID itself, so a serial carrying a `/`, a `:`, or a
/// `.` can't break the `index-{id}.db` filename or the `{device}:{storage}`
/// split.
pub fn mtp_device_id(serial_or_location: &str) -> String {
    derived_id(VolumeScheme::Mtp, serial_or_location, &[serial_or_location])
}

/// Build the ID for an Android device reached over ADB from its (opaque,
/// verbatim) serial.
///
/// Its own scheme rather than a reuse of [`mtp_device_id`]: the same phone
/// attached over USB is both an MTP device and an ADB device at once, and they
/// are different storages (the curated media tree vs. the real filesystem) with
/// different indexes. Keying both on the serial would let one's cache answer
/// for the other. Routing the serial through the funnel keeps a `/`, `:`, or
/// `.` in it out of the `index-{id}.db` filename.
pub fn adb_volume_id(serial: &str) -> String {
    derived_id(VolumeScheme::Adb, serial, &[serial])
}

/// Which identity scheme a volume ID was minted under: the typed reading of its
/// `{scheme}-` prefix, and the ONE place anything reads that prefix.
///
/// It answers "what kind of drive does this ID name" from the ID alone, so it
/// holds for a drive that isn't connected, where no registered volume exists to
/// ask. That's what sets it apart from [`BackendKind`](super::BackendKind),
/// which says what serves a REGISTERED volume right now (an `smb-` share Cmdr
/// hasn't upgraded yet is served by a `Local` backend). Route by scheme for
/// "which transport owns this drive", read the backend for "can it be walked
/// now".
///
/// Shape-only: it does NOT prove the drive is attached. Decide per kind with an
/// exhaustive `match`, so a new scheme breaks the build everywhere a decision
/// has to be made for it. The frontend twin is `volumeScheme`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VolumeScheme {
    /// The boot volume, [`DEFAULT_VOLUME_ID`].
    Root,
    /// `vol-`: a local volume keyed by its filesystem UUID ([`local_volume_id`]).
    Local,
    /// `path-`: a mount keyed by its path ([`path_volume_id`]): a local volume
    /// with no UUID, or a non-SMB network mount.
    Path,
    /// `smb-`: an SMB share ([`smb_volume_id`]), whichever backend serves it.
    Smb,
    /// `sftp-`: an SFTP server ([`sftp_volume_id`]).
    Sftp,
    /// `webdav-`: a WebDAV server ([`webdav_volume_id`]).
    Webdav,
    /// `s3-`: a place on an S3 account ([`s3_volume_id`]).
    S3,
    /// `mtp-`: an MTP device, or one storage on it (`super::mtp_ids`).
    Mtp,
    /// `adb-`: an Android device over ADB ([`adb_volume_id`]).
    Adb,
    /// `cloud-`: a cloud-sync folder (iCloud Drive, Dropbox, …), a literal ID.
    Cloud,
    /// `fav-`: a favorite shown as a volume, a literal ID.
    Favorite,
    /// No current scheme: a pre-scheme ID, or a frontend-only virtual one.
    Unknown,
}

impl VolumeScheme {
    /// Every prefixed scheme with its tag. [`Self::of`] reads it and
    /// [`derived_id`] writes it, so minting and parsing can't drift apart.
    const TAGGED: [(Self, &'static str); 10] = [
        (Self::Local, "vol"),
        (Self::Path, "path"),
        (Self::Smb, "smb"),
        (Self::Sftp, "sftp"),
        (Self::Webdav, "webdav"),
        (Self::S3, "s3"),
        (Self::Mtp, "mtp"),
        (Self::Adb, "adb"),
        (Self::Cloud, "cloud"),
        (Self::Favorite, "fav"),
    ];

    /// The scheme `id` was minted under, or [`Self::Unknown`].
    pub fn of(id: &str) -> Self {
        if id == DEFAULT_VOLUME_ID {
            return Self::Root;
        }
        Self::TAGGED
            .iter()
            .find(|(_, tag)| {
                id.strip_prefix(tag)
                    .and_then(|rest| rest.strip_prefix('-'))
                    .is_some_and(|body| !body.is_empty())
            })
            .map_or(Self::Unknown, |(scheme, _)| *scheme)
    }

    /// Whether IDs of this scheme name an OS mount: a local volume or an SMB
    /// share. Eject reads it before trusting "no longer in the mount table",
    /// because a root that was never a mount (a `cloud-` drive's plain folder) is
    /// never listed, which says nothing about whether it's gone.
    pub fn is_mount_backed(self) -> bool {
        match self {
            Self::Local | Self::Path | Self::Smb => true,
            Self::Root
            | Self::Sftp
            | Self::Webdav
            | Self::S3
            | Self::Mtp
            | Self::Adb
            | Self::Cloud
            | Self::Favorite
            | Self::Unknown => false,
        }
    }

    /// The tag [`derived_id`] mints under. Only the derived schemes reach it.
    fn minted_tag(self) -> &'static str {
        debug_assert!(!matches!(self, Self::Root | Self::Unknown), "{self:?} is never derived");
        Self::TAGGED
            .iter()
            .find(|(scheme, _)| *scheme == self)
            .map_or("", |(_, tag)| tag)
    }
}

/// Whether `id` predates the current ID scheme, so the state it keys can never
/// be reached again.
///
/// True for anything that isn't [`DEFAULT_VOLUME_ID`], a `cloud-`/`fav-` literal
/// ID (neither is derived from a volume's identity), or a `{scheme}-…-{digest}`
/// ID from `derived_id` (private). Used by the index's startup sweep to delete the
/// databases stranded by the switch to identity-keyed IDs.
///
/// Both ways of being wrong are cheap: a missed legacy ID leaves one stale file,
/// and a live ID misread as legacy costs a rescan of a disposable cache. It is
/// NOT a security boundary; don't grow one on top of it.
pub fn is_legacy_volume_id(id: &str) -> bool {
    if matches!(
        VolumeScheme::of(id),
        VolumeScheme::Root | VolumeScheme::Cloud | VolumeScheme::Favorite
    ) {
        return false;
    }
    // An MTP volume ID is `{device_id}:{storage_id}`; the device half is the
    // part this scheme governs.
    let core = super::mtp_ids::split_volume_id(id).map_or(id, |(device_id, _)| device_id);
    !ends_with_digest(core)
}

/// Whether `id` ends in `-` plus [`DIGEST_HEX_LEN`] lowercase hex chars, the
/// suffix [`derived_id`] always appends.
fn ends_with_digest(id: &str) -> bool {
    let bytes = id.as_bytes();
    let Some(dash) = bytes.len().checked_sub(DIGEST_HEX_LEN + 1) else {
        return false;
    };
    bytes[dash] == b'-'
        && bytes[dash + 1..]
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
}

#[cfg(test)]
#[path = "ids_test.rs"]
mod id_tests;
