//! The `tauri-plugin-store` files, checked before anything loads them. The plugin reads a file it
//! can't read or parse as an empty store, so `setup()` sets such a file aside first and the plugin
//! then starts from no file at all. When the set-aside doesn't happen, the patched plugin refuses
//! every save of that store for the session (`vendor/tauri-plugin-store/CMDR-PATCH.md`).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use cmdr_fs::log_private::LogPrivate;
use tauri::{AppHandle, Runtime};

use super::{LOG_TARGET, StateFile};

/// The consent settings that default to on. A `settings.json` Cmdr had to set aside may have held
/// a "no" to either, so the fresh file says "no" until the person says otherwise
/// (`lib/settings/definitions/updates-privacy.ts` holds both defaults). `operationLog.enabled`
/// isn't here: undo history never leaves the Mac (David, 2026-10-16).
const CONSENT_OFF_AFTER_RECOVERY: [&str; 2] = ["analytics.enabled", "updates.crashReports"];

/// This session recovered `settings.json`, so [`settings_for_consent`] reads a missing file as
/// every consent off rather than as a first launch.
static SETTINGS_RECOVERED: AtomicBool = AtomicBool::new(false);

/// This session set `license.json` aside (or found it unreadable), so licensing asks the secret
/// store once whether a key is still there ([`license_file_recovered`]).
static LICENSE_RECOVERED: AtomicBool = AtomicBool::new(false);

/// Whether this launch recovered `license.json`, whose "key is in the secret store" marker went
/// with it. Licensing reads it once at launch to probe for the key and restore the marker.
pub fn license_file_recovered() -> bool {
    LICENSE_RECOVERED.load(Ordering::Relaxed)
}

/// The four plugin stores, at the paths their readers use. Call from `setup()` after the instance
/// lock and the security log, and before anything reads one of them.
pub fn recover_plugin_stores<R: Runtime>(app: &AppHandle<R>) {
    if let Some(path) = frontend_store_path(app, "settings.json") {
        recover_settings(app, &path);
    }
    for (name, file) in [
        ("shortcuts.json", StateFile::Shortcuts),
        ("app-status.json", StateFile::AppStatus),
    ] {
        if let Some(path) = frontend_store_path(app, name) {
            // allowed-discarded-outcome: setup has no surface to report a recovery on; `recover` logs what it did
            recover(&path, file);
        }
    }
    // Rust loads this one by bare name, so it always sits in the plugin's own app data dir.
    match tauri_plugin_store::resolve_store_path(app, "license.json") {
        Ok(path) => {
            if recover(&path, StateFile::License) {
                LICENSE_RECOVERED.store(true, Ordering::Relaxed);
            }
        }
        Err(e) => log::warn!(target: LOG_TARGET, "Couldn't resolve license.json to check it: {e}"),
    }
}

/// Checks `settings.json` like the other stores. When it needed recovering, the store the
/// frontend loads is opened here first, with every consent off as its defaults: a fresh file says
/// "no" too, a fresh file that didn't land starts from those defaults, and a file that stayed
/// unloaded (the set-aside failed) never reaches the cache at all. So no session runs on a "yes"
/// the person may never have given, whichever way the recovery went.
fn recover_settings<R: Runtime>(app: &AppHandle<R>, path: &Path) {
    if !recover(path, StateFile::Settings) {
        return;
    }
    SETTINGS_RECOVERED.store(true, Ordering::Relaxed);
    let opened = tauri_plugin_store::StoreBuilder::new(app, path)
        .defaults(consents_off().into_iter().collect())
        .disable_auto_save()
        .build();
    if let Err(e) = opened {
        log::warn!(target: LOG_TARGET, "Couldn't open the recovered settings store with consents off: {e}");
    }
}

/// The stored `settings.json` as raw JSON, for a Rust reader that decides on consent (analytics).
/// `Null` when there's no file (a first launch: the defaults). A file that's there but can't be
/// read or parsed reads as every consent off, the same answer the frontend's store gives that
/// session ([`recover_settings`]), and so does no file once this session recovered one.
pub fn settings_for_consent(path: &Path) -> serde_json::Value {
    consent_settings(path, SETTINGS_RECOVERED.load(Ordering::Relaxed))
}

/// [`settings_for_consent`] over an injected "this session recovered `settings.json`".
fn consent_settings(path: &Path, recovered: bool) -> serde_json::Value {
    match fs::read(path) {
        // A recovery whose fresh file didn't land leaves no file, and that's not a first launch.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && recovered => serde_json::Value::Object(consents_off()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::Value::Null,
        Ok(bytes) => match serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&bytes) {
            Ok(settings) => serde_json::Value::Object(settings),
            Err(_) => serde_json::Value::Object(consents_off()),
        },
        Err(_) => serde_json::Value::Object(consents_off()),
    }
}

/// Every consent that defaults to on, set to off.
fn consents_off() -> serde_json::Map<String, serde_json::Value> {
    CONSENT_OFF_AFTER_RECOVERY
        .iter()
        .map(|key| ((*key).to_string(), serde_json::Value::Bool(false)))
        .collect()
}

/// Where the frontend's store for `name` lives: `resolveStorePath` (`lib/settings/store-path.ts`)
/// puts it in `CMDR_DATA_DIR` when that's set, and the plugin resolves a bare name otherwise.
fn frontend_store_path<R: Runtime>(app: &AppHandle<R>, name: &str) -> Option<PathBuf> {
    if std::env::var("CMDR_DATA_DIR").is_ok_and(|dir| !dir.is_empty()) {
        return crate::config::resolved_app_data_dir(app).ok().map(|dir| dir.join(name));
    }
    tauri_plugin_store::resolve_store_path(app, name).ok()
}

/// Checks one store file: sweeps temp files a killed save left, and sets the file aside when it
/// can't be read (a root-owned file after a `sudo` run, a mode a restore reset, a disk read
/// error) or isn't the JSON object the plugin reads. A recovered `settings.json` then gets its
/// fresh file. Returns whether it recovered.
fn recover(path: &Path, file: StateFile) -> bool {
    sweep_killed_saves(path);
    match fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return false,
        Err(e) => log::warn!(
            target: LOG_TARGET,
            "Couldn't read {path:?}: {e}",
            path = LogPrivate::file_path(path)
        ),
        // The plugin's own reading: a JSON object. A single wrong value inside one is the
        // store's to drop, not a recovery.
        Ok(bytes) if serde_json::from_slice::<HashMap<String, serde_json::Value>>(&bytes).is_ok() => return false,
        Ok(_) => {}
    }
    if super::set_aside(path, file).is_none() {
        // The file stays and the plugin reads it as empty, but refuses to save over it.
        return true;
    }
    if file == StateFile::Settings {
        write_fresh_settings(path);
    }
    true
}

/// Starts the new `settings.json` with every consent setting that defaults to on set to off.
fn write_fresh_settings(path: &Path) {
    let written = serde_json::to_string_pretty(&consents_off())
        .map_err(std::io::Error::other)
        .and_then(|json| crate::config::durable_write_json(path, &temp_path(path, "recovery"), &json));
    if let Err(e) = written {
        log::warn!(
            target: LOG_TARGET,
            "Couldn't write the fresh settings file, so the settings store runs on its consents-off defaults this launch: {e}"
        );
    }
}

/// A temp file beside `path` in the patched plugin's naming (`.<name>.<pid>.<n>.tmp`), so the
/// next launch's sweep finds it too.
fn temp_path(path: &Path, tag: &str) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!(".{name}.{tag}.tmp"))
}

/// Removes the temp files a save killed before its rename left beside `path`. Only at startup,
/// before anything can be saving.
fn sweep_killed_saves(path: &Path) {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return;
    };
    let prefix = format!(".{}.", name.to_string_lossy());
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let entry_name = entry.file_name();
        let entry_name = entry_name.to_string_lossy();
        if entry_name.starts_with(&prefix)
            && entry_name.ends_with(".tmp")
            && let Err(e) = fs::remove_file(entry.path())
        {
            log::warn!(
                target: LOG_TARGET,
                "Couldn't remove the leftover temp file {entry_name}: {e}",
                entry_name = LogPrivate::file_name(&*entry_name)
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security_log::{self, SecurityEvent};
    use crate::state_recovery::testing;
    use crate::test_support::TestDir;

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("read the dir")
            .map(|entry| entry.expect("an entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort_unstable();
        names
    }

    fn copy_in(dir: &Path, name: &str) -> PathBuf {
        let prefix = format!("{name}.broken-");
        let copies: Vec<String> = names_in(dir).into_iter().filter(|n| n.starts_with(&prefix)).collect();
        assert_eq!(copies.len(), 1, "one copy of {name}: {copies:?}");
        dir.join(&copies[0])
    }

    #[test]
    fn a_truncated_settings_file_is_kept_and_the_fresh_one_says_no_to_both_consents() {
        let notices = testing::capture_recoveries();
        let dir = TestDir::new("plugin_stores_settings");
        let path = dir.join("settings.json");
        fs::write(&path, b"{\n  \"analytics.enabled\": fal").expect("write");

        assert!(recover(&path, StateFile::Settings));

        let copy = copy_in(&dir, "settings.json");
        assert_eq!(
            fs::read(&copy).expect("read the copy"),
            b"{\n  \"analytics.enabled\": fal"
        );
        let fresh: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("a fresh file")).expect("valid JSON");
        assert_eq!(
            fresh,
            serde_json::json!({ "analytics.enabled": false, "updates.crashReports": false })
        );
        let loaded = crate::settings::loader::load_settings_from_dir(&dir);
        assert_eq!(
            loaded.crash_reports_enabled,
            Some(false),
            "Rust reads the same launch as off"
        );
        assert_eq!(notices.recoveries().len(), 1);
        assert_eq!(notices.recoveries()[0].file, StateFile::Settings);
    }

    /// One notice and one security event per recovery; the next launch reads the fresh file and
    /// says nothing.
    #[test]
    fn a_recovery_speaks_once_and_a_healthy_file_never() {
        let events = security_log::testing::capture_events();
        let notices = testing::capture_recoveries();
        let dir = TestDir::new("plugin_stores_once");
        let path = dir.join("settings.json");
        fs::write(&path, b"").expect("an empty file, as a crash mid-write leaves it");

        assert!(recover(&path, StateFile::Settings));
        assert!(
            !recover(&path, StateFile::Settings),
            "the next launch finds a healthy file"
        );

        assert_eq!(
            events.events(),
            vec![SecurityEvent::StateFileRecovered {
                file: StateFile::Settings
            }]
        );
        assert_eq!(notices.recoveries().len(), 1);
    }

    #[test]
    fn a_corrupt_shortcuts_or_license_file_is_kept_and_starts_from_no_file() {
        let notices = testing::capture_recoveries();
        let dir = TestDir::new("plugin_stores_others");
        for (name, file) in [
            ("shortcuts.json", StateFile::Shortcuts),
            ("license.json", StateFile::License),
        ] {
            let path = dir.join(name);
            fs::write(&path, b"[1, 2").expect("write");
            assert!(recover(&path, file));
            assert!(!path.exists(), "{name}: the plugin starts from no file");
            assert_eq!(fs::read(copy_in(&dir, name)).expect("read the copy"), b"[1, 2");
        }
        let files: Vec<StateFile> = notices.recoveries().iter().map(|r| r.file).collect();
        assert_eq!(files, vec![StateFile::Shortcuts, StateFile::License]);
    }

    #[test]
    fn a_healthy_or_missing_file_is_left_alone() {
        let notices = testing::capture_recoveries();
        let dir = TestDir::new("plugin_stores_healthy");
        let path = dir.join("settings.json");
        assert!(!recover(&path, StateFile::Settings), "no file is a first launch");
        fs::write(&path, b"{\"listing.showHiddenFiles\": \"not a bool\"}").expect("write");
        assert!(
            !recover(&path, StateFile::Settings),
            "a single invalid value is the store's to drop, not a recovery"
        );
        assert_eq!(names_in(&dir), vec!["settings.json"]);
        assert!(notices.recoveries().is_empty());
    }

    /// A file that's there but can't be read (mode 000, as after a `sudo` run or a restore) is set
    /// aside like a damaged one, so its bytes stay and the plugin starts from no file. A recovered
    /// `settings.json` still comes back with both consents off.
    #[test]
    fn an_unreadable_store_file_is_kept_and_starts_from_no_file() {
        use std::os::unix::fs::PermissionsExt;
        // Root reads a mode-000 file anyway (the Linux Docker lane runs as root).
        // SAFETY: (test) `geteuid` takes no arguments, shares no memory, and can't fail.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let notices = testing::capture_recoveries();
        let dir = TestDir::new("plugin_stores_unreadable");
        for (name, file) in [
            ("settings.json", StateFile::Settings),
            ("license.json", StateFile::License),
        ] {
            let path = dir.join(name);
            fs::write(&path, b"{\"analytics.enabled\": false}").expect("write");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("make it unreadable");

            assert!(recover(&path, file), "{name}: an unreadable file is a recovery");

            let copy = copy_in(&dir, name);
            fs::set_permissions(&copy, fs::Permissions::from_mode(0o644)).expect("read the copy");
            assert_eq!(
                fs::read(&copy).expect("read the copy"),
                b"{\"analytics.enabled\": false}"
            );
        }
        assert!(
            !dir.join("license.json").exists(),
            "the plugin starts from no license file"
        );
        let fresh: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("settings.json")).expect("a fresh file")).expect("valid JSON");
        assert_eq!(
            fresh,
            serde_json::json!({ "analytics.enabled": false, "updates.crashReports": false })
        );
        let files: Vec<StateFile> = notices.recoveries().iter().map(|r| r.file).collect();
        assert_eq!(files, vec![StateFile::Settings, StateFile::License]);
    }

    /// A save killed between writing its temp file and the rename leaves the temp file behind.
    #[test]
    fn a_killed_saves_temp_file_is_swept() {
        let dir = TestDir::new("plugin_stores_sweep");
        let path = dir.join("shortcuts.json");
        fs::write(&path, b"{}").expect("write");
        fs::write(dir.join(".shortcuts.json.4242.0.tmp"), b"{\"half").expect("write");
        fs::write(dir.join(".settings.json.4242.0.tmp"), b"another store's").expect("write");

        recover(&path, StateFile::Shortcuts);

        assert_eq!(names_in(&dir), vec![".settings.json.4242.0.tmp", "shortcuts.json"]);
    }

    fn mock_app() -> tauri::App<tauri::test::MockRuntime> {
        let app = tauri::test::mock_app();
        app.handle()
            .plugin(tauri_plugin_store::Builder::default().build())
            .expect("the store plugin registers on a mock app");
        app
    }

    /// A `settings.json` that didn't load AND couldn't be set aside (here an immutable file,
    /// which refuses the rename) may have held a "no" to usage stats or crash reports. For the
    /// rest of the session both read as off: in the store the frontend loads, and for Rust's
    /// own consent reads.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_settings_file_that_stays_unloaded_reads_every_consent_as_off() {
        use tauri_plugin_store::StoreExt;
        let _notices = testing::capture_recoveries();
        let dir = TestDir::new("plugin_stores_held_consent");
        let path = dir.join("settings.json");
        fs::write(&path, b"{\"analytics.enabled\": fal").expect("write");
        testing::set_immutable(&path, true);
        let app = mock_app();

        recover_settings(app.handle(), &path);
        let frontend = app.store_builder(&path).build().expect("the frontend's load");
        let rust_side = settings_for_consent(&path);
        let saved = frontend.save();
        testing::set_immutable(&path, false);

        for key in CONSENT_OFF_AFTER_RECOVERY {
            assert_eq!(frontend.get(key), Some(serde_json::json!(false)), "{key} in the store");
            assert_eq!(rust_side.get(key), Some(&serde_json::json!(false)), "{key} for Rust");
        }
        assert!(saved.is_err(), "the held file is still never saved over");
        assert_eq!(fs::read(&path).expect("the file stays"), b"{\"analytics.enabled\": fal");
    }

    /// A recovery whose fresh file didn't land (here removed after the fact, as an unwritable data
    /// dir would leave it) leaves no `settings.json`. That's not a first launch, so Rust reads
    /// both consents off, as the frontend's store does.
    #[test]
    fn no_settings_file_after_a_recovery_reads_every_consent_as_off() {
        let _notices = testing::capture_recoveries();
        let dir = TestDir::new("plugin_stores_consent_after_recovery");
        let path = dir.join("settings.json");
        fs::write(&path, b"{\"analytics.enabled\": fal").expect("write");
        let app = mock_app();

        recover_settings(app.handle(), &path);
        fs::remove_file(&path).expect("the fresh file didn't land");

        let rust_side = settings_for_consent(&path);
        for key in CONSENT_OFF_AFTER_RECOVERY {
            assert_eq!(rust_side.get(key), Some(&serde_json::json!(false)), "{key} for Rust");
        }
    }

    /// A file that reads keeps the person's own answers, and a missing one reads as no settings.
    #[test]
    fn a_healthy_or_missing_settings_file_keeps_its_consents() {
        let dir = TestDir::new("plugin_stores_consent_healthy");
        let path = dir.join("settings.json");
        // Over the injected flag: another test in this process may have recovered a settings file.
        assert_eq!(consent_settings(&path, false), serde_json::Value::Null);
        fs::write(&path, b"{\"analytics.enabled\": true}").expect("write");
        assert_eq!(
            consent_settings(&path, false),
            serde_json::json!({ "analytics.enabled": true })
        );
        assert_eq!(
            consent_settings(&path, true),
            serde_json::json!({ "analytics.enabled": true }),
            "a file saved after a recovery is the person's answer again"
        );
    }
}
