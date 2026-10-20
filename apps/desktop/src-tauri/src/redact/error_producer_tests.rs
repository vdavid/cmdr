//! Typed errors logged through `cmdr_fs::log_error::LogError`, through the span pass: each path
//! and name the error carries ships as one token of its kind, at a `{}` and a `{:?}` site, and a
//! hostile name in a field can't close its span early. Every cell feeds what `cmdr_fs` writes.

use super::markers_tests::{HEAD, assert_ships_none, context, hashes, shape};
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::{LogPrivate, RESERVED};
use cmdr_fs::volume::VolumeError;
use cmdr_fs::volume::git_error::{FriendlyGitError, FriendlyGitErrorKind};

/// A share-relative path: no leading `/`, so no regex claims it in prose.
const SHARE_PATH: &str = "Trips/Summer (Lake House) 2026/IMG 0001.jpg";
const PRIVATE: [&str; 6] = ["Trips", "Summer", "Lake", "House", "IMG", "0001"];

/// Every `VolumeError` variant that carries a path, holding `path`.
fn pathed_errors(path: &str) -> Vec<VolumeError> {
    let p = || path.to_string();
    vec![
        VolumeError::NotFound(p()),
        VolumeError::PermissionDenied {
            path: p(),
            raw_os_error: Some(13),
        },
        VolumeError::AlreadyExists(p()),
        VolumeError::NotConnected(p()),
        VolumeError::ReadOnly(p()),
        VolumeError::IsADirectory(p()),
        VolumeError::NotADirectory(p()),
        VolumeError::DeletePending(p()),
        VolumeError::AmbiguousName(p()),
        VolumeError::DuplicateName(p()),
        VolumeError::StaleDestinationHandle(p()),
        VolumeError::ColdStorage(p()),
        VolumeError::SourceChanged(p()),
        VolumeError::FriendlyGit(FriendlyGitError::new(FriendlyGitErrorKind::IndexLocked, p())),
    ]
}

/// The SMB backend's metadata line (`cmdr-smb` `attributes.rs`), which logs the folder marked
/// and the error after it, the shape every backend's "couldn't" lines share.
fn smb_folder_line(error: &VolumeError) -> String {
    format!(
        "{HEAD}SmbVolume::folder_metadata: {} reports no flags: {}",
        LogPrivate::dir_path("Trips"),
        LogError(error)
    )
}

#[test]
fn a_volume_error_logged_through_log_error_ships_no_name_from_its_path() {
    for error in pathed_errors(SHARE_PATH) {
        let line = smb_folder_line(&error);
        assert_ships_none(&line, &PRIVATE);
        let quoted = format!("{HEAD}copy: step failed error={:?}, retrying", LogError(&error));
        assert_ships_none(&quoted, &PRIVATE);
        assert!(
            context().redact_line(&quoted).ends_with(", retrying"),
            "{}",
            context().redact_line(&quoted)
        );
    }
}

/// The `{}` and the `{:?}` spelling of one error hash its path to the same token.
#[test]
fn both_spellings_of_a_logged_error_hash_its_path_alike() {
    let error = VolumeError::NotFound(SHARE_PATH.to_string());
    let shown = hashes(&context().redact_line(&format!("{HEAD}read: {}", LogError(&error))));
    let quoted = hashes(&context().redact_line(&format!("{HEAD}read: error={:?}", LogError(&error))));
    assert!(!shown.is_empty());
    assert_eq!(shown, quoted);
}

/// The kind the variant knows reaches the report: a stale handle is a folder, so its leaf never
/// gets an extension, and a source that changed is a file, so it keeps one.
#[test]
fn each_variant_marks_its_path_as_the_kind_it_carries() {
    let stale = VolumeError::StaleDestinationHandle("Phone/DCIM/Camera.backup".to_string());
    assert_eq!(
        shape(&format!("{HEAD}mtp: write refused: {}", LogError(&stale))),
        format!("{HEAD}mtp: write refused: Destination folder handle was stale: <dir>/<dir>/<dir>")
    );
    let changed = VolumeError::SourceChanged("bucket/Trips/Lake House.mov".to_string());
    assert_eq!(
        shape(&format!("{HEAD}s3: copy stopped: {}", LogError(&changed))),
        format!("{HEAD}s3: copy stopped: The source changed during the copy: <dir>/<dir>/<file>.mov")
    );
}

/// A field holding a hostile name stays one span, and the error's foreign text (a server's own
/// message) can't forge a marker: its reserved chars print as U+FFFD.
#[test]
fn a_hostile_name_in_an_error_field_never_closes_its_span_early() {
    let mut values: Vec<String> = RESERVED.map(|c| format!("Trips/Lake{c}House Rest/x.jpg")).collect();
    values.extend(
        [
            "Trips/Lake\\u{fdd1}House Rest/x.jpg",
            "Trips/Lake \"House\": Rest, a; b key=v -> )/x.jpg",
            "Trips/Lake\nHouse\r/x.jpg",
        ]
        .map(str::to_string),
    );
    for value in values {
        for error in pathed_errors(&value) {
            assert_ships_none(&smb_folder_line(&error), &["Lake", "House", "Rest", "Trips"]);
        }
        let foreign = VolumeError::IoError {
            message: format!("the server said no about {value}"),
            raw_os_error: None,
        };
        let line = format!("{HEAD}read: {}", LogError(&foreign));
        assert!(!line.contains(|c| RESERVED.contains(&c)), "{line:?}");
    }
}

/// The errors that nest a `VolumeError` (an instant mutation's refusal, a conflict check's, a
/// reconnect's, a network scan's) mark its path through the nesting, and a mutation's own path
/// and asked-for name too. Their plain spelling ships the share-relative path whole.
#[test]
fn an_error_nesting_a_volume_error_marks_the_path_through_it() {
    use crate::file_system::volume::reconnect_error::ReconnectError;
    use cmdr_operations::{MutationError, VolumeScanError as ConflictScanError};

    let inner = || VolumeError::NotFound(SHARE_PATH.to_string());
    let lines: Vec<(String, String)> = vec![
        (
            format!("{HEAD}rename refused: {}", LogError(&MutationError::from(inner()))),
            format!("{HEAD}rename refused: {}", MutationError::from(inner())),
        ),
        (
            format!(
                "{HEAD}rename refused: {}",
                LogError(&MutationError::NotFound {
                    path: SHARE_PATH.into()
                })
            ),
            format!(
                "{HEAD}rename refused: {}",
                MutationError::NotFound {
                    path: SHARE_PATH.into()
                }
            ),
        ),
        (
            format!(
                "{HEAD}rename refused: {}",
                LogError(&MutationError::AlreadyExists {
                    name: "IMG 0001 Lake House.jpg".into()
                })
            ),
            format!(
                "{HEAD}rename refused: {}",
                MutationError::AlreadyExists {
                    name: "IMG 0001 Lake House.jpg".into()
                }
            ),
        ),
        (
            format!(
                "{HEAD}conflict check gave up: {}",
                LogError(&ConflictScanError::Volume { error: inner() })
            ),
            format!(
                "{HEAD}conflict check gave up: {}",
                ConflictScanError::Volume { error: inner() }
            ),
        ),
        (
            format!("{HEAD}reconnect: {}", LogError(&ReconnectError::from(inner()))),
            format!("{HEAD}reconnect: {}", ReconnectError::from(inner())),
        ),
    ];
    for (marked, plain) in lines {
        assert_ships_none(&marked, &PRIVATE);
        assert!(
            PRIVATE.iter().any(|word| context().redact_line(&plain).contains(word)),
            "the plain spelling should leak, or this cell proves nothing: {plain}"
        );
    }
}

/// The backend and viewer errors that carry a path of their own (an MTP object, an archive
/// entry, a file the viewer couldn't find) mark it, and their plain spelling ships it.
#[test]
fn a_backend_error_carrying_a_path_marks_it() {
    use crate::file_viewer::ViewerError;
    use cmdr_archive::ArchiveError;
    use cmdr_mtp::MtpConnectionError;

    let device_id = || "mtp-20-5".to_string();
    let lines: Vec<(String, String)> = vec![
        (
            format!(
                "{HEAD}mtp: {}",
                LogError(&MtpConnectionError::ObjectNotFound {
                    device_id: device_id(),
                    path: SHARE_PATH.into()
                })
            ),
            format!(
                "{HEAD}mtp: {}",
                MtpConnectionError::ObjectNotFound {
                    device_id: device_id(),
                    path: SHARE_PATH.into()
                }
            ),
        ),
        (
            format!(
                "{HEAD}archive: {}",
                LogError(&ArchiveError::NotFound(SHARE_PATH.into()))
            ),
            format!("{HEAD}archive: {}", ArchiveError::NotFound(SHARE_PATH.into())),
        ),
        (
            format!(
                "{HEAD}viewer: {}",
                LogError(&ViewerError::NotFound {
                    path: SHARE_PATH.into()
                })
            ),
            format!(
                "{HEAD}viewer: {}",
                ViewerError::NotFound {
                    path: SHARE_PATH.into()
                }
            ),
        ),
    ];
    for (marked, plain) in lines {
        assert_ships_none(&marked, &PRIVATE);
        assert!(
            PRIVATE.iter().any(|word| context().redact_line(&plain).contains(word)),
            "the plain spelling should leak, or this cell proves nothing: {plain}"
        );
    }
}
