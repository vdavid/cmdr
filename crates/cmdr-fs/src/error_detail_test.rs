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

/// The JSON is a plain string both ways, so a stored or IPC detail round-trips as text.
#[test]
fn a_detail_round_trips_through_json_as_a_string() {
    let detail: ErrorDetail = serde_json::from_str("\"disk on fire\"").expect("deserializes");
    assert_eq!(detail.to_string(), "disk on fire");
    assert_eq!(serde_json::to_string(&detail).expect("serializes"), "\"disk on fire\"");
}
