//! Explicit local move names through both engines, including conflicts and empty folders.
use super::test_support::make_state;
use super::*;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::ConflictResolution;
use crate::test_support::TestDir;

#[test]
fn named_local_moves_use_the_requested_leaf_and_preserve_skipped_originals() {
    for staged in [false, true] {
        for policy in [ConflictResolution::Skip, ConflictResolution::Overwrite] {
            let tmp = TestDir::new("named_local_move");
            let source = tmp.as_ref().join("original.txt");
            let dest = tmp.as_ref().join("dest");
            fs::create_dir(&dest).unwrap();
            fs::write(&source, b"new bytes").unwrap();
            fs::write(dest.join("renamed.txt"), b"old bytes").unwrap();
            let config = WriteOperationConfig {
                destination_name: Some("renamed.txt".into()),
                conflict_resolution: policy,
                ..Default::default()
            };
            let events = CollectorEventSink::new();
            let state = make_state(0);
            let sources = std::slice::from_ref(&source);
            if staged {
                cross_fs::move_with_staging(&events, "named-staged-move", &state, sources, &dest, &config, 0).unwrap();
            } else {
                move_files_with_progress_inner(&events, "named-rename-move", &state, sources, &dest, &config).unwrap();
            }
            let skipped = policy == ConflictResolution::Skip;
            let expected = if skipped { b"old bytes" } else { b"new bytes" };
            assert_eq!(fs::read(dest.join("renamed.txt")).unwrap(), expected);
            assert_eq!(source.exists(), skipped);
            assert!(!dest.join("original.txt").exists());
        }
    }
}

#[test]
fn named_local_folder_moves_preserve_children_and_empty_directories() {
    for staged in [false, true] {
        let tmp = TestDir::new("named_local_folder_move");
        let source = tmp.as_ref().join("original");
        fs::create_dir_all(source.join("empty/nested")).unwrap();
        fs::write(source.join("child.txt"), b"payload").unwrap();
        let dest = tmp.as_ref().join("dest");
        fs::create_dir(&dest).unwrap();
        let config = WriteOperationConfig {
            destination_name: Some("renamed".into()),
            ..Default::default()
        };
        let events = CollectorEventSink::new();
        let state = make_state(0);
        let sources = std::slice::from_ref(&source);
        if staged {
            cross_fs::move_with_staging(&events, "named-staged-folder", &state, sources, &dest, &config, 0).unwrap();
        } else {
            move_files_with_progress_inner(&events, "named-rename-folder", &state, sources, &dest, &config).unwrap();
        }
        assert_eq!(fs::read(dest.join("renamed/child.txt")).unwrap(), b"payload");
        assert!(dest.join("renamed/empty/nested").is_dir());
        assert!(!source.exists());
        assert!(!dest.join("original").exists());
    }
}
