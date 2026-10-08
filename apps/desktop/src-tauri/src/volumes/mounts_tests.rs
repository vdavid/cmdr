use super::*;

// ========================================================================
// Hung-mount guard: getfsstat-based discovery (Bug: dead mount froze launch)
// ========================================================================

/// A browsable mount, which is what every drive a user sees is. The
/// unbrowsable case has its own helper, since it's the interesting one.
fn mount(mount_point: &str, fs_type: &str, mount_from: &str, is_read_only: bool) -> MountEntry {
    MountEntry {
        mount_point: mount_point.to_string(),
        fs_type: fs_type.to_string(),
        mount_from: mount_from.to_string(),
        is_read_only,
        is_browsable: true,
        fsid: 0,
        mounted_by: MountedBy::ThisUser,
    }
}

/// The same row, as `who` mounted it.
fn mounted_by(who: MountedBy, entry: MountEntry) -> MountEntry {
    MountEntry {
        mounted_by: who,
        ..entry
    }
}

/// ❗ A live SMB mount off port 445 is named with its port, as its saved row is:
/// two servers on one machine each exporting `private` read identically as
/// "private on localhost" (QA round 2).
#[test]
fn a_live_smb_mount_off_445_names_its_port() {
    assert_eq!(
        network_name(&mount("/Volumes/private", "smbfs", "//localhost:11482/private", false)),
        "private on localhost:11482"
    );
    assert_eq!(
        network_name(&mount("/Volumes/naspi", "smbfs", "//david@192.0.2.9/naspi", false)),
        "naspi on 192.0.2.9"
    );
}

/// A mount macOS marks `MNT_DONTBROWSE`: the plumbing, Xcode's `DeviceFS`,
/// and possibly a cloud client's drive that hides itself from Finder's sidebar.
fn unbrowsable(mount_point: &str, fs_type: &str) -> MountEntry {
    MountEntry {
        is_browsable: false,
        ..mount(mount_point, fs_type, "x", false)
    }
}

/// A `resolve_local` for local mounts whose enrichment the test doesn't care about.
fn plain_resolver(path: &str) -> LocalVolumeMeta {
    LocalVolumeMeta {
        name: volume_name_from_path(path),
        is_ejectable: false,
        icon: None,
        is_disk_image: false,
        uuid: None,
    }
}

/// A `resolve_local` that fails the test if invoked. Used to prove a network
/// mount is classified WITHOUT any blocking NSURL/DiskArbitration/NSWorkspace
/// call — the guarantee that a hung mount can't stall discovery.
fn forbidden_resolver(_path: &str) -> LocalVolumeMeta {
    panic!("resolve_local must NOT run for a network mount");
}

/// The OS's own sidebar rule decides, so the plumbing stays out without this
/// module naming a single system path.
#[test]
fn the_browsable_flag_is_what_separates_drives_from_plumbing() {
    let browsable = |path: &str| is_user_facing_mount(&mount(path, "apfs", "x", false), None);
    let hidden = |path: &str| is_user_facing_mount(&unbrowsable(path, "apfs"), None);

    assert!(browsable("/Volumes/MyDrive"));
    assert!(browsable("/Volumes/naspi"));
    // Everything macOS marks `MNT_DONTBROWSE`: `/System/Volumes/*`, `devfs`,
    // the Recovery volume, autofs triggers. The old prefix filter needed a
    // name check for Recovery and missed the rest.
    assert!(!hidden("/System/Volumes/Data"));
    assert!(!hidden("/Volumes/Recovery"));
    assert!(!hidden("/dev"));
    // The boot volume has its own row, browsable or not.
    assert!(!browsable("/"));
    // A `~/Library/CloudStorage` folder is published by the cloud arm.
    assert!(!browsable("/Volumes/Foo/Library/CloudStorage/Dropbox"));
    // Hidden by name (NSFileManager's old SkipHiddenVolumes).
    assert!(!browsable("/Volumes/.timemachine"));
}

/// An unbrowsable mount gets a row only when we recognize who serves it,
/// wherever it's mounted. Where it sits proves nothing: Xcode puts a system
/// mount inside the home folder.
#[test]
fn an_unbrowsable_mount_shows_only_when_its_provider_is_recognized() {
    let row = |path: &str, fs_type: &str| build_attached_location(&unbrowsable(path, fs_type), plain_resolver);

    let pcloud = row("/Users/sven/pCloud Drive", "pcloudfs").expect("a recognized provider's mount gets a row");
    assert_eq!(pcloud.category, LocationCategory::CloudDrive);
    assert!(
        row("/Volumes/pCloudDrive", "pcloudfs").is_some(),
        "outside the home folder too"
    );
    assert!(row("/Users/sven/.CMVolumes/S3", "macfuse").is_some());

    assert!(
        row("/Users/sven/vaults/work", "apfs").is_none(),
        "an unrecognized mount stays out"
    );
    assert!(
        row("/Users/sven", "nfs").is_none(),
        "a network-mounted home folder is not a drive"
    );
    assert!(
        row("/Users/sven/.hidden-vault", "macfuse").is_none(),
        "a name-hidden mount stays hidden"
    );
}

/// Regression anchor: Xcode's CoreDevice `DeviceFS` is an unbrowsable FSKit
/// mount inside the home folder, and showed up as a phantom "Devices" drive.
#[test]
fn xcodes_device_fs_mount_gets_no_row() {
    let device_fs = unbrowsable("/Users/sven/Library/Developer/CoreDevice/DeviceFS", "devicefs");
    assert!(build_attached_location(&device_fs, plain_resolver).is_none());
}

/// Admission asks "do we recognize the provider at all", categorization asks
/// "is it cloud storage": two questions from one detection. A VeraCrypt
/// container is a `macfuse` mount we recognize and aren't told to hide, and
/// it's a disk, not a cloud drive. Merging the two predicates loses it one
/// way or the other.
#[test]
fn an_unbrowsable_fuse_container_gets_a_row_as_an_ordinary_volume() {
    let vault = unbrowsable("/Users/sven/vaults/work", "macfuse");
    let loc = build_attached_location(&vault, plain_resolver).expect("a recognized FUSE mount gets a row");
    assert_eq!(loc.category, LocationCategory::AttachedVolume);
    assert!(!loc.is_cloud_mount, "a FUSE container keeps its index affordances");
}

/// The one path family where this arm could overlap the cloud arm: iCloud
/// Drive's folder. Nothing mounts there today. If something did, this arm's
/// row sits at the same path as the cloud arm's `cloud-icloud` row, so
/// `list_locations`'s path set keeps one of them, and it's in CLOUD either way.
/// Needs the real home folder, since provider detection reads it.
#[test]
fn a_mount_on_icloud_drive_lands_on_the_cloud_arms_path() {
    let home = dirs::home_dir().expect("a home folder");
    let icloud = home.join(crate::file_system::cloud_provider::ICLOUD_DRIVE_SUBPATH);
    let icloud = icloud.to_str().expect("a UTF-8 home folder");
    let loc = build_attached_location(&unbrowsable(icloud, "apfs"), plain_resolver)
        .expect("iCloud Drive's folder is a recognized provider");
    assert_eq!(loc.path, icloud, "the path `list_locations` dedupes on");
    assert_eq!(loc.category, LocationCategory::CloudDrive);
}

#[test]
fn smb_mount_classifies_without_blocking_enrichment() {
    // A wedged SMB mount must be classified purely from getfsstat data; the
    // blocking resolver must never run, so a dead NAS can't stall discovery.
    let m = mount("/Volumes/naspi", "smbfs", "//david@192.168.1.111/naspi", false);
    let loc = build_attached_location(&m, forbidden_resolver).expect("SMB mount is an attached volume");

    assert_eq!(
        loc.id,
        crate::file_system::volume::smb_volume_id("192.168.1.111", 445, "naspi")
    );
    assert!(loc.name.contains("naspi"), "name shows the share: {}", loc.name);
    assert!(loc.name.contains(" on "), "name shows 'share on server': {}", loc.name);
    // ❗ A disambiguated mount (`/Volumes/naspi-1`) named its tab after the mount dir.
    assert_eq!(
        loc.root_label.as_deref(),
        Some("naspi"),
        "a tab at its root says the share"
    );
    // ❗ The account the mount signed in as, which the hub shows while it's connected.
    assert_eq!(loc.mount_account.as_deref(), Some("david"));
    assert_eq!(loc.fs_type.as_deref(), Some("smbfs"));
    assert_eq!(loc.category, LocationCategory::AttachedVolume);
    assert!(!loc.is_ejectable, "network mounts take the safe non-blocking default");
    assert!(loc.icon.is_none());
    assert!(!loc.is_disk_image);
}

#[test]
fn nfs_mount_classifies_without_blocking_enrichment() {
    let m = mount("/Volumes/export", "nfs", "server:/export", true);
    let loc = build_attached_location(&m, forbidden_resolver).expect("NFS mount is an attached volume");
    assert_eq!(loc.id, crate::file_system::volume::path_volume_id("/Volumes/export"));
    assert_eq!(loc.name, "export");
    assert!(loc.mount_is_read_only, "MNT_RDONLY flag propagates from getfsstat");
    assert_eq!(loc.fs_type.as_deref(), Some("nfs"));
}

#[test]
fn local_mount_runs_the_enrichment_closure() {
    // Local mounts DO get the (safe) blocking enrichment; here we inject a
    // fake so the test stays hermetic and asserts the values flow through.
    // The UUID coming back through this closure is what the ID keys on, so
    // the same disk keeps its ID when macOS remounts it as `/Volumes/USB 1`.
    let m = mount("/Volumes/USB", "exfat", "/dev/disk4s1", false);
    let resolve = |path: &str| {
        assert!(path.starts_with("/Volumes/USB"));
        LocalVolumeMeta {
            name: "My USB".to_string(),
            is_ejectable: true,
            icon: Some("icon-data".to_string()),
            is_disk_image: false,
            uuid: Some("A1B2-C3D4".to_string()),
        }
    };
    let loc = build_attached_location(&m, resolve).expect("local mount is an attached volume");

    assert_eq!(
        loc.id,
        crate::file_system::volume::local_volume_id(Some("A1B2-C3D4"), "/Volumes/USB")
    );
    let remounted = mount("/Volumes/USB 1", "exfat", "/dev/disk4s1", false);
    let remounted_loc = build_attached_location(&remounted, resolve).expect("local mount is an attached volume");
    assert_eq!(
        loc.id, remounted_loc.id,
        "the same disk keeps its ID at a new mount point"
    );

    assert_eq!(loc.name, "My USB");
    assert!(loc.is_ejectable);
    assert_eq!(loc.icon.as_deref(), Some("icon-data"));
    assert_eq!(loc.fs_type.as_deref(), Some("exfat"));
}

#[test]
fn filtered_mount_yields_no_location() {
    // The boot volume and system mounts are dropped before any enrichment.
    assert!(build_attached_location(&mount("/", "apfs", "/dev/disk3s1", false), forbidden_resolver).is_none());
    assert!(build_attached_location(&unbrowsable("/System/Volumes/Data", "apfs"), forbidden_resolver).is_none());
}

// ========================================================================
// Another account's mounts
// ========================================================================

/// The two pCloud rows of a Mac with two accounts that both run it, as
/// `getfsstat` lists them: browsable, no `/dev/` source, one per home folder.
fn pcloud_drive(home: &str, who: MountedBy) -> MountEntry {
    mounted_by(
        who,
        mount(&format!("{home}/pCloud Drive"), "pcloudfs", "pCloud.fs", false),
    )
}

#[test]
fn the_owner_is_this_user_root_or_someone_else() {
    assert_eq!(MountedBy::from_owner(501, 501), MountedBy::ThisUser);
    assert_eq!(MountedBy::from_owner(0, 501), MountedBy::Root);
    assert_eq!(MountedBy::from_owner(502, 501), MountedBy::AnotherUser);
    // Cmdr run as root mounts as root, and that's still its own.
    assert_eq!(MountedBy::from_owner(0, 0), MountedBy::ThisUser);
}

/// Regression anchor: both accounts' pCloud drives got a row, and the other
/// one's showed as unavailable and dropped the pane in the home folder.
/// `forbidden_resolver` is the rest of the fix: no NSURL or icon lookup ever
/// runs against a drive this account can't open.
#[test]
fn another_accounts_cloud_drive_gets_no_row() {
    let theirs = pcloud_drive("/Users/rin", MountedBy::AnotherUser);
    assert!(build_attached_location(&theirs, forbidden_resolver).is_none());

    let theirs_fuse = mounted_by(
        MountedBy::AnotherUser,
        mount("/Volumes/vault", "macfuse", "veracrypt", false),
    );
    assert!(build_attached_location(&theirs_fuse, forbidden_resolver).is_none());
}

#[test]
fn this_accounts_cloud_drive_keeps_its_row() {
    let mine = pcloud_drive("/Users/sven", MountedBy::ThisUser);
    let loc = build_attached_location(&mine, plain_resolver).expect("this account's own drive gets a row");
    assert_eq!(loc.category, LocationCategory::CloudDrive);
}

/// A FUSE daemon running as root mounts for everyone.
#[test]
fn a_root_mounted_fuse_drive_keeps_its_row() {
    let system_wide = mounted_by(MountedBy::Root, mount("/Volumes/vault", "macfuse", "veracrypt", false));
    assert!(build_attached_location(&system_wide, plain_resolver).is_some());
}

/// A disk plugged in while someone else was logged in is "mounted by" them,
/// and every account can still use it.
#[test]
fn a_disk_another_account_mounted_keeps_its_row() {
    let usb = mounted_by(
        MountedBy::AnotherUser,
        mount("/Volumes/USB", "exfat", "/dev/disk4s1", false),
    );
    assert!(build_attached_location(&usb, plain_resolver).is_some());
}

/// Whether another account's network mount is usable from this one is
/// unverified, so the rule leaves those alone rather than hide a share
/// someone can reach.
#[test]
fn a_network_mount_another_account_made_keeps_its_row() {
    for (fs_type, source) in [
        ("smbfs", "//rin@192.0.2.9/naspi"),
        ("nfs", "server:/export"),
        ("webdav", "https://dav.example.com/"),
        ("afpfs", "//rin@nas/share"),
    ] {
        let share = mounted_by(MountedBy::AnotherUser, mount("/Volumes/share", fs_type, source, false));
        assert!(
            build_attached_location(&share, forbidden_resolver).is_some(),
            "another account's {fs_type} mount should keep its row"
        );
    }
}

/// The registry sweep skips another account's mount, so nothing registers a
/// volume for it, and the watcher's by-path question gives the same answer.
#[test]
fn the_registry_sweep_skips_another_accounts_mount() {
    let table = || {
        vec![
            mount("/", "apfs", "/dev/disk3s1s1", true),
            pcloud_drive("/Users/sven", MountedBy::ThisUser),
            pcloud_drive("/Users/rin", MountedBy::AnotherUser),
            mounted_by(
                MountedBy::AnotherUser,
                mount("/Volumes/USB", "exfat", "/dev/disk4s1", false),
            ),
        ]
    };

    assert_eq!(
        registrable_roots(table()),
        ["/", "/Users/sven/pCloud Drive", "/Volumes/USB"]
    );

    let table = table();
    assert!(private_to_another_user_at(&table, "/Users/rin/pCloud Drive"));
    assert!(!private_to_another_user_at(&table, "/Users/sven/pCloud Drive"));
    assert!(!private_to_another_user_at(&table, "/Volumes/USB"));
    assert!(
        !private_to_another_user_at(&table, "/Volumes/Gone"),
        "a path nothing is mounted at is nobody's"
    );
}

// ========================================================================
// One volume ID means one published mount root
// ========================================================================

#[test]
fn two_mount_roots_for_one_share_collapse_to_the_shortest_path() {
    // What this covers is the WIRING: a real pair of mount-table entries for
    // one share derives one volume ID (a share keys on `(server, port,
    // share)`), and `get_attached_volumes`'s collapse turns that into one
    // published location at the original path, while a different volume stays
    // its own row. The collapse rule itself is proved on a toy struct in
    // `cmdr_fs::volume::canonical_root`.
    let first = mount("/Volumes/naspi", "smbfs", "//david@192.168.1.111/naspi", false);
    let second = mount("/Volumes/naspi-1", "smbfs", "//david@192.168.1.111/naspi", false);
    let nfs = mount("/Volumes/export", "nfs", "server:/export", true);
    let locations: Vec<LocationInfo> = [&second, &first, &nfs]
        .iter()
        .filter_map(|m| build_attached_location(m, forbidden_resolver))
        .collect();
    assert_eq!(locations.len(), 3, "every mount starts out as its own location");

    let collapsed = collapse_by_volume_id(locations);
    assert_eq!(collapsed.len(), 2, "one volume ID publishes one location");
    assert_eq!(
        collapsed[0].path, "/Volumes/naspi",
        "the canonical root is the original mount, whatever order they arrive in"
    );
    assert_eq!(
        collapsed[1].path, "/Volumes/export",
        "a different volume stays separate"
    );
}

#[test]
fn enumerate_mounts_finds_the_boot_volume() {
    // getfsstat should always return at least the root mount on a live system,
    // and it must never block (this test would hang if it did).
    let mounts = enumerate_mounts().expect("getfsstat answers on a live system");
    assert!(!mounts.is_empty(), "getfsstat returned no mounts");
    assert!(mounts.iter().any(|m| m.mount_point == "/"), "root mount missing");
}

#[test]
fn is_mount_point_answers_from_the_mount_table() {
    assert_eq!(is_mount_point("/"), Some(true), "the boot volume is a mount point");
    // A folder ON a mount is not one: an eject can't mistake it for a live mount.
    assert_eq!(is_mount_point("/usr/bin"), Some(false));
}

#[test]
fn mount_under_picks_the_deepest_whole_component_mount() {
    let mounts = vec![
        mount("/", "apfs", "/dev/disk3s1s1", true),
        mount("/System/Volumes/Data", "apfs", "/dev/disk3s5", false),
        mount("/Volumes/naspi", "smbfs", "//david@192.0.2.9/naspi", false),
        mount("/Volumes/naspi-1", "exfat", "/dev/disk6s1", false),
    ];
    let fs_type = |path: &str| mount_under(&mounts, Path::new(path)).map(|m| m.fs_type.as_str());
    assert_eq!(fs_type("/Volumes/naspi/photos/2024"), Some("smbfs"));
    assert_eq!(fs_type("/Volumes/naspi"), Some("smbfs"));
    // A sibling whose name extends the share's is its own mount, not the share's subfolder.
    assert_eq!(fs_type("/Volumes/naspi-1/backup"), Some("exfat"));
    assert_eq!(fs_type("/Users/david"), Some("apfs"));
}

#[test]
fn mount_under_reaches_the_last_of_stacked_mounts() {
    let mounts = vec![
        mount("/", "apfs", "/dev/disk3s1s1", true),
        mount("/Volumes/stack", "apfs", "/dev/disk4s1", false),
        mount("/Volumes/stack", "smbfs", "//server/share", false),
    ];
    assert_eq!(
        mount_under(&mounts, Path::new("/Volumes/stack/x")).map(|m| m.fs_type.as_str()),
        Some("smbfs")
    );
}
