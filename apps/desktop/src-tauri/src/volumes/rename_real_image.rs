//! The rename handler on real synthetic disk images (macOS).
//!
//! `#[ignore]`d: each attaches a real APFS or HFS+ image. Hand-run with
//! `cargo nextest run -p cmdr --run-ignored only -E 'test(volumes::rename_real_image::)'`,
//! or through `pnpm check disk-images`. Serialized in the `disk-image` nextest group,
//! and machine-wide by the harness's session lock.
//!
//! macOS renames a mounted volume by moving its mount point and posting one
//! `NSWorkspaceDidRenameVolumeNotification`, with no unmount in between. These prove
//! the handler keeps the volume's id and moves it to the new root, and tells the
//! panes it MOVED (so a pane deep inside keeps its place) rather than that it left.

use cmdr_fs::testing::disk_images::{DiskImage, DiskImageSession, ImageSpec};

use super::rename::handle_volume_renamed;
use super::watcher::handle_volume_mounted;
use crate::file_system::volume::manager::get_volume_manager;
use crate::volume_broadcast::{RootChangeKind, volume_root_changes};

fn a_renamed_volume_keeps_its_id_at_its_new_root(spec: ImageSpec) {
    let session = DiskImageSession::acquire();
    let mut image = DiskImage::attach(&session, spec).expect("attach the image");
    let before = image.volumes()[0].mount_point.to_string_lossy().into_owned();
    handle_volume_mounted(&before);
    let id = super::volume_id_for_mount(&before);
    assert!(
        get_volume_manager().get(&id).is_some(),
        "precondition: the volume is registered"
    );

    let after = image
        .rename_volume(0)
        .expect("rename the volume")
        .mount_point
        .to_string_lossy()
        .into_owned();
    handle_volume_renamed(&before, &after);

    let serving = get_volume_manager().get(&id).expect("still registered under its id");
    assert_eq!(serving.root().to_string_lossy(), after, "it serves from the new root");
    assert!(
        get_volume_manager()
            .find_by_root(std::path::Path::new(&before))
            .is_none(),
        "nothing finds it at the old root"
    );
    let changes = volume_root_changes(&id);
    assert!(
        changes.iter().any(|change| change.kind == RootChangeKind::Moved
            && change.old_root == before
            && change.new_root == after),
        "the panes hear that it moved: {changes:?}"
    );

    get_volume_manager().unregister(&id);
    image.force_detach().expect("detach the image");
}

#[test]
#[ignore = "attaches a real APFS disk image via hdiutil; run with --run-ignored"]
fn a_renamed_apfs_volume_keeps_its_id_at_its_new_root() {
    a_renamed_volume_keeps_its_id_at_its_new_root(ImageSpec::Apfs);
}

#[test]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
fn a_renamed_hfs_volume_keeps_its_id_at_its_new_root() {
    a_renamed_volume_keeps_its_id_at_its_new_root(ImageSpec::Hfs);
}
