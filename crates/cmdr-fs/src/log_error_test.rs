use super::*;
use crate::log_private::{CLOSER, LogKind, RESERVED};
use crate::volume::VolumeError;
use crate::volume::git_error::{FriendlyGitError, FriendlyGitErrorKind};

/// One of each `VolumeError` shape: path-carrying, message-carrying, and empty.
fn errors(path: &str) -> Vec<VolumeError> {
    let p = || path.to_string();
    vec![
        VolumeError::NotFound(p()),
        VolumeError::PermissionDenied {
            path: p(),
            raw_os_error: Some(1),
        },
        VolumeError::AlreadyExists(p()),
        VolumeError::NotConnected(p()),
        VolumeError::LinkTargetUnreachable(p()),
        VolumeError::LinkNeedsConsent(Box::new(crate::volume::LinkSignIn {
            path: p(),
            server: "nas".into(),
            target: "fs01".into(),
        })),
        VolumeError::ReadOnly(p()),
        VolumeError::IsADirectory(p()),
        VolumeError::NotADirectory(p()),
        VolumeError::DeletePending(p()),
        VolumeError::AmbiguousName(p()),
        VolumeError::DuplicateName(p()),
        VolumeError::StaleDestinationHandle(p()),
        VolumeError::ColdStorage(p()),
        VolumeError::SourceChanged(p()),
        VolumeError::FriendlyGit(FriendlyGitError::new(FriendlyGitErrorKind::CorruptRepo, p())),
        VolumeError::DeviceDisconnected(format!("lost {path}").into()),
        VolumeError::IoError {
            message: format!("refused {path}").into(),
            raw_os_error: Some(5),
        },
        VolumeError::NotSupported,
        VolumeError::NeedsPassword { wrong_attempt: true },
    ]
}

fn without_reserved(text: &str) -> String {
    text.chars().filter(|c| !RESERVED.contains(c)).collect()
}

/// What a person, a store, or IPC sees never carries a marker: `Display`, `Debug`, and the JSON.
#[test]
fn display_debug_and_json_stay_plain() {
    for error in errors("Trips/Lake House/a.jpg") {
        for text in [
            error.to_string(),
            format!("{error:?}"),
            serde_json::to_string(&error).expect("serializes"),
        ] {
            assert!(!text.contains(|c| RESERVED.contains(&c)), "{text:?}");
        }
    }
}

/// The logged form is `Display` plus markers: one function writes both, so they can't drift.
#[test]
fn the_logged_text_is_display_once_its_markers_are_gone() {
    for error in errors("Trips/Lake House/a.jpg") {
        assert_eq!(
            without_reserved(&LogError(&error).to_string()),
            error.to_string(),
            "{error:?}"
        );
    }
}

/// Each path-carrying variant marks its whole path as one span of the kind it knows.
#[test]
fn each_path_variant_marks_its_whole_path_by_kind() {
    let path = "Trips/Lake House/a.jpg";
    let span = |kind: LogKind| format!("{}{path}{CLOSER}", kind.opener());
    let cases = [
        (VolumeError::NotFound(path.into()), LogKind::Path),
        (VolumeError::IsADirectory(path.into()), LogKind::DirPath),
        (VolumeError::StaleDestinationHandle(path.into()), LogKind::DirPath),
        (VolumeError::ColdStorage(path.into()), LogKind::FilePath),
        (VolumeError::SourceChanged(path.into()), LogKind::FilePath),
        (
            VolumeError::FriendlyGit(FriendlyGitError::new(FriendlyGitErrorKind::IndexLocked, path)),
            LogKind::Path,
        ),
    ];
    for (error, kind) in cases {
        let logged = LogError(&error).to_string();
        assert!(logged.contains(&span(kind)), "{logged:?}");
        assert_eq!(logged.matches(CLOSER).count(), 1, "{logged:?}");
    }
}

/// An error's own words and a backend's message are foreign text: a reserved char in them
/// prints as U+FFFD, so a hostile name a server echoes can't forge a marker.
#[test]
fn a_reserved_char_in_foreign_text_is_replaced() {
    let hostile = format!("Lake{CLOSER}House");
    let error = VolumeError::IoError {
        message: format!("the server refused {hostile}").into(),
        raw_os_error: None,
    };
    assert_eq!(
        LogError(&error).to_string(),
        "I/O error: the server refused Lake\u{FFFD}House"
    );
    assert_eq!(error.to_string(), format!("I/O error: the server refused {hostile}"));
}

/// A field's reserved chars are escaped inside its span, never raw, so they can't close it.
#[test]
fn a_reserved_char_in_a_path_is_escaped_inside_its_span() {
    let error = VolumeError::NotFound(format!("Lake{CLOSER}House"));
    assert_eq!(
        LogError(&error).to_string(),
        format!("Path not found: {}Lake\\u{{fdd1}}House{CLOSER}", LogKind::Path.opener())
    );
}

/// `{:?}` quotes the logged text as one field, its spans one depth down.
#[test]
fn debug_quotes_the_marked_text_one_depth_down() {
    let error = VolumeError::NotFound("a \"b\".txt".to_string());
    assert_eq!(
        format!("{:?}", LogError(&error)),
        "\"Path not found: \\u{fdd0}a \\\\\\\"b\\\\\\\".txt\\u{fdd1}\""
    );
}

/// An empty path has no name to hash, so it writes no span.
#[test]
fn an_empty_path_writes_no_span() {
    let error = VolumeError::FriendlyGit(FriendlyGitError::new(FriendlyGitErrorKind::NotARepo, ""));
    assert_eq!(LogError(&error).to_string(), "git: NotARepo ()");
}

/// A message variant whose detail kept its cause typed marks the cause's path, and reads as
/// the cause's own words once the markers are gone.
#[test]
fn a_message_variant_marks_the_values_of_a_typed_cause() {
    use crate::error_detail::ErrorDetail;
    use crate::log_private::LogPrivate;

    let path = "Trips/Lake House/a.jpg";
    let cause = || ErrorDetail::of(VolumeError::NotFound(path.to_string()));
    let span = LogPrivate::path(path).to_string();
    for error in [
        VolumeError::IoError {
            message: cause(),
            raw_os_error: Some(66),
        },
        VolumeError::StorageFull { message: cause() },
        VolumeError::DeviceDisconnected(cause()),
        VolumeError::DeviceSessionReset(cause()),
        VolumeError::ConnectionTimeout(cause()),
        VolumeError::Cancelled(cause()),
        VolumeError::InvalidName(cause()),
        VolumeError::SignatureInvalid(cause()),
    ] {
        let logged = LogError(&error).to_string();
        assert!(logged.contains(&span), "{logged:?}");
        assert_eq!(without_reserved(&logged), error.to_string());
    }
}
