//! The two spellings of a remote volume's tree, and the refusals that keep the
//! app from addressing a real, wrong file on someone's server.

use super::*;

const PREFIX: &str = "sftp://test_user_1@testnas.local:22";

fn rooted_at(remote_root: &str) -> RemoteRoot {
    RemoteRoot::new(PREFIX.to_string(), Path::new(remote_root))
}

/// ❗ One spelling of a server-side directory, shared with `RemoteRoot::new`, so
/// a path compared against a root is normalized the way the root itself was.
#[test]
fn a_remote_path_normalizes_to_one_absolute_spelling() {
    assert_eq!(normalize_remote_path(Path::new("/srv/data/")), Path::new("/srv/data"));
    assert_eq!(normalize_remote_path(Path::new("srv/data")), Path::new("/srv/data"));
    assert_eq!(
        normalize_remote_path(Path::new("/srv/./data/x/..")),
        Path::new("/srv/data")
    );
    assert_eq!(normalize_remote_path(Path::new("")), Path::new("/"));
    assert_eq!(normalize_remote_path(Path::new(".")), Path::new("/"));
    assert_eq!(normalize_remote_path(Path::new("/..")), Path::new("/"));
}

#[test]
fn the_app_root_is_the_prefix_plus_the_remote_root() {
    let root = rooted_at("/srv/data");
    assert_eq!(
        root.app_root(),
        Path::new("sftp://test_user_1@testnas.local:22/srv/data")
    );
    assert_eq!(root.remote_root(), Path::new("/srv/data"));
}

/// A volume rooted at the server's own root still carries the prefix, and its
/// app root ends in the slash that makes it a path rather than an authority.
#[test]
fn a_volume_at_the_server_root_still_carries_the_prefix() {
    let root = rooted_at("/");
    assert_eq!(root.app_root(), Path::new("sftp://test_user_1@testnas.local:22/"));
    assert_eq!(root.remote_root(), Path::new("/"));
}

#[test]
fn the_three_spellings_of_the_root_all_mean_the_root() {
    let root = rooted_at("/srv/data");
    assert_eq!(root.to_remote_path(Path::new("")).as_deref(), Some("/srv/data"));
    assert_eq!(root.to_remote_path(Path::new(".")).as_deref(), Some("/srv/data"));
    assert_eq!(root.to_remote_path(Path::new("/")).as_deref(), Some("/srv/data"));
}

/// The two spellings the app actually uses land on one server path, and the
/// prefixed one is what comes back out.
#[test]
fn a_prefixed_and_a_relative_path_land_on_the_same_server_path() {
    let root = rooted_at("/srv/data");
    let from_prefix = root
        .to_remote_path(Path::new("sftp://test_user_1@testnas.local:22/srv/data/photos"))
        .expect("a path under this volume's own prefix is on this volume");
    let from_relative = root
        .to_remote_path(Path::new("photos"))
        .expect("a relative path hangs off the root");
    assert_eq!(from_prefix, "/srv/data/photos");
    assert_eq!(from_relative, from_prefix);
    assert_eq!(
        root.to_app_path(&from_prefix).as_deref(),
        Some(Path::new("sftp://test_user_1@testnas.local:22/srv/data/photos")),
        "`to_app_path` is the exact inverse, so a round trip is the identity"
    );
}

/// ❗ **A server answer outside the root has no app path.** A misbehaving server
/// (a WebDAV `href` above the collection, a `..` for a name) would otherwise
/// mint a path off this volume, which the way back then refuses. Containment is
/// checked both ways, the same way: by whole components, after `..`.
#[test]
fn a_server_path_outside_the_root_has_no_app_path() {
    let root = rooted_at("/srv/data");
    assert_eq!(
        root.to_app_path("/srv/data/photos/trip.jpg").as_deref(),
        Some(Path::new(
            "sftp://test_user_1@testnas.local:22/srv/data/photos/trip.jpg"
        ))
    );
    assert_eq!(root.to_app_path("/srv"), None, "an href above the collection");
    assert_eq!(root.to_app_path("/etc/passwd"), None, "somewhere else entirely");
    assert_eq!(
        root.to_app_path("/srv/data/.."),
        None,
        "`..` is resolved before the check"
    );
    assert_eq!(
        root.to_app_path("/srv/data-1/photos"),
        None,
        "a sibling sharing the root's string prefix"
    );
}

/// ❗ **A bare server-absolute path is refused, not anchored.**
///
/// Five app sites run a path through `cmdr_fs::volume::root_anchored` before the
/// volume sees it, and that helper JOINS anything not under the root onto it:
/// `/srv/data/x` would become `sftp://…/srv/data/srv/data/x`, strip back to a
/// real server path, and never refuse. With the prefix in place the app never
/// spells a remote path bare, so leniency here buys nothing but that hole.
#[test]
fn a_bare_server_absolute_path_is_refused() {
    let root = rooted_at("/srv/data");
    assert_eq!(root.to_remote_path(Path::new("/srv/data/photos")), None);
    assert_eq!(root.to_remote_path(Path::new("/etc/passwd")), None);
}

#[test]
fn a_prefixed_path_outside_the_root_is_refused() {
    let root = rooted_at("/srv/data");
    assert_eq!(
        root.to_remote_path(Path::new("sftp://test_user_1@testnas.local:22/etc/passwd")),
        None
    );
    assert_eq!(
        root.to_remote_path(Path::new("sftp://test_user_1@testnas.local:22/srv")),
        None
    );
}

/// Another server's path, or another account's on the same server, is not this
/// volume's however similar it looks.
#[test]
fn another_servers_prefix_is_not_this_volume() {
    let root = rooted_at("/srv/data");
    assert_eq!(
        root.to_remote_path(Path::new("sftp://test_user_1@other.local:22/srv/data/photos")),
        None
    );
    assert_eq!(
        root.to_remote_path(Path::new("sftp://test_user_2@testnas.local:22/srv/data/photos")),
        None
    );
    assert_eq!(
        root.to_remote_path(Path::new("webdav://test_user_1@testnas.local:22/srv/data/photos")),
        None
    );
}

/// The trap a raw string prefix compare falls into: a sibling whose name merely
/// starts with the root's. Stripping `/srv/data` off `/srv/data-1/photos` asks
/// the server for `-1/photos`, which is a legal name.
#[test]
fn the_root_is_matched_by_whole_components() {
    let root = rooted_at("/srv/data");
    assert_eq!(
        root.to_remote_path(Path::new("sftp://test_user_1@testnas.local:22/srv/data-1/photos")),
        None
    );
    assert_eq!(
        root.to_remote_path(Path::new("sftp://test_user_1@testnas.local:22/srv/dataX")),
        None
    );
}

/// The prefix itself is matched by whole components too, so a server whose name
/// merely starts with ours can't borrow our volume.
#[test]
fn the_prefix_is_matched_by_whole_components() {
    let root = rooted_at("/srv/data");
    assert_eq!(
        root.to_remote_path(Path::new("sftp://test_user_1@testnas.local:2222/srv/data/photos")),
        None
    );
}

/// `..` is resolved lexically BEFORE the containment check, because the server
/// would resolve `photos/../../etc` happily and the question is what the caller
/// addressed.
#[test]
fn a_relative_path_cannot_climb_out_of_the_root() {
    let root = rooted_at("/srv/data");
    assert_eq!(root.to_remote_path(Path::new("../secrets")), None);
    assert_eq!(root.to_remote_path(Path::new("photos/../../secrets")), None);
    assert_eq!(
        root.to_remote_path(Path::new("sftp://test_user_1@testnas.local:22/srv/data/../secrets")),
        None
    );
}

/// A volume at the server root reaches everything under it, still through the
/// prefix.
#[test]
fn a_volume_at_the_server_root_reaches_everything_through_the_prefix() {
    let root = rooted_at("/");
    assert_eq!(
        root.to_remote_path(Path::new("sftp://test_user_1@testnas.local:22/etc/passwd"))
            .as_deref(),
        Some("/etc/passwd")
    );
    assert_eq!(
        root.to_remote_path(Path::new("etc/passwd")).as_deref(),
        Some("/etc/passwd")
    );
    assert_eq!(root.to_remote_path(Path::new("/")).as_deref(), Some("/"));
    assert_eq!(
        root.to_app_path("/").as_deref(),
        Some(Path::new("sftp://test_user_1@testnas.local:22/")),
        "the root round-trips to the app root"
    );
}

/// `root_anchored` is what five app sites run a path through before the volume
/// sees it, and this is the shape that has to survive it: a path the pane
/// already holds passes through untouched, and a relative one lands under the
/// root.
#[test]
fn a_path_anchored_by_the_app_still_lands_where_the_pane_says() {
    let root = rooted_at("/srv/data");
    let anchored = crate::volume::root_anchored(root.app_root(), Path::new("photos/trip.jpg"));
    assert_eq!(
        anchored,
        Path::new("sftp://test_user_1@testnas.local:22/srv/data/photos/trip.jpg")
    );
    assert_eq!(
        root.to_remote_path(&anchored).as_deref(),
        Some("/srv/data/photos/trip.jpg")
    );

    let already_full = Path::new("sftp://test_user_1@testnas.local:22/srv/data/photos/trip.jpg");
    assert_eq!(
        crate::volume::root_anchored(root.app_root(), already_full),
        already_full,
        "a path the pane already holds is anchored to itself, never doubled"
    );
}

// ── The app-path schemes ─────────────────────────────────────────────

#[test]
fn an_adb_prefix_keeps_the_serial_exactly_as_the_server_names_it() {
    // The ADB server keys on the exact serial, so a folded prefix would dial a
    // device nobody listed. The id folds its slug; the prefix never does.
    assert_eq!(adb_app_root("TESTSERIAL0001"), "adb://TESTSERIAL0001");
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
fn an_s3_app_root_names_the_account_and_folds_the_host() {
    assert_eq!(
        s3_app_root("S3.eu-west-1.amazonaws.com", 443, "AKIAEXAMPLE"),
        "s3://AKIAEXAMPLE@s3.eu-west-1.amazonaws.com:443"
    );
}

#[test]
fn an_s3_path_names_the_place_of_its_bucket() {
    // ❗ The bucket is the place, so a path inside one names that bucket's id;
    // a path at the account's root names the account place.
    let in_bucket = server_of_path("s3://AKIAEXAMPLE@127.0.0.1:14480/cmdr-test/a/b.txt").expect("an S3 path");
    assert_eq!(in_bucket.kind, super::super::BackendKind::S3);
    assert_eq!(
        in_bucket.volume_id,
        super::super::s3_volume_id("127.0.0.1", 14480, "AKIAEXAMPLE", Some("cmdr-test"))
    );
    for root in ["s3://AKIAEXAMPLE@127.0.0.1:14480", "s3://AKIAEXAMPLE@127.0.0.1:14480/"] {
        let account = server_of_path(root).expect("an S3 account path");
        assert_eq!(
            account.volume_id,
            super::super::s3_volume_id("127.0.0.1", 14480, "AKIAEXAMPLE", None),
            "{root}"
        );
    }
}

#[test]
fn an_nfs_app_root_carries_the_export_as_one_segment() {
    // ❗ No account in the authority (the uid is a setting, not identity), and the
    // export rides as ONE segment, so `server_of_path` can tell where it ends.
    assert_eq!(
        nfs_app_root("TESTNAS.local", 2049, "/Public"),
        "nfs://testnas.local:2049/Public"
    );
    assert_eq!(
        nfs_app_root("testnas.local", 2049, "/volume1/data/"),
        "nfs://testnas.local:2049/volume1%2Fdata"
    );
    assert_eq!(nfs_app_root("testnas.local", 2049, "/"), "nfs://testnas.local:2049/%2F");
    assert_eq!(
        nfs_app_root("testnas.local", 2049, "/100%/x"),
        "nfs://testnas.local:2049/100%25%2Fx"
    );
}

#[test]
fn an_nfs_path_names_the_place_of_its_export() {
    for (export, path) in [
        ("/Public", "nfs://testnas.local:2049/Public/photos/a.jpg"),
        ("/Public", "nfs://testnas.local:2049/Public"),
        ("/volume1/data", "nfs://testnas.local:2049/volume1%2Fdata/photos"),
        ("/", "nfs://testnas.local:2049/%2F/etc"),
        ("/100%/x", "nfs://testnas.local:2049/100%25%2Fx"),
    ] {
        let server = server_of_path(path).expect("an NFS path");
        assert_eq!(server.kind, super::super::BackendKind::Nfs, "{path}");
        assert_eq!(server.volume_id, nfs_volume_id("testnas.local", 2049, export), "{path}");
    }
}

#[test]
fn an_nfs_path_without_an_export_or_a_port_names_nothing() {
    for path in [
        "nfs://testnas.local:2049",
        "nfs://testnas.local:2049/",
        "nfs://testnas.local/Public",
        "nfs://:2049/Public",
        // A broken escape is no spelling `nfs_app_root` mints.
        "nfs://testnas.local:2049/a%2",
        "nfs://testnas.local:2049/a%zz",
    ] {
        assert_eq!(server_of_path(path), None, "{path}");
    }
}

#[test]
fn an_nfs_app_root_and_its_volume_id_agree_through_server_of_path() {
    // Invariant 11: the path a volume mints and the id it registers under come
    // from one tuple, so routing a path finds the volume that minted it.
    for export in ["/Public", "/volume1/data", "/", "/Caf\u{e9} & co"] {
        let root = nfs_app_root("Testnas.Local", 15481, export);
        let server = server_of_path(&format!("{root}/sub/file")).expect("an NFS path");
        assert_eq!(
            server.volume_id,
            nfs_volume_id("testnas.local", 15481, export),
            "{export}"
        );
    }
}

#[test]
fn a_shown_s3_path_names_the_host_and_bucket_but_never_the_key_id() {
    // #706: the key id identifies the account, and a person reads (or a log keeps) the place
    // by its host and bucket.
    let root = s3_app_root("s3.eu-west-1.amazonaws.com", 443, "AKIAEXAMPLEKEY");
    assert_eq!(
        without_s3_key_ids(&format!("{root}/photos/2025/a.jpg")),
        "s3://s3.eu-west-1.amazonaws.com:443/photos/2025/a.jpg"
    );
    assert_eq!(without_s3_key_ids(&root), "s3://s3.eu-west-1.amazonaws.com:443");
}

#[test]
fn every_s3_path_in_a_line_loses_its_key_id_and_nothing_else_changes() {
    // Marked log spans, quotes, and another scheme's account all stay as they are.
    let line = "copy \u{FDD2}s3://GK00c0de@127.0.0.1:14480/b/x\u{FDEF} -> \"s3://AKIA2@h:443/c/\" (sftp://test_user_1@nas:22/x)";
    assert_eq!(
        without_s3_key_ids(line),
        "copy \u{FDD2}s3://127.0.0.1:14480/b/x\u{FDEF} -> \"s3://h:443/c/\" (sftp://test_user_1@nas:22/x)"
    );
}

#[test]
fn text_without_an_s3_account_comes_back_untouched() {
    for text in [
        "/Users/test_user_1/file.txt",
        // The AWS CLI's spelling carries no account.
        "s3://bucket/key@2x.png",
        "mail test_user_1@example.com about s3://bucket",
        "xs3://key@host:1/b",
    ] {
        assert!(
            matches!(without_s3_key_ids(text), Cow::Borrowed(t) if t == text),
            "{text}"
        );
    }
}

/// Both halves of an app's refusal copy (a pane and a transfer) ask this, so they
/// can't disagree about who said no. Purely the scheme: a local path, even one that
/// spells a scheme inside it, has no refuser.
#[test]
fn a_refusal_names_the_backend_the_path_is_on() {
    for (path, refuser) in [
        ("adb://R58M123/data", RemoteRefuser::Phone),
        ("adb://R58M123", RemoteRefuser::Phone),
        ("mtp://pixel/65537/DCIM", RemoteRefuser::Phone),
        ("sftp://test_user_1@testnas.local:22/etc", RemoteRefuser::Server),
        ("webdav://test_user_1@testnas.local:443/a", RemoteRefuser::Server),
        ("nfs://testnas.local:2049/%2Fexport/a", RemoteRefuser::Server),
        ("s3://AKIATEST@s3.example.com:443/photos/a", RemoteRefuser::ObjectStore),
    ] {
        assert_eq!(refuser_of_path(path), Some(refuser), "{path}");
    }
    for path in [
        "/Users/me/a",
        "/Volumes/testshare/adb://x",
        "smb://nas/share",
        "adb://",
        "mtp://",
    ] {
        assert_eq!(refuser_of_path(path), None, "{path}");
    }
}
