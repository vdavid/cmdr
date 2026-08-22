//! The protocol's four distinguishable codes, mapped onto Cmdr's vocabulary.
//!
//! ❗ Every assertion matches on a VARIANT. `error-string-match` forbids
//! recovering a classification from a message, in tests as much as in production:
//! a wording change must never silently reclassify a failure.

use super::*;

#[test]
fn the_codes_the_protocol_distinguishes_map_to_typed_variants() {
    // SFTP v3 tells apart exactly these. Everything else is `SSH_FX_FAILURE`,
    // which is genuinely unclassified rather than something to guess at.
    for (kind, matches) in [
        (
            SftpErrorKind::NoSuchFile,
            (|e: &VolumeError| matches!(e, VolumeError::NotFound(_))) as fn(&VolumeError) -> bool,
        ),
        (SftpErrorKind::PermDenied, |e| {
            matches!(e, VolumeError::PermissionDenied(_))
        }),
        (SftpErrorKind::OpUnsupported, |e| matches!(e, VolumeError::NotSupported)),
    ] {
        let mapped = classify(kind, "denied");
        assert!(matches(&mapped), "{kind:?} mapped to {mapped:?}");
    }
}

#[test]
fn the_catch_all_code_stays_unclassified() {
    // ⚠️ `SSH_FX_FAILURE` covers `EEXIST`, `ENOTEMPTY`, and most of errno on
    // OpenSSH. Guessing `AlreadyExists` here would make the folder-merge walker
    // merge into a directory that doesn't exist; telling them apart takes a stat
    // probe on the write path, where there's something to probe.
    let mapped = classify(SftpErrorKind::Failure, "failure");
    assert!(matches!(mapped, VolumeError::IoError { .. }), "got {mapped:?}");
}

#[test]
fn a_dead_channel_reads_as_a_disconnect_rather_than_an_io_hiccup() {
    // The pane has to stop asking rather than retry into a session that's gone.
    let mapped = map_sftp_error(&SftpError::IOError(std::io::Error::from(
        std::io::ErrorKind::ConnectionReset,
    )));
    assert!(matches!(mapped, VolumeError::DeviceDisconnected(_)), "got {mapped:?}");
}
