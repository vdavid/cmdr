use super::*;

/// ❗ Same-named siblings are a device's own refusal, ❌ never SMB's look-alike
/// `AmbiguousName`: the two read differently to a person (#523), and the
/// frontend picks its words by the variant alone.
#[test]
fn same_named_siblings_map_to_a_duplicate_name() {
    let mapped = map_mtp_error(MtpConnectionError::AmbiguousName {
        device_id: "mtp-1".to_string(),
        path: "/DCIM/IMG_1.JPG".to_string(),
    });
    assert!(
        matches!(&mapped, VolumeError::DuplicateName(path) if path == "/DCIM/IMG_1.JPG"),
        "got {mapped:?}"
    );
}
