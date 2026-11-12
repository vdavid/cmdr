//! Tests for `binary_search.rs`: each OS's candidate list, built from a fixed environment, so
//! every OS's answer is checked on every OS.

use std::path::PathBuf;

use super::*;

fn env() -> SearchEnv {
    SearchEnv {
        path_dirs: vec![PathBuf::from("/p1"), PathBuf::from("/p2")],
        android_home: Some(PathBuf::from("/ah")),
        android_sdk_root: Some(PathBuf::from("/asr")),
        home: Some(PathBuf::from("/home/test_user_1")),
        local_app_data: Some(PathBuf::from("/lad")),
    }
}

fn shown(candidates: Vec<PathBuf>) -> Vec<String> {
    candidates
        .iter()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect()
}

#[test]
fn macos_searches_the_path_the_sdk_variables_its_default_sdk_and_homebrew() {
    assert_eq!(
        shown(candidates(&AdbSearch::MACOS, &env())),
        [
            "/p1/adb",
            "/p2/adb",
            "/ah/platform-tools/adb",
            "/asr/platform-tools/adb",
            "/home/test_user_1/Library/Android/sdk/platform-tools/adb",
            "/opt/homebrew/bin/adb",
            "/usr/local/bin/adb",
        ]
    );
}

/// Android Studio's default SDK on Linux is `~/Android/Sdk` (#1108).
#[test]
fn linux_searches_android_studios_default_sdk_under_home() {
    assert_eq!(
        shown(candidates(&AdbSearch::LINUX, &env())),
        [
            "/p1/adb",
            "/p2/adb",
            "/ah/platform-tools/adb",
            "/asr/platform-tools/adb",
            "/home/test_user_1/Android/Sdk/platform-tools/adb",
            "/usr/local/bin/adb",
        ]
    );
}

/// Windows names the binary `adb.exe` everywhere, and Android Studio puts its SDK under
/// `%LOCALAPPDATA%`, not the home folder (#1108).
#[test]
fn windows_searches_adb_exe_and_the_sdk_under_local_app_data() {
    assert_eq!(
        shown(candidates(&AdbSearch::WINDOWS, &env())),
        [
            "/p1/adb.exe",
            "/p2/adb.exe",
            "/ah/platform-tools/adb.exe",
            "/asr/platform-tools/adb.exe",
            "/lad/Android/Sdk/platform-tools/adb.exe",
        ]
    );
}

#[test]
fn a_missing_variable_drops_only_its_own_candidates() {
    let bare = SearchEnv {
        path_dirs: Vec::new(),
        android_home: None,
        android_sdk_root: None,
        home: None,
        local_app_data: None,
    };
    assert_eq!(shown(candidates(&AdbSearch::LINUX, &bare)), ["/usr/local/bin/adb"]);
    assert!(candidates(&AdbSearch::WINDOWS, &bare).is_empty());
}
