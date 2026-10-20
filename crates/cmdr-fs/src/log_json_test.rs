use serde::Serialize;

use super::*;
use crate::error_detail::ErrorDetail;
use crate::log_private::{LogKind, LogPrivate, RESERVED};
use crate::volume::VolumeError;

/// A share-relative path with a quote and a backslash in it: no regex claims it, and both
/// chars escape differently in JSON and in a span body unless the writer keeps them apart.
const PATH: &str = "Trips/Lake \"House\" \\ 2026/IMG 0001.jpg";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Refusal {
    r#type: &'static str,
    path: String,
    volume_name: String,
    message: ErrorDetail,
    hint: String,
    count: u32,
}

fn refusal(hint: &str) -> Refusal {
    Refusal {
        r#type: "read_error",
        path: PATH.to_string(),
        volume_name: "Family Photos".to_string(),
        message: ErrorDetail::of(VolumeError::NotFound(PATH.to_string())),
        hint: hint.to_string(),
        count: 3,
    }
}

fn kinds(key: &str) -> JsonField {
    match key {
        "path" => JsonField::Mark(LogKind::Path),
        "volumeName" => JsonField::Mark(LogKind::Volume),
        "message" => JsonField::Detail,
        _ => JsonField::Words,
    }
}

fn without_reserved(text: &str) -> String {
    text.chars().filter(|c| !RESERVED.contains(c)).collect()
}

/// Each field the caller names is one span of its kind, its body the value's `{:?}` body
/// verbatim (not JSON-escaped a second time), and a detail's typed cause marks its own path.
#[test]
fn each_named_field_is_one_span_of_its_kind() {
    let logged = log_json(&refusal("try again"), kinds);
    let text = logged.as_marked_text();
    let path_span = LogPrivate::path(PATH).to_string();
    assert!(text.contains(&format!("\"path\":\"{path_span}\"")), "{text}");
    assert!(
        text.contains(&format!("\"volumeName\":\"{}\"", LogPrivate::volume("Family Photos"))),
        "{text}"
    );
    assert!(
        text.contains(&format!("\"message\":\"Path not found: {path_span}\"")),
        "{text}"
    );
}

/// Stripped of its markers, the log JSON reads back as the very JSON the frontend gets.
#[test]
fn the_logged_json_is_the_ipc_json_once_its_markers_are_gone() {
    let value = refusal("try again");
    let plain = serde_json::to_string(&value).expect("serializes");
    assert!(!plain.contains(|c| RESERVED.contains(&c)), "{plain}");
    let logged = without_reserved(log_json(&value, kinds).as_marked_text());
    let back: Value = serde_json::from_str(&logged).expect("still JSON");
    assert_eq!(back, serde_json::from_str::<Value>(&plain).expect("JSON"));
}

/// A reserved char in an unmarked field is someone else's text: it prints as U+FFFD, so it
/// can't forge a span.
#[test]
fn a_reserved_char_in_an_unmarked_field_is_replaced() {
    let forged = format!("{}x{}", LogKind::Name.opener(), CLOSER);
    let logged = log_json(&refusal(&forged), kinds);
    assert!(
        logged.as_marked_text().contains("\"hint\":\"\u{FFFD}x\u{FFFD}\""),
        "{}",
        logged.as_marked_text()
    );
}
