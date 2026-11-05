//! The JSON every Multi-Rename answer, refusal, and event puts on the wire (serde, not
//! specta). The sheet reads exactly these shapes, and any second host must send them
//! too, so a moved type or a changed `serde` attribute shows up here first.

use serde::Serialize;
use serde_json::{Value, json};

use cmdr_media_meta::MediaField;
use cmdr_operations::RenameStartError;

use super::error::{MultiRenameError, ReadOnlyPlace};
use super::history::{FieldHistoryEntry, HistoryField};
use super::mask::{FieldGap, MaskError};
use super::media::{
    FieldSample, FieldSamples, FieldUsage, MediaFieldOverview, MediaOverview, MediaProgress, MultiRenameMediaProgress,
};
use super::plan::{InvalidNameReason, MultiRenameSpec, PreviewRow, RowStatus, SpecError};
use super::presets::{LastSpec, MultiRenameLastSettings, MultiRenamePreset, PresetSaveRefusal};
use super::run_test::strip_diacritics;
use super::session::{MultiRenameOpened, MultiRenamePreview, PreviewCounts};

fn wire(value: impl Serialize) -> Value {
    serde_json::to_value(value).expect("serializes")
}

fn spec() -> MultiRenameSpec {
    MultiRenameSpec {
        name_mask: "[N]_[C:2]".to_string(),
        ..strip_diacritics()
    }
}

fn spec_json() -> Value {
    json!({
        "nameMask": "[N]_[C:2]",
        "extensionMask": "[E]",
        "search": "",
        "replace": "",
        "caseSensitive": false,
        "firstOnly": false,
        "includeExtension": false,
        "regex": false,
        "substitute": false,
        "case": "unchanged",
        "removeDiacritics": true,
        "greekToLatin": false,
        "normalizeUnicode": false,
    })
}

fn row(status: RowStatus) -> PreviewRow {
    PreviewRow {
        row: 3,
        old_name: "a.jpg".to_string(),
        new_name: "b.jpg".to_string(),
        status,
        icon_id: Some("ext:jpg".to_string()),
        is_directory: false,
        edited: true,
    }
}

#[test]
fn an_opened_session() {
    let opened = MultiRenameOpened {
        session_id: "s1".to_string(),
        count: 2,
    };
    assert_eq!(wire(opened), json!({ "sessionId": "s1", "count": 2 }));
}

#[test]
fn a_preview_and_its_row() {
    let preview = MultiRenamePreview {
        preview_id: 7,
        counts: PreviewCounts {
            ready: 1,
            unchanged: 2,
            problems: 3,
            reading: 4,
        },
        rows: vec![row(RowStatus::Ready)],
    };
    assert_eq!(
        wire(preview),
        json!({
            "previewId": 7,
            "counts": { "ready": 1, "unchanged": 2, "problems": 3, "reading": 4 },
            "rows": [{
                "row": 3,
                "oldName": "a.jpg",
                "newName": "b.jpg",
                "status": { "type": "ready" },
                "iconId": "ext:jpg",
                "isDirectory": false,
                "edited": true,
            }],
        })
    );
    let gone = PreviewRow {
        icon_id: None,
        ..row(RowStatus::Missing)
    };
    assert_eq!(wire(gone)["iconId"], Value::Null);
}

#[test]
fn every_row_status() {
    let field = MediaField::ExifTaken;
    let statuses = [
        (RowStatus::Ready, json!({ "type": "ready" })),
        (RowStatus::Unchanged, json!({ "type": "unchanged" })),
        (
            RowStatus::InvalidName {
                reason: InvalidNameReason::Empty,
            },
            json!({ "type": "invalidName", "reason": { "type": "empty" } }),
        ),
        (
            RowStatus::InvalidName {
                reason: InvalidNameReason::DisallowedCharacter {
                    character: "/".to_string(),
                },
            },
            json!({ "type": "invalidName", "reason": { "type": "disallowedCharacter", "character": "/" } }),
        ),
        (
            RowStatus::InvalidName {
                reason: InvalidNameReason::TooLong,
            },
            json!({ "type": "invalidName", "reason": { "type": "tooLong" } }),
        ),
        (
            RowStatus::InvalidName {
                reason: InvalidNameReason::Reserved,
            },
            json!({ "type": "invalidName", "reason": { "type": "reserved" } }),
        ),
        (RowStatus::Duplicate, json!({ "type": "duplicate" })),
        (RowStatus::TargetExists, json!({ "type": "targetExists" })),
        (RowStatus::Missing, json!({ "type": "missing" })),
        (RowStatus::Reading, json!({ "type": "reading" })),
        (
            RowStatus::NoValue { field },
            json!({ "type": "noValue", "field": "exifTaken" }),
        ),
        (
            RowStatus::Unreadable { field },
            json!({ "type": "unreadable", "field": "exifTaken" }),
        ),
    ];
    for (status, expected) in statuses {
        assert_eq!(wire(row(status))["status"], expected);
    }
}

#[test]
fn every_error() {
    let errors = [
        (
            MultiRenameError::Gone {
                listing_id: "l1".to_string(),
            },
            json!({ "type": "gone", "listingId": "l1" }),
        ),
        (
            MultiRenameError::SelectionChanged {
                listing_id: "l1".to_string(),
            },
            json!({ "type": "selectionChanged", "listingId": "l1" }),
        ),
        (MultiRenameError::SessionClosed, json!({ "type": "sessionClosed" })),
        (
            MultiRenameError::Spec {
                error: SpecError::NameMask {
                    error: MaskError::Unclosed { at: 4 },
                },
            },
            json!({ "type": "spec", "error": { "type": "nameMask", "error": { "type": "unclosed", "at": 4 } } }),
        ),
        (
            MultiRenameError::Spec {
                error: SpecError::ExtensionMask {
                    error: MaskError::Unknown {
                        placeholder: "Q".to_string(),
                    },
                },
            },
            json!({
                "type": "spec",
                "error": { "type": "extensionMask", "error": { "type": "unknown", "placeholder": "Q" } },
            }),
        ),
        (
            MultiRenameError::Spec {
                error: SpecError::BadRegex {
                    detail: "unclosed group".to_string(),
                },
            },
            json!({ "type": "spec", "error": { "type": "badRegex", "detail": "unclosed group" } }),
        ),
        (MultiRenameError::NothingToRename, json!({ "type": "nothingToRename" })),
        (
            MultiRenameError::NotConnected {
                volume_id: "v1".to_string(),
            },
            json!({ "type": "notConnected", "volumeId": "v1" }),
        ),
        (
            MultiRenameError::CouldntStart {
                reason: RenameStartError::NotInOneFolder,
            },
            json!({ "type": "couldntStart", "reason": { "type": "notInOneFolder" } }),
        ),
        (
            MultiRenameError::PreviewOutOfDate,
            json!({ "type": "previewOutOfDate" }),
        ),
        (MultiRenameError::StillReading, json!({ "type": "stillReading" })),
        (
            MultiRenameError::ReadOnly {
                place: ReadOnlyPlace::Archive,
            },
            json!({ "type": "readOnly", "place": "archive" }),
        ),
        (
            MultiRenameError::ReadOnly {
                place: ReadOnlyPlace::GitPortal,
            },
            json!({ "type": "readOnly", "place": "gitPortal" }),
        ),
        (
            MultiRenameError::ReadOnly {
                place: ReadOnlyPlace::ResticSnapshots,
            },
            json!({ "type": "readOnly", "place": "resticSnapshots" }),
        ),
        (
            MultiRenameError::CouldntWriteNames {
                detail: "d".to_string(),
            },
            json!({ "type": "couldntWriteNames", "detail": "d" }),
        ),
        (
            MultiRenameError::NamesFileGone {
                detail: "d".to_string(),
            },
            json!({ "type": "namesFileGone", "detail": "d" }),
        ),
        (MultiRenameError::TimedOut, json!({ "type": "timedOut" })),
        (
            MultiRenameError::Internal {
                detail: "d".to_string(),
            },
            json!({ "type": "internal", "detail": "d" }),
        ),
    ];
    for (error, expected) in errors {
        assert_eq!(wire(&error), expected, "{error:?}");
    }
}

#[test]
fn the_media_progress_event_and_answers() {
    let event = MultiRenameMediaProgress {
        session_id: "s1".to_string(),
        read: 3,
        total: 5,
        done: false,
    };
    assert_eq!(
        wire(event),
        json!({ "sessionId": "s1", "read": 3, "total": 5, "done": false })
    );

    let overview = MediaOverview {
        progress: MediaProgress {
            read: 5,
            total: 5,
            done: true,
        },
        fields: vec![MediaFieldOverview {
            field: MediaField::TagTitle,
            example: Some("Harder".to_string()),
            files_with_it: 2,
        }],
    };
    assert_eq!(
        wire(overview),
        json!({
            "progress": { "read": 5, "total": 5, "done": true },
            "fields": [{ "field": "tagTitle", "example": "Harder", "filesWithIt": 2 }],
        })
    );

    let samples = FieldSamples {
        samples: vec![
            FieldSample {
                row: 0,
                text: Some("2024-05-06".to_string()),
                gap: None,
            },
            FieldSample {
                row: 1,
                text: None,
                gap: Some(FieldGap::Reading),
            },
            FieldSample {
                row: 2,
                text: None,
                gap: Some(FieldGap::NoValue {
                    field: MediaField::ExifTaken,
                }),
            },
            FieldSample {
                row: 3,
                text: None,
                gap: Some(FieldGap::Unreadable {
                    field: MediaField::ExifTaken,
                }),
            },
        ],
        usage: FieldUsage {
            with_value: 1,
            fallback: 2,
            missing: 3,
            reading: 4,
        },
    };
    assert_eq!(
        wire(samples),
        json!({
            "samples": [
                { "row": 0, "text": "2024-05-06", "gap": null },
                { "row": 1, "text": null, "gap": { "type": "reading" } },
                { "row": 2, "text": null, "gap": { "type": "noValue", "field": "exifTaken" } },
                { "row": 3, "text": null, "gap": { "type": "unreadable", "field": "exifTaken" } },
            ],
            "usage": { "withValue": 1, "fallback": 2, "missing": 3, "reading": 4 },
        })
    );
}

#[test]
fn presets_last_settings_and_history() {
    let preset = MultiRenamePreset {
        id: "p1".to_string(),
        name: "Trip".to_string(),
        spec: spec(),
    };
    assert_eq!(wire(preset), json!({ "id": "p1", "name": "Trip", "spec": spec_json() }));
    assert_eq!(
        wire(PresetSaveRefusal::ListFull { max: 200 }),
        json!({ "type": "listFull", "max": 200 })
    );

    let last = MultiRenameLastSettings {
        spec: spec(),
        preset_id: Some("p1".to_string()),
    };
    assert_eq!(wire(&last), json!({ "spec": spec_json(), "presetId": "p1" }));
    // The one entry `-last.json` keeps: the settings flattened beside its id.
    assert_eq!(
        wire(LastSpec::new(spec(), None)),
        json!({ "id": "last", "spec": spec_json(), "presetId": null })
    );

    let fields = [
        (HistoryField::NameMask, "nameMask"),
        (HistoryField::ExtensionMask, "extensionMask"),
        (HistoryField::Search, "search"),
        (HistoryField::Replace, "replace"),
    ];
    for (field, name) in fields {
        let entry = FieldHistoryEntry {
            id: "h1".to_string(),
            field,
            value: "[N]".to_string(),
        };
        assert_eq!(wire(entry), json!({ "id": "h1", "field": name, "value": "[N]" }));
    }
}
