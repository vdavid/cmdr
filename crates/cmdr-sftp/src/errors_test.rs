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
        let mapped = classify(kind, "denied", "/srv/data/notes.txt");
        assert!(matches(&mapped), "{kind:?} mapped to {mapped:?}");
    }
}

#[test]
fn the_catch_all_code_stays_unclassified() {
    // ⚠️ `SSH_FX_FAILURE` covers `EEXIST`, `ENOTEMPTY`, and most of errno on
    // OpenSSH. Guessing `AlreadyExists` here would make the folder-merge walker
    // merge into a directory that doesn't exist; telling them apart takes a stat
    // probe on the write path, where there's something to probe.
    let mapped = classify(SftpErrorKind::Failure, "failure", "/srv/data/notes.txt");
    assert!(matches!(mapped, VolumeError::IoError { .. }), "got {mapped:?}");
}

#[test]
fn a_dead_channel_reads_as_a_disconnect_rather_than_an_io_hiccup() {
    // The pane has to stop asking rather than retry into a session that's gone.
    let mapped = map_sftp_error(
        &SftpError::IOError(std::io::Error::from(std::io::ErrorKind::ConnectionReset)),
        "/srv/data/notes.txt",
    );
    assert!(matches!(mapped, VolumeError::DeviceDisconnected(_)), "got {mapped:?}");
}

// ── Resolving the catch-all, after the fact ──────────────────────────

#[test]
fn a_name_that_something_already_holds_reads_as_already_exists() {
    // The whole reason the write path probes at all. `create_file`'s exclusive
    // open, `create_directory`, and a no-clobber rename's destination claim all
    // owe `AlreadyExists`, and five conformance assertions plus the folder-merge
    // walker branch on getting it.
    for found in [WhatIsThere::NotADirectory, WhatIsThere::Directory] {
        let resolved = resolve(
            SftpErrorKind::Failure,
            "failure",
            "/srv/data/notes.txt",
            Attempted::TakingAName,
            found,
        );
        assert!(
            matches!(resolved, VolumeError::AlreadyExists(_)),
            "{found:?} resolved to {resolved:?}"
        );
    }
}

#[test]
fn a_name_with_nothing_at_it_stays_unclassified() {
    // ❗ The probe may only make a report MORE accurate. A `Failure` on a path
    // that holds nothing is out of space, a read-only export, a quota: guessing
    // `AlreadyExists` there would make the merge walker merge into a directory
    // that isn't there.
    let resolved = resolve(
        SftpErrorKind::Failure,
        "failure",
        "/srv/data/notes.txt",
        Attempted::TakingAName,
        WhatIsThere::Nothing,
    );
    assert!(matches!(resolved, VolumeError::IoError { .. }), "got {resolved:?}");
}

#[test]
fn a_directory_that_refused_to_go_carries_the_errno_every_backend_reports() {
    // `Volume::delete` must refuse a directory that still holds something, and
    // the app renders that from the errno rather than from wording. MTP and
    // LocalPosix both answer this way, so SFTP has to as well or the same
    // refusal reads differently per backend.
    let resolved = resolve(
        SftpErrorKind::Failure,
        "failure",
        "/srv/data/full-dir",
        Attempted::RemovingANode,
        WhatIsThere::Directory,
    );
    assert!(
        matches!(resolved, VolumeError::IoError { raw_os_error: Some(errno), .. } if errno == ENOTEMPTY),
        "got {resolved:?}"
    );
}

#[test]
fn a_probe_that_found_nothing_leaves_a_failed_delete_unclassified() {
    for found in [WhatIsThere::Nothing, WhatIsThere::NotADirectory] {
        let resolved = resolve(
            SftpErrorKind::Failure,
            "failure",
            "/srv/data/notes.txt",
            Attempted::RemovingANode,
            found,
        );
        assert!(matches!(resolved, VolumeError::IoError { .. }), "got {resolved:?}");
    }
}

#[test]
fn a_code_the_protocol_does_distinguish_is_never_second_guessed() {
    // ❗ The probe resolves ONE code. A server that said `SSH_FX_NO_SUCH_FILE`
    // was precise, and turning that into `AlreadyExists` because a racing writer
    // happened to create the path a millisecond later would be a lie built out
    // of a stale answer.
    let resolved = resolve(
        SftpErrorKind::NoSuchFile,
        "no such file",
        "/srv/data/notes.txt",
        Attempted::TakingAName,
        WhatIsThere::NotADirectory,
    );
    assert!(matches!(resolved, VolumeError::NotFound(_)), "got {resolved:?}");
}

#[test]
fn the_two_path_carrying_variants_carry_the_path_and_not_the_servers_wording() {
    // ❗ The payload, not the classification. `VolumeError::NotFound` and
    // `PermissionDenied` are DEFINED to carry the path, and the transfer layer
    // forwards that string verbatim into `SourceNotFound { path }` for the
    // frontend to render as the name of the missing file. Carrying the server's
    // own sentence instead shipped `source_not_found` with a path of
    // "Err Message: No such file, Language Tag: " — a filename that was never on
    // anybody's disk.
    //
    // Reading the payload out is not error classification (nothing branches on
    // it); it's the only way to assert what a variant carries.
    const PATH: &str = "/srv/data/notes.txt";
    let VolumeError::NotFound(carried) = classify(SftpErrorKind::NoSuchFile, "No such file", PATH) else {
        panic!("NoSuchFile must map to NotFound");
    };
    assert_eq!(carried, PATH, "NotFound must carry the path");

    let VolumeError::PermissionDenied(carried) = classify(SftpErrorKind::PermDenied, "Permission denied", PATH) else {
        panic!("PermDenied must map to PermissionDenied");
    };
    assert_eq!(carried, PATH, "PermissionDenied must carry the path");
}

#[test]
fn a_resolved_catch_all_carries_the_path_through_every_arm_that_can() {
    // `resolve` had the path in scope all along and spent it on the
    // `AlreadyExists` arm only; its fall-through arms reach the same two
    // path-carrying variants and owe the same payload.
    const PATH: &str = "/srv/data/notes.txt";
    let resolved = resolve(
        SftpErrorKind::NoSuchFile,
        "No such file",
        PATH,
        Attempted::TakingAName,
        WhatIsThere::Nothing,
    );
    let VolumeError::NotFound(carried) = resolved else {
        panic!("got {resolved:?}");
    };
    assert_eq!(carried, PATH);

    let resolved = resolve(
        SftpErrorKind::Failure,
        "failure",
        PATH,
        Attempted::TakingAName,
        WhatIsThere::NotADirectory,
    );
    let VolumeError::AlreadyExists(carried) = resolved else {
        panic!("got {resolved:?}");
    };
    assert_eq!(carried, PATH, "the arm that already carried the path still does");
}
