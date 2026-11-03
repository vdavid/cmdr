//! `open_with_default_app`: the volume's say comes before any launch, and the
//! opener's refusal comes back typed.

use super::*;
use crate::file_system::volume::Volume;
use crate::file_system::volume::manager::test_support::TestVolumeRegistration;
use crate::test_support::CapabilityStub;
use std::sync::Arc;

/// A registered volume answering `paths_are_os_visible`, under a per-test id so
/// the process-wide registry can't make two tests collide.
fn register(id: &str, paths_are_os_visible: bool) -> TestVolumeRegistration {
    TestVolumeRegistration::install(
        id,
        Arc::new(CapabilityStub {
            supports_local_fs_access: false,
            paths_are_os_visible,
        }) as Arc<dyn Volume>,
    )
}

/// A path nothing will ever create, so a real opener can only refuse it.
const MISSING: &str = "/nonexistent-cmdr-default-app-test/never-here.txt";

/// The reported bug's shape: a direct SMB share whose macOS mount is gone. Its
/// rows still read `/Volumes/<share>/…`, and handing one to `open` did nothing,
/// silently. Refused before the opener runs.
#[tokio::test]
async fn a_volume_whose_paths_other_apps_cant_open_refuses_before_launching() {
    let _volume = register("default-app-test-stranded-smb", false);
    let answer = open_with_default_app(MISSING.to_string(), Some("default-app-test-stranded-smb".to_string())).await;
    assert_eq!(answer, Err(OpenPathError::NotOnThisMac));
}

/// The opener's own refusal used to be dropped (`spawn` and walk away). macOS's
/// `open` exits 1 for a missing file and launches nothing, which makes it safe to
/// run here.
#[cfg(all(target_os = "macos", not(feature = "playwright-e2e")))]
#[tokio::test]
async fn the_openers_refusal_comes_back_typed() {
    let _volume = register("default-app-test-visible", true);
    let answer = open_with_default_app(MISSING.to_string(), Some("default-app-test-visible".to_string())).await;
    assert!(
        matches!(answer, Err(OpenPathError::NotOpened { exit_code: Some(code) }) if code != 0),
        "got {answer:?}"
    );
}

/// No volume to ask (the search-results snapshot's id, a favorite): the opener
/// answers for the path itself.
#[cfg(all(target_os = "macos", not(feature = "playwright-e2e")))]
#[tokio::test]
async fn an_unregistered_volume_leaves_the_answer_to_the_opener() {
    let answer = open_with_default_app(MISSING.to_string(), Some("default-app-test-nobody-here".to_string())).await;
    assert!(matches!(answer, Err(OpenPathError::NotOpened { .. })), "got {answer:?}");
}
