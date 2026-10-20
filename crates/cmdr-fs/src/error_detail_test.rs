use super::*;
use crate::log_error::LogError;
use crate::log_private::{LogPrivate, RESERVED};
use crate::volume::VolumeError;

/// A share-relative path: no leading `/`, so no regex claims it in prose.
const PATH: &str = "Trips/Summer (Lake House) 2026/IMG 0001.jpg";

/// A detail built from a typed error marks that error's path in a log line, and reads as the
/// error's own `Display` everywhere a person, a store, or IPC sees it.
#[test]
fn a_detail_from_a_typed_error_marks_its_path_and_reads_as_its_display() {
    let cause = VolumeError::NotFound(PATH.to_string());
    let detail = ErrorDetail::of(cause.clone());

    let span = LogPrivate::path(PATH).to_string();
    let logged = LogError(&detail).to_string();
    assert!(logged.contains(&span), "{logged:?}");
    assert_eq!(logged.replace(&span, PATH), cause.to_string());

    assert_eq!(detail.to_string(), cause.to_string());
    assert_eq!(format!("{detail:?}"), format!("{:?}", cause.to_string()));
    assert_eq!(
        serde_json::to_string(&detail).expect("serializes"),
        serde_json::to_string(&cause.to_string()).expect("serializes")
    );
}

/// A detail that is only text stays text: it's foreign words, so a reserved char in it can't
/// forge a marker in a log line.
#[test]
fn a_text_detail_logs_as_foreign_text() {
    let forged = format!("{}{}", RESERVED.start(), RESERVED.end());
    let detail = ErrorDetail::from(format!("the server said no {forged}"));
    let logged = LogError(&detail).to_string();
    assert!(!logged.contains(|c| RESERVED.contains(&c)), "{logged:?}");
    assert_eq!(detail.to_string(), format!("the server said no {forged}"));
}

/// A backend's own words about a path it names: the words stay text, the path is marked.
#[test]
fn a_detail_about_a_path_marks_the_path() {
    let detail = ErrorDetail::at_path("ENOSPC on ", PATH);
    let span = LogPrivate::path(PATH).to_string();
    let logged = LogError(&detail).to_string();
    assert_eq!(logged, format!("ENOSPC on {span}"));
    assert_eq!(detail.to_string(), format!("ENOSPC on {PATH}"));
}

/// The same with the path first (`"<key>: the server ended the upload"`).
#[test]
fn a_detail_leading_with_a_path_marks_the_path() {
    let detail = ErrorDetail::path_then(PATH, ": the server ended the upload");
    let span = LogPrivate::path(PATH).to_string();
    assert_eq!(
        LogError(&detail).to_string(),
        format!("{span}: the server ended the upload")
    );
    assert_eq!(detail.to_string(), format!("{PATH}: the server ended the upload"));
}

/// An `io::Error` that carries a `VolumeError` (a backend's byte stream handing one to a
/// reader that only speaks `io`) keeps it typed; any other one is the OS's words.
#[test]
fn a_detail_from_an_io_error_keeps_a_carried_volume_error_typed() {
    let cause = VolumeError::NotFound(PATH.to_string());
    let detail = VolumeError::io_detail(&std::io::Error::other(cause.clone()));
    let span = LogPrivate::path(PATH).to_string();
    assert!(LogError(&detail).to_string().contains(&span));
    assert_eq!(detail.to_string(), cause.to_string());

    let os = std::io::Error::from_raw_os_error(28);
    assert_eq!(VolumeError::io_detail(&os).to_string(), os.to_string());
}

/// The JSON is a plain string both ways, so a stored or IPC detail round-trips as text.
#[test]
fn a_detail_round_trips_through_json_as_a_string() {
    let detail: ErrorDetail = serde_json::from_str("\"disk on fire\"").expect("deserializes");
    assert_eq!(detail.to_string(), "disk on fire");
    assert_eq!(serde_json::to_string(&detail).expect("serializes"), "\"disk on fire\"");
}
