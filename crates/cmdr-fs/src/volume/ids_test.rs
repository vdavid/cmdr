use super::super::mtp_ids::{device_id_for, mtp_volume_id};
use super::*;

// ── Scheme shape: which IDs name an OS mount ──────────────────────────

#[test]
fn only_the_ids_minted_for_a_mount_are_mount_backed() {
    // Eject trusts "no longer in the mount table" only for these: a root that was
    // never a mount (a cloud drive's plain folder) is never listed, which says
    // nothing about whether it's still there.
    assert!(is_mount_backed_volume_id(&local_volume_id(
        Some("5C1A2D4E-0000-4000-8000-00000000BEEF"),
        "/Volumes/Backup"
    )));
    assert!(is_mount_backed_volume_id(&path_volume_id("/Volumes/NO NAME")));
    assert!(is_mount_backed_volume_id(&smb_volume_id("naspolya", 445, "public")));

    assert!(!is_mount_backed_volume_id(DEFAULT_VOLUME_ID));
    assert!(!is_mount_backed_volume_id("cloud-dropbox"));
    assert!(!is_mount_backed_volume_id("fav-1"));
    assert!(!is_mount_backed_volume_id(&sftp_volume_id("naspolya", 22, "ada")));
    assert!(!is_mount_backed_volume_id(&webdav_volume_id("naspolya", 443, "ada")));
    assert!(!is_mount_backed_volume_id(&adb_volume_id("R58M12345")));
}

// ── The property the whole module exists for: injectivity ─────────────

#[test]
fn distinct_mount_paths_never_share_an_id() {
    // Two different disks, two different IDs. An ID built by DELETING characters
    // is many-to-one, so `My Disk` and `My_Disk` would key the same index DB,
    // the same `lastUsedPaths` entry, and the same registry slot.
    assert_ne!(path_volume_id("/Volumes/My Disk"), path_volume_id("/Volumes/My_Disk"));
}

#[test]
fn two_accounts_on_one_sftp_server_never_share_an_id() {
    // The whole reason the username is in the tuple: these two see different
    // files under the same paths, so one ID would hand one account's index,
    // saved paths, and tab state to the other.
    assert_ne!(
        sftp_volume_id("naspolya", 22, "ada"),
        sftp_volume_id("naspolya", 22, "grace")
    );
}

#[test]
fn an_sftp_volume_keeps_its_id_across_remote_roots() {
    // The root is addressing, not identity: browsing in from `/srv` rather
    // than from `/` must not strand the index and the saved paths.
    assert_eq!(
        sftp_volume_id("naspolya", 22, "ada"),
        sftp_volume_id("naspolya", 22, "ada")
    );
}

#[test]
fn sftp_volume_id_folds_the_host_but_not_the_account() {
    // DNS is case-insensitive; POSIX accounts are not, so `Ada` and `ada`
    // may be two people and must not collapse.
    assert_eq!(
        sftp_volume_id("Naspolya", 22, "ada"),
        sftp_volume_id("naspolya", 22, "ada")
    );
    assert_ne!(
        sftp_volume_id("naspolya", 22, "Ada"),
        sftp_volume_id("naspolya", 22, "ada")
    );
}

#[test]
fn sftp_volume_id_distinguishes_ports() {
    // Same host, different port is a different server in practice: a jump
    // box, a container, a dev fixture on localhost.
    assert_ne!(
        sftp_volume_id("localhost", 12480, "ada"),
        sftp_volume_id("localhost", 12481, "ada")
    );
}

#[test]
fn two_accounts_on_one_webdav_server_never_share_an_id() {
    // Same rule as SFTP: two accounts see different files under the same
    // paths, so one ID would hand one account's index and tab state to the
    // other.
    assert_ne!(
        webdav_volume_id("dav.example.test", 443, "ada"),
        webdav_volume_id("dav.example.test", 443, "grace")
    );
}

#[test]
fn a_webdav_volume_keeps_its_id_across_remote_roots() {
    // The root is addressing, not identity: the id is derived from the
    // triple alone, so it is the same however deep the user browsed in.
    assert_eq!(
        webdav_volume_id("dav.example.test", 443, "ada"),
        webdav_volume_id("dav.example.test", 443, "ada")
    );
}

#[test]
fn webdav_volume_id_folds_the_host_but_not_the_account() {
    assert_eq!(
        webdav_volume_id("DAV.example.test", 443, "ada"),
        webdav_volume_id("dav.example.test", 443, "ada")
    );
    assert_ne!(
        webdav_volume_id("dav.example.test", 443, "Ada"),
        webdav_volume_id("dav.example.test", 443, "ada")
    );
}

#[test]
fn webdav_volume_id_distinguishes_ports() {
    // Two Docker fixtures on localhost are two servers.
    assert_ne!(
        webdav_volume_id("localhost", 18080, "ada"),
        webdav_volume_id("localhost", 18081, "ada")
    );
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
        sftp_volume_id("nas.local", 22, "ada/with:punct"),
        webdav_volume_id("nas.local", 443, "ada/with:punct"),
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
fn a_phone_is_recognizable_from_its_id_alone() {
    // The native menus have only the volume id to go on when they decide
    // whether the row's detach control says Eject or Disconnect.
    assert!(is_adb_volume_id(&adb_volume_id("39041FDJH00A0K")));
    assert!(!is_adb_volume_id(&mtp_device_id("39041FDJH00A0K")));
    assert!(!is_adb_volume_id(&path_volume_id("/Volumes/Backup")));
    assert!(!is_adb_volume_id(DEFAULT_VOLUME_ID));
}

#[test]
fn an_adb_prefix_keeps_the_serial_exactly_as_the_server_names_it() {
    // The ADB server keys on the exact serial, so a folded prefix would dial a
    // device nobody listed. The id folds its slug; the prefix never does.
    assert_eq!(adb_app_root("46061FDAS000A4"), "adb://46061FDAS000A4");
    assert_eq!(adb_app_root("192.168.1.5:5555"), "adb://192.168.1.5:5555");
    assert_ne!(adb_app_root("R58M1"), adb_app_root("r58m1"));
}

#[test]
fn a_phone_path_names_its_serial_exactly_as_the_prefix_spelled_it() {
    // The index routes a pane's `adb://…` path to its phone by this serial, so
    // it must read back exactly what `adb_app_root` wrote, port and case included.
    assert_eq!(adb_serial_of_path(&adb_app_root("ZY22ABC")), Some("ZY22ABC"));
    assert_eq!(adb_serial_of_path("adb://ZY22ABC/sdcard/DCIM"), Some("ZY22ABC"));
    assert_eq!(
        adb_serial_of_path("adb://192.168.1.5:5555/sdcard"),
        Some("192.168.1.5:5555")
    );
    assert_eq!(adb_serial_of_path("adb://"), None);
    assert_eq!(adb_serial_of_path("adb:///sdcard"), None);
    assert_eq!(adb_serial_of_path("mtp://dev/1"), None);
    // A bare device path is the Mac's boot disk in the app's vocabulary.
    assert_eq!(adb_serial_of_path("/sdcard/DCIM"), None);
}

#[test]
fn ids_from_different_schemes_never_collide() {
    // The scheme prefix is the contract every consumer's classification relies
    // on (`is_mtp_volume_id`, the index's root check, the legacy sweep).
    let smb = smb_volume_id("localhost", 10494, "public");
    let sftp = sftp_volume_id("localhost", 12480, "ada");
    let webdav = webdav_volume_id("localhost", 12480, "ada");
    let local = path_volume_id("/Volumes/Smb");
    let mtp = mtp_device_id("SERIAL");
    assert!(sftp.starts_with("sftp-"), "got: {sftp}");
    // Same triple as the SFTP one above, and still a different volume: the
    // scheme prefix is what keeps two backends on one host apart.
    assert!(webdav.starts_with("webdav-"), "got: {webdav}");
    assert_ne!(webdav, sftp);
    assert_ne!(webdav, smb);
    assert_ne!(webdav, local);
    assert_ne!(webdav, mtp);
    assert_ne!(sftp, smb);
    assert_ne!(sftp, local);
    assert_ne!(sftp, mtp);
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
fn smb_volume_id_folds_unicode_normalization() {
    // macOS hands out NFD (decomposed) share names from `statfs` while mDNS and
    // the server's own share list hand out NFC (composed) ones, so one visible
    // share arrives spelled two ways. Two IDs for one share splits its index,
    // `lastUsedPaths`, and tab `volumeId`s down whichever path registered it.
    // Reported as ERR-ABXW4 on the share `Régi NAS`.
    let composed = "R\u{e9}gi NAS";
    let decomposed = "Re\u{301}gi NAS";
    assert_ne!(
        composed, decomposed,
        "the two spellings must differ as bytes, or this proves nothing"
    );
    assert_eq!(
        smb_volume_id("naspolya", 445, composed),
        smb_volume_id("naspolya", 445, decomposed)
    );
    // The server half arrives from the same two pipes, so it folds too.
    assert_eq!(
        smb_volume_id("caf\u{e9}-nas", 445, "naspi"),
        smb_volume_id("cafe\u{301}-nas", 445, "naspi")
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
        sftp_volume_id("naspolya", 22, "ada"),
        webdav_volume_id("naspolya", 443, "ada"),
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
