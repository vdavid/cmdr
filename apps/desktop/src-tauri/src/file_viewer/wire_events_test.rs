//! The app's side of the viewer's wire: the Tauri events a viewer window hears, pinned as
//! JSON, and the viewer's own facts crossing exactly alike (a browser host serializes the
//! facts themselves; the crate pins those in `crates/cmdr-viewer/src/wire_characterization_test.rs`).

use super::app_host::file_changed_event;
use super::events::{ViewerMediaLoad, ViewerPullProgress};
use super::host::events::FileChange;
use super::{MediaLoad, MediaLoadStage, PullProgress};

#[test]
fn media_load_events_cross_as_their_facts_do() {
    for stage in [MediaLoadStage::Reading, MediaLoadStage::Done, MediaLoadStage::Failed] {
        let fact = MediaLoad {
            token: "t".into(),
            stage,
            bytes_done: 4,
            bytes_total: 9,
        };
        let event = serde_json::to_string(&ViewerMediaLoad::from(fact.clone())).expect("serializes");
        assert_eq!(event, serde_json::to_string(&fact).expect("serializes"));
    }
    let done = ViewerMediaLoad::from(MediaLoad {
        token: "t".into(),
        stage: MediaLoadStage::Done,
        bytes_done: 4,
        bytes_total: 9,
    });
    assert_eq!(
        serde_json::to_string(&done).expect("serializes"),
        r#"{"token":"t","stage":"done","bytesDone":4,"bytesTotal":9}"#
    );
}

#[test]
fn pull_progress_events_cross_as_their_facts_do() {
    for fact in [
        PullProgress {
            bytes_done: 1,
            bytes_total: Some(2),
        },
        PullProgress {
            bytes_done: 3,
            bytes_total: None,
        },
    ] {
        assert_eq!(
            serde_json::to_string(&ViewerPullProgress::from(fact)).expect("serializes"),
            serde_json::to_string(&fact).expect("serializes")
        );
    }
    assert_eq!(
        serde_json::to_string(&ViewerPullProgress::from(PullProgress {
            bytes_done: 3,
            bytes_total: None,
        }))
        .expect("serializes"),
        r#"{"bytesDone":3,"bytesTotal":null}"#
    );
}

#[test]
fn file_changed_events_cross_in_this_shape() {
    let (grew_name, grew) = file_changed_event("sid-1", FileChange::Grew { new_size: 42 });
    let (rotated_name, rotated) = file_changed_event("sid-1", FileChange::Rotated);
    assert_eq!(grew_name, "viewer:file-changed:sid-1");
    assert_eq!(rotated_name, "viewer:file-changed:sid-1");
    assert_eq!(grew.to_string(), r#"{"kind":"grew","newSize":42}"#);
    assert_eq!(rotated.to_string(), r#"{"kind":"rotated","newSize":null}"#);
}
