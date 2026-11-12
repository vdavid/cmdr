//! The engine's log lines through the app's report redactor: a label the engine
//! builds has to come out of `redact::redact_line` with nothing personal left in
//! it, and the redactor is app policy.

use std::path::PathBuf;

use cmdr_operations::testing::ScanTarget;

/// The log lines name the first source as a quoted `path=` field, so a report redacts a folder
/// whose last word is lowercase (`Summer trip`) whole. Bare, the redactor read ` trip on volume
/// root` as prose and shipped `trip` (ERR-UK5NH). The user-facing label stays plain.
#[test]
fn the_log_label_keys_the_source_so_a_report_redacts_it_whole() {
    let one = ScanTarget::of(&[PathBuf::from("/Users/test_user_1/Pictures/Summer trip")], "root");
    let line = format!("scan preview 3 started: {}", one.log_label);
    assert_eq!(
        crate::redact::redact_line(&line),
        r#"scan preview 3 started: path="$HOME/Pictures/<dir>" on volume root"#
    );
    assert_eq!(one.label, "/Users/test_user_1/Pictures/Summer trip on volume root");

    let two = ScanTarget::of(
        &[
            PathBuf::from("/Volumes/testshare/Photos/Summer trip"),
            PathBuf::from("/Volumes/testshare/b"),
        ],
        "smb-1",
    );
    let line = format!("scan preview 4 started: {}", two.log_label);
    assert_eq!(
        crate::redact::redact_line(&line),
        r#"scan preview 4 started: path="/Volumes/<volume>/<dir>/<dir>" and 1 more on volume smb-1"#
    );

    assert_eq!(ScanTarget::of(&[], "root").log_label, "nothing on volume root");
}
