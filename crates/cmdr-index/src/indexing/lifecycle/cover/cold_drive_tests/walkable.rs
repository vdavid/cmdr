//! Which drives a walk can take at all, and which walker reads them. A drive
//! nothing has mounted, a share or a phone (the trait walk, never the local
//! one), and a volume its own full scan is already covering.

use super::*;

/// A drive that isn't mounted has nothing to walk and nothing to root an index
/// at, so it reads as what it is: not indexed.
#[test]
fn an_unmounted_volume_is_not_walkable() {
    let drive = ColdDrive::new("cover-cold-unmounted-test");
    assert!(
        matches!(
            drive.index.cover(
                "nothing-is-mounted-here",
                vec![drive.path("x")],
                CoverageDimension::Listing,
                CancellationToken::new(),
            ),
            Err(crate::indexing::handle::IndexError::NotIndexed { .. })
        ),
        "an unmounted drive can't be bootstrapped"
    );
}

/// A share and a phone are walked over the `Volume` trait, and the LOCAL guarded
/// walker is never pointed at one.
///
/// That half is the data-safety rule: walking a network mount locally traverses a
/// share over syscalls that block for minutes, and the rows it wrote would fight
/// the trait scanner's. What decides it is typed facts — a live smb2 session, a
/// network filesystem, MTP's own id vocabulary — never a path substring. The walk
/// itself is `network_tests.rs`.
#[test]
fn a_share_or_a_phone_walks_over_the_trait_and_never_locally() {
    let walks_over_the_trait = |drive: &ColdDrive| {
        bootstrap::walkable_volume(drive.volume_id)
            .expect("a registered volume is walkable")
            .kind
            .is_trait_scanned()
    };

    {
        let share = ColdDrive::with_volume("cover-cold-share-test", |volume| {
            volume
                .with_local_fs_access()
                .with_smb_connection_state(cmdr_fs::volume::SmbConnectionState::Direct)
        });
        assert!(walks_over_the_trait(&share), "a live smb2 session is not local ground");
    }
    {
        // A phone's files exist only over PTP: no local path to walk at all.
        let phone = ColdDrive::with_volume("cover-cold-phone-test", |volume| volume);
        assert!(
            walks_over_the_trait(&phone),
            "a volume with no local filesystem access is not local ground"
        );
    }
    // And a phone by its own id vocabulary, which is what routes MTP everywhere
    // else. It's asked FIRST, before any mount probe: `mtp://…` is not a path a
    // `statfs` can answer for.
    let phone = ColdDrive::with_volume("mtp-serial:1", |volume| volume.with_local_fs_access());
    assert_eq!(
        bootstrap::walkable_volume(phone.volume_id)
            .expect("a phone is walkable")
            .kind,
        IndexVolumeKind::Mtp,
    );
}

/// A volume whose own full scan is running isn't walked at all.
///
/// Two reasons, and either alone would be enough: the scan already covers
/// everything a search would want walked, and a walk beside it allocates fresh
/// ids for names the scan is inserting under its own — `INSERT OR IGNORE` drops
/// whichever loses and orphans everything below it.
#[test]
fn a_volume_mid_full_scan_is_not_walked() {
    let drive = ColdDrive::new("cover-scan-in-progress-test");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");

    // A writer-only start is the shape a walk leaves: Running, nothing scanning.
    crate::indexing::lifecycle::state::start_indexing_for(
        drive.volume_id,
        drive.tree.path().to_path_buf(),
        IndexVolumeKind::Local,
        true,
        crate::indexing::lifecycle::state::Activation::WriterOnly,
    )
    .expect("stand the index up");
    assert!(
        context_for_walk(drive.volume_id).is_ok(),
        "precondition: with no scan running, the walk reuses this writer"
    );

    crate::indexing::lifecycle::state::set_scanning_for_test(drive.volume_id, true);
    assert!(
        matches!(context_for_walk(drive.volume_id), Err(NoCoverContext::ScanInProgress)),
        "a scan owns the volume while it runs"
    );

    crate::indexing::lifecycle::state::set_scanning_for_test(drive.volume_id, false);
    assert!(
        context_for_walk(drive.volume_id).is_ok(),
        "and hands it back when it's done"
    );
}
