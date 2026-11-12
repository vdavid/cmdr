//! Tests for `installer.rs`: the bundle vetting order, the version gate, and the advisory list.

use super::*;

#[test]
fn staging_dir_uses_instance_id_when_set() {
    // SAFETY: nextest runs each test in its own process, so env-var mutation is process-local.
    unsafe { std::env::set_var("CMDR_INSTANCE_ID", "wt-foo") };
    let dir = staging_dir();
    assert_eq!(
        dir.file_name().and_then(|s| s.to_str()),
        Some("cmdr-update-staging-wt-foo")
    );
}

#[test]
fn staging_dir_falls_back_to_default_when_unset() {
    // SAFETY: see note in the sibling test.
    unsafe { std::env::remove_var("CMDR_INSTANCE_ID") };
    let dir = staging_dir();
    assert_eq!(
        dir.file_name().and_then(|s| s.to_str()),
        Some("cmdr-update-staging-default")
    );
}

#[test]
fn staging_dir_sits_under_temp_dir() {
    // SAFETY: see note in the sibling test.
    unsafe { std::env::remove_var("CMDR_INSTANCE_ID") };
    let dir = staging_dir();
    // allowed-fixed-temp-dir: the OS temp root IS the assertion; this reads no fixture of its own
    assert!(dir.starts_with(std::env::temp_dir()), "{:?} should sit under tmp", dir);
}

#[test]
fn find_app_bundle_above_returns_bundle_for_installed_layout() {
    let exe = Path::new("/Applications/Cmdr.app/Contents/MacOS/Cmdr");
    assert_eq!(
        find_app_bundle_above(exe),
        Some(PathBuf::from("/Applications/Cmdr.app"))
    );
}

#[test]
fn find_app_bundle_above_returns_bundle_for_user_applications_layout() {
    let exe = Path::new("/Users/test_user_12/Applications/Cmdr.app/Contents/MacOS/Cmdr");
    assert_eq!(
        find_app_bundle_above(exe),
        Some(PathBuf::from("/Users/test_user_12/Applications/Cmdr.app"))
    );
}

#[test]
fn find_app_bundle_above_returns_none_for_dev_target_layout() {
    // Regression for the noisy auto-error-reports observed in v0.21.0: dev builds run from
    // `target/<triple>/release/Cmdr` have no `.app` ancestor, so the installer can't possibly
    // succeed. The wrapping `is_running_from_app_bundle()` gate uses exactly this distinction.
    let exe = Path::new("/Users/test_user_12/code/cmdr/target/aarch64-apple-darwin/release/Cmdr");
    assert_eq!(find_app_bundle_above(exe), None);
}

#[test]
fn find_app_bundle_above_returns_none_for_worktree_target_layout() {
    let exe =
        Path::new("/Users/test_user_12/code/cmdr/.claude/worktrees/feature/target/aarch64-apple-darwin/release/Cmdr");
    assert_eq!(find_app_bundle_above(exe), None);
}

#[test]
fn sync_error_classifies_permission_denied_by_io_kind_not_message() {
    // Regression for the medium-severity audit finding: pre-fix the
    // installer substring-matched English `"Permission denied"` and
    // `"Operation not permitted"`, so localized macOS systems never
    // reached the permission arm and updates silently failed.
    // The typed variant must trigger regardless of message wording.
    let err = SyncError::from_io(
        || "Couldn't copy /a -> /b".to_string(),
        io::Error::from(io::ErrorKind::PermissionDenied),
    );
    assert!(err.is_permission_denied());

    // Localized French wording from a real `EACCES` on macOS — the old
    // substring check would have returned false on this exact string.
    let localized_message_err = SyncError {
        message: "Couldn't rename /a -> /b: Permission refusée".to_string(),
        kind: io::ErrorKind::PermissionDenied,
    };
    assert!(localized_message_err.is_permission_denied());

    // Other I/O failures aren't permission refusals.
    let other = SyncError::from_io(|| "Couldn't copy".to_string(), io::Error::from(io::ErrorKind::NotFound));
    assert!(!other.is_permission_denied());
}

/// The safety net under the pre-flight probe: a write refused on permissions mid-sync is the same
/// "can't install it here" notice the probe would have raised, ❌ never an admin prompt (on macOS
/// 13+ App Management refuses that one too) and ❌ never a failure that files an error report.
#[test]
fn a_permission_refusal_during_the_sync_is_the_unwritable_notice() {
    let err = SyncError::from_io(
        || "Couldn't rename /a -> /b".to_string(),
        io::Error::from(io::ErrorKind::PermissionDenied),
    );
    assert_eq!(
        sync_failure(err, || BundleWriteBlocker::NotWritableStandardUser),
        UpdateInstallError::Unwritable {
            blocker: BundleWriteBlocker::NotWritableStandardUser
        }
    );
}

#[test]
fn a_read_only_refusal_during_the_sync_names_the_read_only_volume() {
    let err = SyncError::from_io(
        || "Couldn't rename /a -> /b".to_string(),
        io::Error::from(io::ErrorKind::ReadOnlyFilesystem),
    );
    assert_eq!(
        sync_failure(err, || BundleWriteBlocker::NotWritableAdminUser),
        UpdateInstallError::Unwritable {
            blocker: BundleWriteBlocker::ReadOnlyVolume
        }
    );
}

#[test]
fn any_other_sync_failure_stays_a_failure() {
    let err = SyncError::from_io(|| "Couldn't copy".to_string(), io::Error::from(io::ErrorKind::NotFound));
    assert!(matches!(
        sync_failure(err, || BundleWriteBlocker::NotWritableAdminUser),
        UpdateInstallError::Failed { .. }
    ));
}

/// The real syscalls, end to end: syncing into a bundle folder this user can't write surfaces as
/// `PermissionDenied`, which is what routes it to the notice above.
#[test]
fn syncing_into_a_folder_the_user_cant_write_is_refused_on_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let staged = tempfile::tempdir().unwrap();
    let installed = tempfile::tempdir().unwrap();
    for root in [staged.path(), installed.path()] {
        fs::create_dir_all(root.join("MacOS")).unwrap();
    }
    fs::write(staged.path().join("MacOS/Cmdr"), "new").unwrap();
    fs::write(installed.path().join("MacOS/Cmdr"), "old").unwrap();
    let locked = installed.path().join("MacOS");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();

    let result = sync_bundle(staged.path(), installed.path());
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    let err = result.expect_err("a read-only folder must refuse the sync");
    assert!(err.is_permission_denied(), "expected PermissionDenied, got {err}");
    assert_eq!(fs::read_to_string(locked.join("Cmdr")).unwrap(), "old");
}

/// A staged `Cmdr.app` whose `Info.plist` names `version` (`None`: no version key).
fn staged_app_with_version(version: Option<&str>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let contents = dir.path().join("Cmdr.app/Contents");
    fs::create_dir_all(&contents).unwrap();
    let mut info = plist::Dictionary::new();
    info.insert("CFBundleIdentifier".into(), "com.veszelovszki.cmdr".into());
    if let Some(version) = version {
        info.insert("CFBundleShortVersionString".into(), version.into());
    }
    plist::Value::Dictionary(info)
        .to_file_xml(contents.join("Info.plist"))
        .unwrap();
    dir
}

/// The code-signature check for tests whose fixture bundles are unsigned on purpose.
fn trusted(_: &Path) -> Result<(), CodeSignatureRefusal> {
    Ok(())
}

#[test]
fn a_newer_staged_bundle_installs() {
    let dir = staged_app_with_version(Some("0.51.0"));
    assert_eq!(
        refuse_unless_newer(&dir.path().join("Cmdr.app"), "0.50.0"),
        Ok(semver::Version::new(0, 51, 0))
    );
}

/// ❗ The signature proves an archive is a real release, not WHICH release:
/// an older signed archive behind a manifest that claims a newer version
/// would otherwise roll the install back to a build with known bugs.
#[test]
fn a_staged_bundle_that_isnt_newer_is_refused() {
    for staged in ["0.50.0", "0.49.2", "0.51.0-beta.1"] {
        let dir = staged_app_with_version(Some(staged));
        assert!(
            refuse_unless_newer(&dir.path().join("Cmdr.app"), "0.51.0").is_err(),
            "{staged} over 0.51.0 must be refused"
        );
    }
}

/// The policy is asked about the version the archive NAMES: an unsigned manifest could offer
/// 0.52.1 under a `"0.52"` ceiling and serve a genuine, signed 0.53.0.
#[test]
fn the_policy_judges_the_version_inside_the_archive() {
    let dir = staged_app_with_version(Some("0.53.0"));
    let ceiling = crate::managed_policy::UpdateCeiling::Minor(0, 52);
    let held = |version: &semver::Version| ceiling.allows(version);
    assert_eq!(
        vet_staged_bundle(&dir.path().join("Cmdr.app"), "0.51.0", &held, &trusted, &no_list),
        Err(UpdateInstallError::BlockedByPolicy)
    );
    assert_eq!(
        vet_staged_bundle(&dir.path().join("Cmdr.app"), "0.51.0", &|_| true, &trusted, &no_list),
        Ok(vetted_0_53(None))
    );
}

/// ❗ A bundle whose Apple code signature isn't Cmdr's is refused before anything inside it is
/// believed, even its `Info.plist`: a leaked updater key can sign an archive, not a Developer ID
/// build. The refusal is typed, and the policy is never asked about such a bundle.
#[test]
fn a_bundle_not_signed_by_cmdr_is_refused_first() {
    let dir = staged_app_with_version(Some("0.53.0"));
    let policy_asked = std::cell::Cell::new(false);
    let may_install = |_: &semver::Version| {
        policy_asked.set(true);
        true
    };
    let not_ours = |_: &Path| Err(CodeSignatureRefusal::NotOurs);
    assert!(matches!(
        vet_staged_bundle(
            &dir.path().join("Cmdr.app"),
            "0.51.0",
            &may_install,
            &not_ours,
            &no_list
        ),
        Err(UpdateInstallError::UntrustedCodeSignature { .. })
    ));
    assert!(
        !policy_asked.get(),
        "an untrusted bundle's version must not reach the policy"
    );
}

/// A rollback stays a failure even where the policy would allow it: the version check comes first.
#[test]
fn an_older_archive_is_a_failure_not_a_policy_refusal() {
    let dir = staged_app_with_version(Some("0.50.0"));
    assert!(matches!(
        vet_staged_bundle(&dir.path().join("Cmdr.app"), "0.51.0", &|_| true, &trusted, &no_list),
        Err(UpdateInstallError::Failed { .. })
    ));
}

/// What vetting the staged 0.53.0 bundle answers: its own version, with `security_notice`.
fn vetted_0_53(security_notice: Option<SecurityNotice>) -> VettedBundle {
    VettedBundle {
        version: semver::Version::new(0, 53, 0),
        security_notice,
    }
}

/// The advisory reader for tests that don't care about the list.
fn no_list(_: &Path) -> Option<String> {
    None
}

/// Writes `json` where a bundle carries its advisory list.
fn with_advisory_list(dir: &tempfile::TempDir, json: &str) {
    let resources = dir.path().join("Cmdr.app/Contents/Resources");
    fs::create_dir_all(&resources).unwrap();
    fs::write(resources.join("security-advisories.json"), json).unwrap();
}

const STAGED_LIST: &str = r#"{"advisories":[
    {"id":"GHSA-high-0000-0000","fixedIn":"0.52.0","severity":"high","url":"https://github.com/vdavid/cmdr/security/advisories/GHSA-high-0000-0000"},
    {"id":"GHSA-old0-0000-0000","fixedIn":"0.51.0","severity":"critical","url":"https://github.com/vdavid/cmdr/security/advisories/GHSA-old0-0000-0000"}
]}"#;

/// ❗ Until the code signature holds, the list inside the bundle is whatever the updater key's
/// holder chose to write, so it's never read. Nor is the list of a bundle the policy refuses.
#[test]
fn the_advisory_list_of_an_untrusted_or_refused_bundle_is_never_read() {
    let dir = staged_app_with_version(Some("0.53.0"));
    let list_read = std::cell::Cell::new(false);
    let read = |_: &Path| {
        list_read.set(true);
        Some(STAGED_LIST.to_string())
    };
    let not_ours = |_: &Path| Err(CodeSignatureRefusal::NotOurs);
    assert!(vet_staged_bundle(&dir.path().join("Cmdr.app"), "0.51.0", &|_| true, &not_ours, &read).is_err());
    assert!(!list_read.get(), "an untrusted bundle's advisory list must not be read");

    assert_eq!(
        vet_staged_bundle(&dir.path().join("Cmdr.app"), "0.51.0", &|_| false, &trusted, &read),
        Err(UpdateInstallError::BlockedByPolicy)
    );
    assert!(!list_read.get(), "a refused bundle's advisory list must not be read");
}

/// A trusted bundle's own list names the notice, counting only what's newer than the running
/// build: 0.51.0 already carries the fix it was built with.
#[test]
fn a_trusted_bundle_answers_the_notice_its_own_list_names() {
    let dir = staged_app_with_version(Some("0.53.0"));
    with_advisory_list(&dir, STAGED_LIST);
    assert_eq!(
        vet_staged_bundle(
            &dir.path().join("Cmdr.app"),
            "0.51.0",
            &|_| true,
            &trusted,
            &advisories::read_bundle_list
        ),
        Ok(vetted_0_53(Some(SecurityNotice {
            severity: crate::security_advisories::Severity::High,
            count: 1,
            url: Some("https://github.com/vdavid/cmdr/security/advisories/GHSA-high-0000-0000".to_string()),
        })))
    );
}

/// The update is the security fix itself: a list that's missing or broken costs the notice,
/// never the install.
#[test]
fn a_missing_or_malformed_list_still_installs() {
    let dir = staged_app_with_version(Some("0.53.0"));
    let app = dir.path().join("Cmdr.app");
    assert_eq!(
        vet_staged_bundle(&app, "0.51.0", &|_| true, &trusted, &advisories::read_bundle_list),
        Ok(vetted_0_53(None))
    );
    with_advisory_list(&dir, "{ not json");
    assert_eq!(
        vet_staged_bundle(&app, "0.51.0", &|_| true, &trusted, &advisories::read_bundle_list),
        Ok(vetted_0_53(None))
    );
}

#[test]
fn a_staged_bundle_without_a_readable_version_is_refused() {
    let dir = staged_app_with_version(None);
    assert!(refuse_unless_newer(&dir.path().join("Cmdr.app"), "0.50.0").is_err());
    let empty = tempfile::tempdir().unwrap();
    assert!(refuse_unless_newer(&empty.path().join("Cmdr.app"), "0.50.0").is_err());
}

#[test]
fn an_update_brings_the_finder_extension_along_and_a_bundle_without_one_still_syncs() {
    use std::fs;

    let staged = tempfile::tempdir().unwrap();
    let installed = tempfile::tempdir().unwrap();
    let appex = "PlugIns/CmdrFinderExtension.appex/Contents/MacOS/CmdrFinderExtension";
    fs::create_dir_all(staged.path().join(appex).parent().unwrap()).unwrap();
    fs::create_dir_all(staged.path().join("MacOS")).unwrap();
    fs::write(staged.path().join(appex), "new extension").unwrap();
    fs::write(staged.path().join("MacOS/Cmdr"), "new").unwrap();

    sync_bundle(staged.path(), installed.path()).expect("the first sync");
    assert_eq!(
        fs::read_to_string(installed.path().join(appex)).unwrap(),
        "new extension"
    );

    fs::remove_dir_all(staged.path().join("PlugIns")).unwrap();
    sync_bundle(staged.path(), installed.path()).expect("a staged bundle with no extension");
    assert!(!installed.path().join(appex).exists(), "a stale extension must go");
}
