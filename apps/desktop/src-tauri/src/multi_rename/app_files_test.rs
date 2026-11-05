//! The app's Multi-Rename files: the last settings keep one entry across a restart, and
//! the Results file lives where "Delete all Cmdr data" reaches it.

use super::app_files::results_path;
use super::plan::MultiRenameSpec;
use super::presets::LastSpec;
use super::run_test::strip_diacritics;
use crate::recents::RecentsFile;

fn spec(name_mask: &str) -> MultiRenameSpec {
    MultiRenameSpec {
        name_mask: name_mask.to_string(),
        ..strip_diacritics()
    }
}

#[test]
fn the_last_settings_keep_one_entry_and_survive_a_restart() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("multi-rename-last.json");

    let writer: RecentsFile<LastSpec> = RecentsFile::new();
    writer.add_at(Some(&path), LastSpec::new(spec("[N]-one"), None), 1);
    let loaded = Some("p1".to_string());
    writer.add_at(Some(&path), LastSpec::new(spec("[N]-two"), loaded.clone()), 1);

    let reader: RecentsFile<LastSpec> = RecentsFile::new();
    reader.load_at(&path);
    let entries = reader.entries(None);
    assert_eq!(entries.len(), 1, "saving replaces the one entry");
    assert_eq!(entries[0].settings.spec, spec("[N]-two"));
    assert_eq!(entries[0].settings.preset_id, loaded);
}

/// The Results file lives in the app's cache dir, which "Delete all Cmdr data" removes whole
/// (`data_wipe::plan`): moving it out would let it escape the wipe.
#[test]
fn the_results_file_goes_with_the_cache_dir() {
    let cache = crate::config::app_cache_dir().expect("a test has a cache dir");
    let path = results_path("multi-rename-1").expect("a test has a cache dir");
    assert!(
        path.starts_with(&cache),
        "{} is outside {}",
        path.display(),
        cache.display()
    );
}
