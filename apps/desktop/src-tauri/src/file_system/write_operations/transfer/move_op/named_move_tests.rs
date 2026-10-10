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

/// Whether `dir` folds case, as APFS and HFS+ do by default. A case-sensitive
/// filesystem (Linux CI) holds both names, so there's no self-alias to test.
fn folds_case(dir: &Path) -> bool {
    let probe = dir.join("case-probe");
    fs::write(&probe, b"").unwrap();
    let folds = dir.join("CASE-PROBE").exists();
    fs::remove_file(&probe).unwrap();
    folds
}

/// A Move that only changes the name's case lands on its own entry on a
/// case-folding filesystem. Identity must not swallow it as "already there".
#[test]
fn a_case_only_named_move_renames_the_item_in_place() {
    for (source_name, new_name, is_dir) in [("notes.txt", "Notes.txt", false), ("photos", "Photos", true)] {
        let tmp = TestDir::new("case_only_named_move");
        if !folds_case(tmp.as_ref()) {
            return;
        }
        let source = tmp.as_ref().join(source_name);
        if is_dir {
            fs::create_dir(&source).unwrap();
            fs::write(source.join("child.txt"), b"payload").unwrap();
        } else {
            fs::write(&source, b"payload").unwrap();
        }
        let config = WriteOperationConfig {
            destination_name: Some(new_name.into()),
            ..Default::default()
        };
        let events = CollectorEventSink::new();
        let state = make_state(0);
        move_files_with_progress_inner(
            &events,
            "case-only-move",
            &state,
            std::slice::from_ref(&source),
            tmp.as_ref(),
            &config,
        )
        .unwrap();
        let names: Vec<String> = fs::read_dir(tmp.as_ref())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec![new_name.to_string()]);
        if is_dir {
            assert_eq!(
                fs::read(tmp.as_ref().join(new_name).join("child.txt")).unwrap(),
                b"payload"
            );
        } else {
            assert_eq!(fs::read(tmp.as_ref().join(new_name)).unwrap(), b"payload");
        }
    }
}
