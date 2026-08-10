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
/// # Case folding
///
/// Server and share are lowercased before hashing, which is canonicalization
/// rather than loss: DNS hostnames are case-insensitive, and so are SMB share
/// names (Windows and Samba default), so `Naspolya`/`naspolya` and
/// `Public`/`public` really are the same mount. The port is literal.
pub fn smb_volume_id(server: &str, port: u16, share: &str) -> String {
    let server = server.to_lowercase();
    let share = share.to_lowercase();
    let port = port.to_string();
    derived_id("smb", &format!("{server}-{port}-{share}"), &[&server, &port, &share])
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

/// Whether `id` predates the current ID scheme, so the state it keys can never
/// be reached again.
///
/// True for anything that isn't [`DEFAULT_VOLUME_ID`], a `cloud-`/`fav-` literal
/// ID (neither is derived from a volume's identity), or a `{scheme}-…-{digest}`
/// ID from [`derived_id`]. Used by the index's startup sweep to delete the
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
mod id_tests {
    use super::super::mtp_ids::{device_id_for, mtp_volume_id};
    use super::*;

    // ── The property the whole module exists for: injectivity ─────────────

    #[test]
    fn distinct_mount_paths_never_share_an_id() {
        // Two different disks, two different IDs. An ID built by DELETING characters
        // is many-to-one, so `My Disk` and `My_Disk` would key the same index DB,
        // the same `lastUsedPaths` entry, and the same registry slot.
        assert_ne!(path_volume_id("/Volumes/My Disk"), path_volume_id("/Volumes/My_Disk"));
    }

    #[test]
    fn distinct_smb_shares_never_share_an_id() {
        assert_ne!(
            smb_volume_id("naspolya", 445, "My Share"),
            smb_volume_id("naspolya", 445, "MyShare")
        );
    }

    #[test]
    fn a_corpus_of_confusable_identities_maps_one_to_one() {
        // Every pair here collides under a strip-and-lowercase scheme. Held as a
        // corpus rather than N assert_ne!s so a new scheme has one place to prove
        // itself, and so the check is over the WHOLE set, not just neighbors.
        let paths = [
            "/Volumes/My Disk",
            "/Volumes/My_Disk",
            "/Volumes/MyDisk",
            "/Volumes/my-disk",
            "/Volumes/mydisk",
            "/Volumes/My.Disk",
            "/Volumes/Backup",
            "/Volumes/Backup 1",
            "/Volumes/Backup/1",
            "/Volumes/Ünïcödé",
            "/Volumes/Unicode",
            "/Volumes/…",
            "/Volumes/·",
            "/Volumes/Photos 2024",
            "/Volumes/Photos 2025",
        ];
        let mut seen = std::collections::HashMap::new();
        for path in paths {
            let id = path_volume_id(path);
            if let Some(other) = seen.insert(id.clone(), path) {
                panic!("{path} and {other} both got the ID {id}");
            }
        }

        let mounts = [
            ("naspolya", 445, "Public"),
            ("naspolya", 445, "Pub lic"),
            ("naspolya", 445, "Pu-blic"),
            ("naspolya", 10494, "Public"),
            ("nas-polya", 445, "Public"),
            ("naspolya2", 445, "Public"),
            ("192.168.1.111", 445, "naspi"),
            ("192.168.1.112", 445, "naspi"),
            ("19216811", 1, "naspi"),
        ];
        let mut seen = std::collections::HashMap::new();
        for (server, port, share) in mounts {
            let id = smb_volume_id(server, port, share);
            if let Some(other) = seen.insert(id.clone(), (server, port, share)) {
                panic!("{server}:{port}/{share} and {other:?} both got the ID {id}");
            }
        }
    }

    #[test]
    fn component_boundaries_cannot_be_shifted() {
        // Length-prefixed hashing: without it, ("nas", "polya…") and ("naspolya…")
        // would feed the hasher identical bytes.
        assert_ne!(
            smb_volume_id("nas", 445, "polyashare"),
            smb_volume_id("naspolya", 445, "share")
        );
    }

    #[test]
    fn schemes_are_domain_separated() {
        // The same canonical text under two schemes must not produce one ID.
        assert_ne!(path_volume_id("abc"), mtp_device_id("abc"));
    }

    // ── Stability: the same volume keeps its ID ───────────────────────────

    #[test]
    fn the_same_identity_always_produces_the_same_id() {
        // Required for `lastUsedPaths`, tabs, and the index DB to round-trip.
        assert_eq!(path_volume_id("/Volumes/naspi"), path_volume_id("/Volumes/naspi"));
        assert_eq!(
            smb_volume_id("naspolya", 445, "naspi"),
            smb_volume_id("naspolya", 445, "naspi")
        );
        assert_eq!(
            local_volume_id(Some("A1B2-C3D4"), "/Volumes/X"),
            local_volume_id(Some("A1B2-C3D4"), "/Volumes/X")
        );
    }

    #[test]
    fn a_uuid_backed_id_ignores_the_mount_point() {
        // The headline win over path-derived IDs: macOS mounts a second disk of the
        // same name at `/Volumes/Backup 1`, and the volume must keep its index.
        assert_eq!(
            local_volume_id(Some("A1B2-C3D4"), "/Volumes/Backup"),
            local_volume_id(Some("A1B2-C3D4"), "/Volumes/Backup 1"),
        );
    }

    #[test]
    fn a_uuid_backed_id_ignores_uuid_case() {
        assert_eq!(
            local_volume_id(Some("a1b2-c3d4"), "/Volumes/X"),
            local_volume_id(Some("A1B2-C3D4"), "/Volumes/X"),
        );
    }

    #[test]
    fn distinct_uuids_get_distinct_ids() {
        assert_ne!(
            local_volume_id(Some("A1B2-C3D4"), "/Volumes/X"),
            local_volume_id(Some("A1B2-C3D5"), "/Volumes/X"),
        );
    }

    #[test]
    fn a_volume_without_a_uuid_falls_back_to_its_path() {
        // tmpfs and most FUSE mounts report no UUID; they still need an ID.
        assert_eq!(local_volume_id(None, "/Volumes/X"), path_volume_id("/Volumes/X"));
        assert_eq!(local_volume_id(Some("   "), "/Volumes/X"), path_volume_id("/Volumes/X"));
    }

    #[test]
    fn the_boot_volume_keeps_its_literal_id() {
        // `root` is special-cased across the app (space polling, rollback lanes,
        // index retention), so it must survive every constructor.
        assert_eq!(path_volume_id("/"), DEFAULT_VOLUME_ID);
        assert_eq!(local_volume_id(None, "/"), DEFAULT_VOLUME_ID);
        assert_eq!(local_volume_id(Some("A1B2-C3D4"), "/"), DEFAULT_VOLUME_ID);
    }

    // ── Shape: these IDs become filenames ─────────────────────────────────

    #[test]
    fn every_id_is_filename_safe_and_bounded() {
        // IDs land in `index-{id}.db` beside `importance-` and `media-`. A path
        // separator, a `:`, or an unbounded length would break that.
        let ids = [
            path_volume_id("/Volumes/A Disk/With: Punctuation?/And/Slashes"),
            path_volume_id(&format!("/Volumes/{}", "x".repeat(500))),
            smb_volume_id("nas.local", 445, "Some Share/With Slash"),
            mtp_device_id("SERIAL/WITH:PUNCT.uation"),
            local_volume_id(Some("A1B2-C3D4"), "/Volumes/X"),
        ];
        for id in ids {
            assert!(
                id.chars().all(|c| c.is_alphanumeric() || c == '-'),
                "id must be alphanumerics and dashes only: {id}",
            );
            assert!(
                id.len() <= 64,
                "id must stay far under the 255-byte filename limit: {id}"
            );
            assert!(!id.is_empty());
        }
    }

    #[test]
    fn an_id_stays_readable_enough_to_recognize() {
        // The slug is why a data dir is eyeballable. Cosmetic, but it's the reason
        // we don't just use a bare digest, so it's worth a test.
        assert!(path_volume_id("/Volumes/Photos").contains("volumes-photos"));
        assert!(smb_volume_id("naspolya", 445, "naspi").contains("naspolya-445-naspi"));
    }

    #[test]
    fn an_unsluggable_identity_still_gets_an_id() {
        // A mount path of pure punctuation leaves no slug; the ID must not end up
        // as a bare `path-` with a dangling dash.
        let id = path_volume_id("/…/·");
        assert!(id.starts_with("path-"), "got: {id}");
        assert!(!id.ends_with('-'));
        assert_ne!(id, path_volume_id("/·/…"));
    }

    // ── Cross-scheme separation ───────────────────────────────────────────

    #[test]
    fn ids_from_different_schemes_never_collide() {
        // The scheme prefix is the contract every consumer's classification relies
        // on (`is_mtp_volume_id`, the index's root check, the legacy sweep).
        let smb = smb_volume_id("localhost", 10494, "public");
        let local = path_volume_id("/Volumes/Smb");
        let mtp = mtp_device_id("SERIAL");
        assert!(smb.starts_with("smb-"), "got: {smb}");
        assert!(local.starts_with("path-"), "got: {local}");
        assert!(mtp.starts_with("mtp-"), "got: {mtp}");
        assert_ne!(smb, local);
        assert_ne!(local, mtp);
        assert_ne!(smb, mtp);
    }

    #[test]
    fn smb_volume_id_distinguishes_servers_with_same_share_name() {
        // The exact bug that motivated per-mount IDs: QNAP's `Public` share and a
        // Docker container's `public` share would both collide on `volumespublic`
        // under a path-shape ID scheme, cross-contaminating `lastUsedPaths`, tabs,
        // and per-volume state.
        assert_ne!(
            smb_volume_id("Naspolya", 445, "Public"),
            smb_volume_id("localhost", 10494, "public")
        );
    }

    #[test]
    fn smb_volume_id_folds_case_where_the_protocol_does() {
        // DNS hostnames and SMB share names are both case-insensitive, so these are
        // the same mount and must share an ID.
        assert_eq!(
            smb_volume_id("Naspolya", 445, "naspi"),
            smb_volume_id("naspolya", 445, "naspi")
        );
        assert_eq!(
            smb_volume_id("naspolya", 445, "Public"),
            smb_volume_id("naspolya", 445, "public")
        );
    }

    #[test]
    fn smb_volume_id_distinguishes_ports_and_ip_addresses() {
        // Same host, same share, different port = a different server in practice
        // (reverse proxies, dev fixtures on localhost).
        assert_ne!(
            smb_volume_id("localhost", 10480, "public"),
            smb_volume_id("localhost", 10494, "public")
        );
        assert_ne!(
            smb_volume_id("192.168.1.111", 445, "naspi"),
            smb_volume_id("192.168.1.112", 445, "naspi")
        );
    }

    // ── Legacy detection (the one-shot sweep of stranded index DBs) ───────

    #[test]
    fn recognizes_ids_from_the_retired_scheme() {
        // What the pre-identity scheme produced: a stripped, lowercased path.
        assert!(is_legacy_volume_id("volumesmydisk"));
        assert!(is_legacy_volume_id("smb-naspolya-445-naspi"));
        assert!(is_legacy_volume_id("mtp-ABC123:65537"));
        assert!(is_legacy_volume_id("volumesexternal"));
    }

    #[test]
    fn accepts_every_id_the_current_scheme_can_mint() {
        for id in [
            DEFAULT_VOLUME_ID.to_string(),
            "cloud-icloud".to_string(),
            "fav-3".to_string(),
            path_volume_id("/Volumes/X"),
            path_volume_id("/…/·"),
            smb_volume_id("naspolya", 445, "naspi"),
            local_volume_id(Some("A1B2-C3D4"), "/Volumes/X"),
            mtp_device_id("SERIAL"),
            mtp_volume_id(&device_id_for(Some("SERIAL"), 0), 65537),
            mtp_volume_id(&device_id_for(Some("AA:BB:CC"), 0), 65537),
            mtp_volume_id(&device_id_for(None, 336_592_896), 65537),
        ] {
            assert!(!is_legacy_volume_id(&id), "current-scheme ID misread as legacy: {id}");
        }
    }

    #[test]
    fn a_digest_shaped_tail_is_not_enough_on_its_own() {
        // Guard the shape check itself: the digest is 16 lowercase hex chars after a
        // dash, and nothing shorter, longer, or uppercase counts.
        assert!(is_legacy_volume_id("path-x-ABCDEF0123456789"), "hex is lowercase");
        assert!(is_legacy_volume_id("path-x-abcdef012345678"), "15 chars is too short");
        assert!(is_legacy_volume_id("path-x-abcdefg123456789"), "g is not hex");
        assert!(!is_legacy_volume_id("path-x-abcdef0123456789"));
        // An empty slug is legitimate (`{scheme}-{digest}`), so this one is current.
        assert!(!is_legacy_volume_id("path-abcdef0123456789"));
    }
}
