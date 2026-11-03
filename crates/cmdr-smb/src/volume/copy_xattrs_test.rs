use super::*;
use cmdr_fs::metadata::MetadataCaps;

fn caps(stores: MetadataClasses, reports: MetadataClasses) -> MetadataCaps {
    MetadataCaps {
        stores,
        reports,
        ..MetadataCaps::NONE
    }
}

const STREAMS: MetadataClasses = MetadataClasses::XATTRS.union(MetadataClasses::CREATED);

/// A destination that keeps xattrs wants their values; one that can't wants
/// their names for the loss note; a pair that never asks, or a share without
/// streams, lists nothing at all.
#[test]
fn a_read_lists_only_for_a_copy_that_asks_and_a_share_that_has_streams() {
    let smb = caps(STREAMS, STREAMS);
    let keeps = MetadataAsk::for_copy(smb, MetadataCaps::ALL);
    let loses = MetadataAsk::for_copy(smb, caps(MetadataClasses::CREATED, MetadataClasses::NONE));
    assert_eq!(XattrAsk::of(keeps, true), XattrAsk::Values);
    assert_eq!(XattrAsk::of(loses, true), XattrAsk::Names);
    assert_eq!(XattrAsk::of(keeps, false), XattrAsk::Nothing);
    assert_eq!(XattrAsk::of(MetadataAsk::default(), true), XattrAsk::Nothing);
    assert!(!XattrAsk::Nothing.lists());
}

/// ❗ A names-only answer never reaches `metadata`, or a destination would
/// write an empty value over the file's real one.
#[test]
fn names_alone_are_an_outline_and_never_values() {
    let tags = Xattr {
        name: OsString::from("com.apple.metadata:_kMDItemUserTags"),
        value: Vec::new(),
    };
    let names = SourceXattrs::Names(vec![tags.clone()]);
    assert!(names.values().is_empty());
    assert_eq!(names.outline(), vec![tags.clone()]);
    let values = SourceXattrs::Values(vec![tags.clone()]);
    assert_eq!(values.values(), vec![tags.clone()]);
    assert_eq!(values.outline(), vec![tags]);
}

/// The `._` file sits beside its file, in the same folder, at the share root too.
#[test]
fn the_apple_double_file_sits_beside_its_file() {
    assert_eq!(apple_double_path("a/b/.cmdr-tmp-x"), "a/b/._.cmdr-tmp-x");
    assert_eq!(apple_double_path(".cmdr-tmp-x"), "._.cmdr-tmp-x");
}

#[test]
fn provenance_is_never_carried() {
    assert!(!carries_stream(PROVENANCE));
    assert!(carries_stream("AFP_AfpInfo"));
    assert!(carries_stream("com.apple.metadata:_kMDItemUserTags"));
}
