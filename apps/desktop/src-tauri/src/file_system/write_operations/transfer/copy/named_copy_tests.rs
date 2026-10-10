//! Named local copies: landing, conflicts, source preservation, and input boundaries.

use super::*;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;

fn make_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(200)))
}

#[test]
fn named_copy_lands_beside_source_without_changing_it() {
    let tmp = crate::test_support::TestDir::new("named_copy");
    let source = tmp.as_ref().join("original.txt");
    fs::write(&source, b"keep the original").unwrap();
    let events = CollectorEventSink::new();
    let config = WriteOperationConfig {
        destination_name: Some("backup.txt".to_string()),
        ..Default::default()
    };
    copy_files_with_progress_inner(
        &events,
        "named-copy",
        &make_state(),
        std::slice::from_ref(&source),
        tmp.as_ref(),
        &config,
    )
    .unwrap();
    assert_eq!(fs::read(&source).unwrap(), b"keep the original");
    assert_eq!(fs::read(tmp.as_ref().join("backup.txt")).unwrap(), b"keep the original");
    assert!(!tmp.as_ref().join("original (1).txt").exists());
}

#[test]
fn named_folder_copy_maps_children_and_empty_directories() {
    let tmp = crate::test_support::TestDir::new("named_folder_copy");
    let source = tmp.as_ref().join("original");
    fs::create_dir_all(source.join("empty/nested")).unwrap();
    fs::write(source.join("notes.txt"), b"notes").unwrap();
    let config = WriteOperationConfig {
        destination_name: Some("backup".to_string()),
        ..Default::default()
    };
    copy_files_with_progress_inner(
        &CollectorEventSink::new(),
        "named-folder-copy",
        &make_state(),
        std::slice::from_ref(&source),
        tmp.as_ref(),
        &config,
    )
    .unwrap();
    assert_eq!(fs::read(tmp.as_ref().join("backup/notes.txt")).unwrap(), b"notes");
    assert!(tmp.as_ref().join("backup/empty/nested").is_dir());
    assert!(source.join("empty/nested").is_dir());
}

#[test]
fn named_copy_honors_skip_and_overwrite_at_the_requested_name() {
    for resolution in [ConflictResolution::Skip, ConflictResolution::Overwrite] {
        let tmp = crate::test_support::TestDir::new("named_copy_conflict");
        let source = tmp.as_ref().join("original.txt");
        let target = tmp.as_ref().join("backup.txt");
        fs::write(&source, b"source").unwrap();
        fs::write(&target, b"existing").unwrap();
        let config = WriteOperationConfig {
            destination_name: Some("backup.txt".to_string()),
            conflict_resolution: resolution,
            ..Default::default()
        };
        copy_files_with_progress_inner(
            &CollectorEventSink::new(),
            "named-copy-conflict",
            &make_state(),
            std::slice::from_ref(&source),
            tmp.as_ref(),
            &config,
        )
        .unwrap();
        let expected: &[u8] = if resolution == ConflictResolution::Skip {
            b"existing"
        } else {
            b"source"
        };
        assert_eq!(fs::read(&target).unwrap(), expected);
        assert_eq!(fs::read(&source).unwrap(), b"source");
    }
}

#[test]
fn named_copy_of_itself_duplicates_instead_of_overwriting() {
    let tmp = crate::test_support::TestDir::new("named_copy_self");
    let source = tmp.as_ref().join("original.txt");
    fs::write(&source, b"source").unwrap();
    let config = WriteOperationConfig {
        destination_name: Some("original.txt".to_string()),
        conflict_resolution: ConflictResolution::Overwrite,
        ..Default::default()
    };
    copy_files_with_progress_inner(
        &CollectorEventSink::new(),
        "named-copy-self",
        &make_state(),
        std::slice::from_ref(&source),
        tmp.as_ref(),
        &config,
    )
    .unwrap();
    assert_eq!(fs::read(&source).unwrap(), b"source");
    assert_eq!(fs::read(tmp.as_ref().join("original (1).txt")).unwrap(), b"source");
}

#[test]
fn named_copy_rejects_path_components_and_batches() {
    let tmp = crate::test_support::TestDir::new("named_copy_invalid");
    let source = tmp.as_ref().join("original.txt");
    fs::write(&source, b"source").unwrap();
    for name in ["", ".", "..", "../escape.txt", "/absolute", "bad\0name"] {
        let config = WriteOperationConfig {
            destination_name: Some(name.to_string()),
            ..Default::default()
        };
        let result = copy_files_with_progress_inner(
            &CollectorEventSink::new(),
            "named-copy-invalid",
            &make_state(),
            std::slice::from_ref(&source),
            tmp.as_ref(),
            &config,
        );
        assert!(matches!(result, Err(WriteOperationError::InvalidName { .. })));
    }
    let other = tmp.as_ref().join("other.txt");
    fs::write(&other, b"other").unwrap();
    let config = WriteOperationConfig {
        destination_name: Some("backup.txt".to_string()),
        ..Default::default()
    };
    let result = copy_files_with_progress_inner(
        &CollectorEventSink::new(),
        "named-copy-batch",
        &make_state(),
        &[source, other],
        tmp.as_ref(),
        &config,
    );
    assert!(matches!(result, Err(WriteOperationError::InvalidName { .. })));
    assert!(!tmp.as_ref().join("backup.txt").exists());
}
