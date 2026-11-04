//! Fixture helpers the session test files share.

use std::fs;
use std::path::{Path, PathBuf};

use crate::test_support::TestDir;

/// The viewer the app's tests open sessions on: the app's own, over the app's host.
pub(crate) fn viewer() -> &'static super::Viewer {
    super::app_viewer()
}

pub(super) fn create_test_dir(name: &str) -> TestDir {
    TestDir::new(&format!("viewer_session_{}", name))
}

pub(super) fn write_test_file(dir: &Path, name: &str, content: &str) -> PathBuf {
    let file = dir.join(name);
    fs::write(&file, content).unwrap();
    file
}
