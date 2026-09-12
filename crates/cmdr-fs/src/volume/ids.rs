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
//! - `mtp-`: an MTP device keyed by its serial (`super::mtp_ids`).
//! - `path-`: the fallback when nothing better exists ([`path_volume_id`]),
//!   keyed by the mount path. Stable only as long as the mount path is.
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
fn derived_id(scheme: &str, slug_source: &str, canonical_parts: &[&str]) -> String {
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
            derived_id("vol", &folded, &[&folded])
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
    derived_id("path", mount_path, &[mount_path])
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
    use unicode_normalization::UnicodeNormalization;

    let server: String = server.nfc().flat_map(char::to_lowercase).collect();
    let share: String = share.nfc().flat_map(char::to_lowercase).collect();
    let port = port.to_string();
    derived_id("smb", &format!("{server}-{port}-{share}"), &[&server, &port, &share])
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
    derived_id("sftp", &format!("{host}-{port}-{username}"), &[&host, &port, username])
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
        "webdav",
        &format!("{host}-{port}-{username}"),
        &[&host, &port, username],
    )
}

/// The `sftp://<user>@<host>:<port>` prefix every app path on an SFTP volume
/// carries.
///
/// Minted here, beside [`sftp_volume_id`], so the crate and the app spell it from
/// one function and a saved server's row, a restored tab, and the live volume all
/// agree. The volume's app root is this plus the remote root
/// (`sftp://ada@nas.local:22/srv/data`).
///
/// ❗ **The prefix is what makes a remote path self-describing.** The mount table
/// answers the LOCAL root for any absolute path it doesn't recognize, on both
/// platforms, so a scheme-free `/srv/data/x` resolves to the boot disk at every
/// resolver site. `cmdr_fs::volume::remote_paths` is the translation, and
/// `commands/volumes.rs::resolve_path_to_volume` is the arm that reads it.
///
/// # Case folding
///
/// The host is lowercased and the username is not, exactly as in
/// [`sftp_volume_id`], so a path and the id it resolves to agree on identity.
pub fn sftp_app_root(host: &str, port: u16, username: &str) -> String {
    remote_app_root(SFTP_SCHEME, host, port, username)
}

/// The `webdav://<user>@<host>:<port>` prefix every app path on a WebDAV volume
/// carries. The SFTP twin, for the same reasons: [`sftp_app_root`].
///
/// ❗ `webdav://` whatever the transport is. `http` and `https` to one host and
/// port are the same server (the port tells the two default listeners apart),
/// which is the same call [`webdav_volume_id`] makes, so the two can't disagree.
pub fn webdav_app_root(host: &str, port: u16, username: &str) -> String {
    remote_app_root(WEBDAV_SCHEME, host, port, username)
}

/// The scheme [`sftp_app_root`] mints and [`server_of_path`] reads back.
const SFTP_SCHEME: &str = "sftp";

/// The scheme [`webdav_app_root`] mints and [`server_of_path`] reads back.
const WEBDAV_SCHEME: &str = "webdav";

/// `{scheme}://{username}@{host}:{port}`, with the host folded the way the volume
/// id folds it.
fn remote_app_root(scheme: &str, host: &str, port: u16, username: &str) -> String {
    let host = host.to_lowercase();
    format!("{scheme}://{username}@{host}:{port}")
}

/// The server account an app path names, as [`server_of_path`] reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerPath {
    /// The backend its scheme names: [`BackendKind::Sftp`](super::BackendKind::Sftp)
    /// or [`BackendKind::Webdav`](super::BackendKind::Webdav).
    pub kind: super::BackendKind,
    /// The id that account mints ([`sftp_volume_id`] or [`webdav_volume_id`]).
    pub volume_id: String,
}

/// The server account an `sftp://` or `webdav://<user>@<host>:<port>[/…]` app
/// path names, or `None` for any other path, and for one missing its user, host,
/// or port.
///
/// The one split of what [`remote_app_root`] joins, kept beside it so the two
/// can't drift. Pure for the reason [`adb_serial_of_path`] is: the path IS the
/// identity its id is minted from, so a saved server nobody has connected answers
/// the same as a live one, and the kind rides along so a caller can ask
/// [`BackendKind::can_be_indexed`](super::BackendKind::can_be_indexed) without a
/// registry.
///
/// The user is everything before the authority's LAST `@` and the port everything
/// after its last `:`, which is how an IPv6 host (`sftp://ada@::1:22`) still
/// splits.
pub fn server_of_path(path: &str) -> Option<ServerPath> {
    type Mint = fn(&str, u16, &str) -> String;
    let servers: [(&str, super::BackendKind, Mint); 2] = [
        (SFTP_SCHEME, super::BackendKind::Sftp, sftp_volume_id),
        (WEBDAV_SCHEME, super::BackendKind::Webdav, webdav_volume_id),
    ];
    let (rest, kind, mint) = servers.into_iter().find_map(|(scheme, kind, mint)| {
        let rest = path.strip_prefix(scheme)?.strip_prefix("://")?;
        Some((rest, kind, mint))
    })?;
    let authority = rest.split('/').next()?;
    let (username, host_port) = authority.rsplit_once('@')?;
    let (host, port) = host_port.rsplit_once(':')?;
    let port: u16 = port.parse().ok()?;
    if username.is_empty() || host.is_empty() {
        return None;
    }
    Some(ServerPath {
        kind,
        volume_id: mint(host, port, username),
    })
}

/// Build the ID for an MTP device from its (opaque, verbatim) serial.
///
/// Called by [`super::mtp_ids::device_id_for`], which owns the serial-vs-topology
/// choice; this is only the encoding half. Routing the serial through the funnel
/// is what keeps it out of the ID itself, so a serial carrying a `/`, a `:`, or a
/// `.` can't break the `index-{id}.db` filename or the `{device}:{storage}`
/// split.
pub fn mtp_device_id(serial_or_location: &str) -> String {
    derived_id("mtp", serial_or_location, &[serial_or_location])
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
    derived_id("adb", serial, &[serial])
}

/// The `adb://<serial>` prefix every app path on an ADB volume carries, and the
/// volume's root.
///
/// Minted here, beside [`adb_volume_id`], for the reason [`sftp_app_root`] is:
/// the crate, the device provider's row, and a restored tab all spell it from
/// one function. The device's own tree hangs under it
/// (`adb://R58M1/sdcard/DCIM`), and `cmdr_fs::volume::remote_paths` is the
/// translation.
///
/// ❗ The serial goes in VERBATIM, case and all. The id's slug is folded for
/// readability, but the id's digest and the ADB server both key on the exact
/// serial, so a folded prefix would name a device the server doesn't list.
pub fn adb_app_root(serial: &str) -> String {
    format!("{ADB_PATH_SCHEME}{serial}")
}

/// The scheme every [`adb_app_root`] starts with.
const ADB_PATH_SCHEME: &str = "adb://";

/// The serial an `adb://<serial>[/…]` app path names, verbatim, or `None` for
/// any other path.
///
/// The one split of what [`adb_app_root`] joins, kept beside it so the two can't
/// drift: the device provider asks it which phone a path is on, and the index
/// asks it which phone's index a path belongs to. It reads the serial only; the
/// device path under it is the volume's own translation (`remote_paths`).
pub fn adb_serial_of_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix(ADB_PATH_SCHEME)?;
    let serial = rest.split('/').next()?;
    (!serial.is_empty()).then_some(serial)
}

/// Whether `id` names an Android device reached over ADB.
///
/// The Rust twin of `isAdbVolumeId` in `adb-path-utils.ts`, and the same test:
/// the scheme prefix [`adb_volume_id`] mints. Shape-only, like
/// [`is_mtp_device_id`](super::mtp_ids::is_mtp_device_id): it does NOT prove the
/// phone is attached. Used where a phone must read differently from a disk,
/// which is anywhere the word "Eject" would otherwise appear.
pub fn is_adb_volume_id(id: &str) -> bool {
    id.starts_with("adb-")
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
    if id == DEFAULT_VOLUME_ID || id.starts_with("cloud-") || id.starts_with("fav-") {
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
