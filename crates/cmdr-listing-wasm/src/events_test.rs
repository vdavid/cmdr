//! The drift guard: every event this crate sends carries exactly the keys the
//! generated `bindings.ts` declares for it. The wire structs mirror the app's
//! (`listing_host/events.rs`), so a field added there fails here until the
//! mirror catches up.

use std::collections::BTreeSet;

use cmdr_listing::{SortColumn, StalledOn};
use serde::Serialize;

use crate::events::{
    DirectoryDiff, ListingCompleteEvent, ListingErrorEvent, ListingIdEvent, ListingPathEvent, ListingProgressEvent,
    ListingReadCompleteEvent, ListingStalledEvent,
};

/// Read at test time rather than embedded: the compiling lanes fingerprint it
/// (`rustEmbeddedInputs` in `scripts/check/checks/inputs.go`), and the source
/// scanners, which never run this, needn't.
static BINDINGS: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/desktop/src/lib/ipc/bindings.ts"
    );
    std::fs::read_to_string(path).expect("the generated bindings are readable")
});

/// The TS type `makeEvent<T>('event')` names for `event`.
fn ts_type_of_event(event: &str) -> &'static str {
    let needle = format!(">('{event}')");
    let line = BINDINGS
        .lines()
        .find(|l| l.contains("makeEvent<") && l.contains(&needle))
        .unwrap_or_else(|| panic!("`bindings.ts` has no `{event}` event"));
    let start = line.find("makeEvent<").expect("makeEvent") + "makeEvent<".len();
    let end = line[start..].find('>').expect("closing bracket") + start;
    &line[start..end]
}

/// The top-level keys of `export type <name> = { ... }`.
fn ts_keys(name: &str) -> BTreeSet<String> {
    let header = format!("export type {name} = {{");
    let mut lines = BINDINGS.lines().skip_while(|l| *l != header);
    assert!(lines.next().is_some(), "`bindings.ts` has no `{name}` object type");
    lines
        .take_while(|l| *l != "}")
        .filter_map(|l| {
            let key = l.strip_prefix("  ")?;
            let (key, _) = key.split_once(':')?;
            let key = key.trim_end_matches('?');
            key.chars().all(|c| c.is_ascii_alphanumeric()).then(|| key.to_string())
        })
        .collect()
}

fn keys_of(payload: impl Serialize) -> BTreeSet<String> {
    match serde_json::to_value(payload).expect("serializes") {
        serde_json::Value::Object(map) => map.keys().cloned().collect(),
        other => panic!("not an object: {other}"),
    }
}

fn assert_matches(event: &str, payload: impl Serialize) {
    let ts = ts_type_of_event(event);
    assert_eq!(
        keys_of(payload),
        ts_keys(ts),
        "`{event}` drifted from `{ts}` in bindings.ts"
    );
}

#[test]
fn every_event_matches_the_generated_bindings() {
    let listing_id = "pane-1";
    for event in ["listing-opening", "listing-cancelled"] {
        assert_matches(event, ListingIdEvent { listing_id });
    }
    let progress = ListingProgressEvent {
        listing_id,
        loaded_count: 1,
        phase: None,
    };
    assert_matches("listing-progress", progress);
    let stalled = ListingStalledEvent {
        listing_id,
        stalled_on: StalledOn::Unknown,
    };
    assert_matches("listing-stalled", stalled);
    let read = ListingReadCompleteEvent {
        listing_id,
        total_count: 1,
    };
    assert_matches("listing-read-complete", read);
    let complete = ListingCompleteEvent {
        listing_id,
        total_count: 1,
        volume_root: "/".into(),
        stored_path: None,
        effective_sort_by: SortColumn::Name,
    };
    assert_matches("listing-complete", complete);
    let error = ListingErrorEvent {
        listing_id,
        message: String::new(),
        error: None,
    };
    assert_matches("listing-error", error);
    let diff = DirectoryDiff {
        listing_id,
        batches: Vec::new(),
    };
    assert_matches("directory-diff", diff);
    // The viewer's media progress: the core's fact goes out as is (`viewer.rs`).
    let load = cmdr_viewer::MediaLoad {
        token: String::new(),
        stage: cmdr_viewer::MediaLoadStage::Done,
        bytes_done: 0,
        bytes_total: 0,
    };
    assert_matches(crate::viewer::VIEWER_MEDIA_LOAD, load);
    for event in ["listing-respelled", "directory-deleted"] {
        assert_matches(
            event,
            ListingPathEvent {
                listing_id,
                path: "/".into(),
            },
        );
    }
}
