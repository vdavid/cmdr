//! Explicit names share the serial landing and source-sweep paths of both volume moves.
use super::test_support::{make_state, make_volumes};
use super::*;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::ConflictResolution;

#[tokio::test]
async fn named_volume_moves_honor_skip_and_overwrite_at_the_requested_leaf() {
    for same_volume in [false, true] {
        for policy in [ConflictResolution::Skip, ConflictResolution::Overwrite] {
            let (source, other) = make_volumes();
            let dest = if same_volume { Arc::clone(&source) } else { other };
            source
                .create_file(Path::new("/original.txt"), b"new bytes")
                .await
                .unwrap();
            dest.create_directory(Path::new("/dest")).await.unwrap();
            dest.create_file(Path::new("/dest/renamed.txt"), b"old bytes")
                .await
                .unwrap();
            let config = VolumeCopyConfig {
                destination_name: Some("renamed.txt".into()),
                conflict_resolution: policy,
                ..Default::default()
            };
            let events = Arc::new(CollectorEventSink::new());
            let state = make_state();
            let sources = [PathBuf::from("/original.txt")];
            if same_volume {
                super::super::move_same::move_within_same_volume_with_progress(
                    events,
                    "named-same-move",
                    &state,
                    Arc::clone(&source),
                    &sources,
                    Path::new("/dest"),
                    &config,
                )
                .await
                .unwrap();
            } else {
                move_volumes_with_progress(
                    events,
                    "named-cross-move",
                    &state,
                    Arc::clone(&source),
                    &sources,
                    Arc::clone(&dest),
                    Path::new("/dest"),
                    &config,
                )
                .await
                .unwrap();
            }
            let mut reader = dest.open_read_stream(Path::new("/dest/renamed.txt")).await.unwrap();
            let skipped = policy == ConflictResolution::Skip;
            let expected = if skipped { b"old bytes" } else { b"new bytes" };
            assert_eq!(reader.next_chunk().await.unwrap().unwrap(), expected);
            assert_eq!(source.exists(Path::new("/original.txt")).await, skipped);
            assert!(!dest.exists(Path::new("/dest/original.txt")).await);
        }
    }
}

#[tokio::test]
async fn named_volume_folder_moves_keep_empty_directories() {
    for same_volume in [false, true] {
        let (source, other) = make_volumes();
        let dest = if same_volume { Arc::clone(&source) } else { other };
        source
            .create_directory_all(Path::new("/original/empty/nested"))
            .await
            .unwrap();
        source
            .create_file(Path::new("/original/child.txt"), b"payload")
            .await
            .unwrap();
        let config = VolumeCopyConfig {
            destination_name: Some("renamed".into()),
            ..Default::default()
        };
        let events = Arc::new(CollectorEventSink::new());
        let state = make_state();
        let sources = [PathBuf::from("/original")];
        if same_volume {
            super::super::move_same::move_within_same_volume_with_progress(
                events,
                "named-same-folder",
                &state,
                Arc::clone(&source),
                &sources,
                Path::new("/dest"),
                &config,
            )
            .await
            .unwrap();
        } else {
            move_volumes_with_progress(
                events,
                "named-cross-folder",
                &state,
                Arc::clone(&source),
                &sources,
                Arc::clone(&dest),
                Path::new("/dest"),
                &config,
            )
            .await
            .unwrap();
        }
        assert!(dest.exists(Path::new("/dest/renamed/child.txt")).await);
        assert!(
            dest.is_directory(Path::new("/dest/renamed/empty/nested"))
                .await
                .unwrap()
        );
        assert!(!source.exists(Path::new("/original")).await);
        assert!(!dest.exists(Path::new("/dest/original")).await);
    }
}
