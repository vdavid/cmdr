//! Where the license key lives: the secret store (the Keychain on macOS), never `license.json`.
//!
//! The key's payload carries the buyer's email, and the key (or its short code) is what activates
//! a license, so it's a secret like an SMB password or an API key. `license.json` keeps only
//! non-secret state: the cached server verdict, timers, and [`FILE_KEY_IN_SECRET_STORE`], the
//! marker that says a key is waiting in the secret store. The marker is what keeps a Personal
//! install from ever touching the Keychain.
//!
//! Older versions wrote the key into `license.json` in plain text. [`migrate`] moves it on launch,
//! and only removes it from the file once the secret store gave the same bytes back. See
//! `DETAILS.md` § Key storage.

use crate::secrets::{SecretStore, SecretStoreError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri_plugin_store::StoreExt;

/// The licensing store file, under the app data dir.
pub(super) const LICENSE_FILE: &str = "license.json";

/// The secret-store entry holding [`StoredLicense`] as JSON.
pub(super) const SECRET_KEY: &str = "license.key";

/// Where versions before the move kept the key, in plain text. Read only to migrate it (or to use
/// it while a migration keeps failing), and written by nothing.
const FILE_KEY_LEGACY_LICENSE: &str = "license_key";
const FILE_KEY_LEGACY_SHORT_CODE: &str = "license_short_code";

/// `true` in `license.json` when the key is in the secret store.
const FILE_KEY_IN_SECRET_STORE: &str = "license_key_in_secret_store";

/// The secret part of a license: the full key and the short code it was activated with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StoredLicense {
    pub license_key: String,
    pub short_code: Option<String>,
}

/// The slice of `license.json` this module touches: the tauri store in production, a map in tests.
pub(super) trait LicenseFile {
    fn get(&self, key: &str) -> Option<Value>;
    fn set(&self, key: &str, value: Value);
    fn delete(&self, key: &str);
}

impl<R: tauri::Runtime> LicenseFile for tauri_plugin_store::Store<R> {
    fn get(&self, key: &str) -> Option<Value> {
        tauri_plugin_store::Store::get(self, key)
    }

    fn set(&self, key: &str, value: Value) {
        tauri_plugin_store::Store::set(self, key, value);
    }

    fn delete(&self, key: &str) {
        // `false` only means the key wasn't there, which is the goal anyway.
        let _ = tauri_plugin_store::Store::delete(self, key);
    }
}

/// Writes `license.json` now, and says so when it can't. The store plugin's debounced auto-save
/// drops a failed write without a word, so every change here saves explicitly: the patched plugin
/// leaves the old file whole on a failure (`vendor/tauri-plugin-store/CMDR-PATCH.md`), and the log
/// says the change didn't land.
pub(super) fn save_license_file<R: tauri::Runtime>(store: &tauri_plugin_store::Store<R>) {
    if let Err(e) = store.save() {
        log::warn!("Couldn't save {LICENSE_FILE}, so the last saved version stays: {e}");
    }
}

/// Moves a license key an older version left in `license.json` in plain text into the secret
/// store. Runs once per launch, before the first status check; a no-op once moved. A failure
/// keeps the key in the file (the license keeps working from there) and retries next launch.
pub fn move_license_key_into_secret_store(app: &tauri::AppHandle) {
    let store = match app.store(LICENSE_FILE) {
        Ok(store) => store,
        Err(e) => {
            log::warn!("Couldn't open {LICENSE_FILE} to check for a license key to move: {e}");
            return;
        }
    };
    match migrate(&*store, crate::secrets::store()) {
        MigrationOutcome::NothingToMigrate => {}
        MigrationOutcome::Migrated => {
            save_license_file(&store);
            log::info!("Moved the license key from {LICENSE_FILE} into the secret store");
        }
        MigrationOutcome::KeptInFile(reason) => log::warn!(
            "Couldn't move the license key into the secret store, keeping it in {LICENSE_FILE} and retrying next launch: {reason}"
        ),
    }
    match restore_marker(
        &*store,
        crate::secrets::store(),
        crate::state_recovery::license_file_recovered(),
    ) {
        MarkerRestore::NotNeeded => {}
        MarkerRestore::Restored => {
            save_license_file(&store);
            log::info!("{LICENSE_FILE} was set aside, and the license key is still in the secret store, so it's back");
        }
        MarkerRestore::NoKeyStored => {
            log::info!("{LICENSE_FILE} was set aside, and the secret store holds no license key to restore");
        }
        MarkerRestore::Unreadable(reason) => log::warn!(
            "{LICENSE_FILE} was set aside, and the secret store didn't give back a license key, so the license reads as absent until it's entered again: {reason}"
        ),
    }
}

/// Removes the stored key from the secret store and `license.json`. Called by `reset_license`.
pub(super) fn clear_stored_license(app: &tauri::AppHandle) {
    if let Ok(store) = app.store(LICENSE_FILE) {
        clear(&*store, crate::secrets::store());
        save_license_file(&store);
    }
}

/// What [`migrate`] did, for the log.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum MigrationOutcome {
    /// No key in `license.json`: nothing to move.
    NothingToMigrate,
    /// The key is in the secret store and gone from the file.
    Migrated,
    /// The secret store didn't take the key, so the file stays exactly as it was and the next
    /// launch tries again.
    KeptInFile(String),
}

/// Moves a key an older version left in `license.json` into the secret store.
///
/// Never loses a license: the file loses the key only after the secret store returned exactly the
/// bytes written. Any failure leaves the file untouched, [`load`] keeps reading the key from it,
/// and the next launch retries.
pub(super) fn migrate(file: &impl LicenseFile, secrets: &dyn SecretStore) -> MigrationOutcome {
    let Some(license) = license_in_file(file) else {
        return MigrationOutcome::NothingToMigrate;
    };
    match write_and_read_back(secrets, &license) {
        Ok(()) => {
            mark_in_secret_store(file);
            MigrationOutcome::Migrated
        }
        Err(e) => MigrationOutcome::KeptInFile(e.to_string()),
    }
}

/// Stores a newly activated license. On `Err`, the file is unchanged.
pub(super) fn save(
    file: &impl LicenseFile,
    secrets: &dyn SecretStore,
    license: &StoredLicense,
) -> Result<(), SecretStoreError> {
    write_and_read_back(secrets, license)?;
    mark_in_secret_store(file);
    Ok(())
}

/// The stored license, if any. Touches the secret store only when the marker says a key is there.
///
/// A key still in the file wins: it's either a migration that hasn't succeeded yet, or a newer key
/// an older version wrote after a downgrade, which the next launch's migration moves over.
pub(super) fn load(
    file: &impl LicenseFile,
    secrets: &dyn SecretStore,
) -> Result<Option<StoredLicense>, SecretStoreError> {
    if let Some(license) = license_in_file(file) {
        return Ok(Some(license));
    }
    if file.get(FILE_KEY_IN_SECRET_STORE).and_then(|v| v.as_bool()) != Some(true) {
        return Ok(None);
    }
    match secrets.get(SECRET_KEY) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| SecretStoreError::Other(format!("The stored license entry doesn't parse: {e}"))),
        Err(SecretStoreError::NotFound(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Removes the license from both places. A secret-store failure is logged, not returned: the
/// marker goes either way, so the app reads as unlicensed, and the next `save` overwrites the
/// leftover entry.
pub(super) fn clear(file: &impl LicenseFile, secrets: &dyn SecretStore) {
    match secrets.delete(SECRET_KEY) {
        Ok(()) | Err(SecretStoreError::NotFound(_)) => {}
        Err(e) => log::warn!("Couldn't remove the license key from the secret store: {e}"),
    }
    file.delete(FILE_KEY_IN_SECRET_STORE);
    file.delete(FILE_KEY_LEGACY_LICENSE);
    file.delete(FILE_KEY_LEGACY_SHORT_CODE);
}

/// What [`restore_marker`] did, for the log.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum MarkerRestore {
    /// `license.json` wasn't recovered this launch, or still says where the key is: no probe.
    NotNeeded,
    /// The secret store still held a license, so the marker is back.
    Restored,
    /// The secret store holds no license: a Personal install, or one that was reset.
    NoKeyStored,
    /// The secret store didn't answer, or held something that isn't a license. No marker.
    Unreadable(String),
}

/// After `license.json` was set aside, asks the secret store once whether a license is still
/// there, and writes the marker back if so. A set-aside file took the marker with it while the key
/// stayed in the Keychain, so without this a paid install reads as Personal until the key is
/// entered again. Every launch that didn't recover the file skips the probe, which keeps a
/// Personal install off the Keychain.
pub(super) fn restore_marker(
    file: &impl LicenseFile,
    secrets: &dyn SecretStore,
    license_file_recovered: bool,
) -> MarkerRestore {
    let marker_present = file.get(FILE_KEY_IN_SECRET_STORE).and_then(|v| v.as_bool()) == Some(true);
    if !license_file_recovered || marker_present || license_in_file(file).is_some() {
        return MarkerRestore::NotNeeded;
    }
    match secrets.get(SECRET_KEY) {
        Ok(bytes) => match serde_json::from_slice::<StoredLicense>(&bytes) {
            Ok(_) => {
                file.set(FILE_KEY_IN_SECRET_STORE, Value::Bool(true));
                MarkerRestore::Restored
            }
            Err(e) => MarkerRestore::Unreadable(format!("The stored license entry doesn't parse: {e}")),
        },
        Err(SecretStoreError::NotFound(_)) => MarkerRestore::NoKeyStored,
        Err(e) => MarkerRestore::Unreadable(e.to_string()),
    }
}

/// A key an older version wrote into the file, if there is one.
fn license_in_file(file: &impl LicenseFile) -> Option<StoredLicense> {
    let license_key = file.get(FILE_KEY_LEGACY_LICENSE)?.as_str()?.to_string();
    let short_code = file
        .get(FILE_KEY_LEGACY_SHORT_CODE)
        .and_then(|v| v.as_str().map(str::to_string));
    Some(StoredLicense {
        license_key,
        short_code,
    })
}

/// Writes the entry, then reads it back: a locked keyring can accept a write it never keeps.
fn write_and_read_back(secrets: &dyn SecretStore, license: &StoredLicense) -> Result<(), SecretStoreError> {
    let bytes = serde_json::to_vec(license)
        .map_err(|e| SecretStoreError::Other(format!("Couldn't serialize the license entry: {e}")))?;
    secrets.set(SECRET_KEY, &bytes)?;
    if secrets.get(SECRET_KEY)? != bytes {
        return Err(SecretStoreError::Other(
            "The secret store gave back different bytes than it was given".to_string(),
        ));
    }
    Ok(())
}

/// Records that the key is in the secret store and drops any plaintext copy from the file.
fn mark_in_secret_store(file: &impl LicenseFile) {
    file.set(FILE_KEY_IN_SECRET_STORE, Value::Bool(true));
    file.delete(FILE_KEY_LEGACY_LICENSE);
    file.delete(FILE_KEY_LEGACY_SHORT_CODE);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ignore_poison::IgnorePoison;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct MapFile(Mutex<HashMap<String, Value>>);

    impl LicenseFile for MapFile {
        fn get(&self, key: &str) -> Option<Value> {
            self.0.lock_ignore_poison().get(key).cloned()
        }
        fn set(&self, key: &str, value: Value) {
            self.0.lock_ignore_poison().insert(key.to_string(), value);
        }
        fn delete(&self, key: &str) {
            self.0.lock_ignore_poison().remove(key);
        }
    }

    impl MapFile {
        fn snapshot(&self) -> HashMap<String, Value> {
            self.0.lock_ignore_poison().clone()
        }
    }

    /// An in-memory secret store that can refuse writes, hand back other bytes than it was given,
    /// and counts every call.
    #[derive(Default)]
    struct FakeSecrets {
        entries: Mutex<HashMap<String, Vec<u8>>>,
        failing_writes: AtomicUsize,
        corrupt_reads: bool,
        calls: AtomicUsize,
    }

    impl FakeSecrets {
        fn failing_next_writes(count: usize) -> Self {
            let fake = Self::default();
            fake.failing_writes.store(count, Ordering::SeqCst);
            fake
        }
        fn stored(&self) -> Option<StoredLicense> {
            let entries = self.entries.lock_ignore_poison();
            entries
                .get(SECRET_KEY)
                .map(|bytes| serde_json::from_slice(bytes).expect("the fake holds only what save wrote"))
        }
    }

    impl SecretStore for FakeSecrets {
        fn set(&self, key: &str, value: &[u8]) -> Result<(), SecretStoreError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self
                .failing_writes
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_ok()
            {
                return Err(SecretStoreError::AccessDenied("user denied".to_string()));
            }
            self.entries
                .lock_ignore_poison()
                .insert(key.to_string(), value.to_vec());
            Ok(())
        }
        fn get(&self, key: &str) -> Result<Vec<u8>, SecretStoreError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.corrupt_reads {
                return Ok(b"something else".to_vec());
            }
            self.entries
                .lock_ignore_poison()
                .get(key)
                .cloned()
                .ok_or_else(|| SecretStoreError::NotFound(key.to_string()))
        }
        fn delete(&self, key: &str) -> Result<(), SecretStoreError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.entries
                .lock_ignore_poison()
                .remove(key)
                .map(|_| ())
                .ok_or_else(|| SecretStoreError::NotFound(key.to_string()))
        }
        fn keys(&self) -> Result<Vec<String>, SecretStoreError> {
            Ok(self.entries.lock_ignore_poison().keys().cloned().collect())
        }
    }

    const KEY: &str = "eyJlbWFpbCI6ImJ1eWVyQGV4YW1wbGUuY29tIn0=.c2lnbmF0dXJl";

    fn file_from_an_older_version() -> MapFile {
        let file = MapFile::default();
        file.set(FILE_KEY_LEGACY_LICENSE, serde_json::json!(KEY));
        file.set(FILE_KEY_LEGACY_SHORT_CODE, serde_json::json!("CMDR-AAAA-BBBB-CCCC"));
        file.set("cached_license_status", serde_json::json!({ "status": "active" }));
        file
    }

    fn the_license() -> StoredLicense {
        StoredLicense {
            license_key: KEY.to_string(),
            short_code: Some("CMDR-AAAA-BBBB-CCCC".to_string()),
        }
    }

    #[test]
    fn migration_moves_the_key_out_of_the_file_into_the_secret_store() {
        let file = file_from_an_older_version();
        let secrets = FakeSecrets::default();

        assert_eq!(migrate(&file, &secrets), MigrationOutcome::Migrated);

        assert_eq!(secrets.stored(), Some(the_license()));
        assert_eq!(
            file.get(FILE_KEY_LEGACY_LICENSE),
            None,
            "the plaintext key must leave the file"
        );
        assert_eq!(file.get(FILE_KEY_LEGACY_SHORT_CODE), None);
        assert_eq!(file.get(FILE_KEY_IN_SECRET_STORE), Some(serde_json::json!(true)));
        assert!(
            file.get("cached_license_status").is_some(),
            "non-secret state stays in the file"
        );
        assert_eq!(load(&file, &secrets).unwrap(), Some(the_license()));
    }

    #[test]
    fn migration_keeps_the_file_untouched_when_the_secret_store_refuses_the_write() {
        let file = file_from_an_older_version();
        let before = file.snapshot();
        let secrets = FakeSecrets::failing_next_writes(1);

        assert!(matches!(migrate(&file, &secrets), MigrationOutcome::KeptInFile(_)));

        assert_eq!(file.snapshot(), before, "a failed migration must not touch the file");
        assert_eq!(
            load(&file, &secrets).unwrap(),
            Some(the_license()),
            "the license still works from the file"
        );
    }

    #[test]
    fn migration_keeps_the_file_when_the_read_back_differs() {
        let file = file_from_an_older_version();
        let before = file.snapshot();
        let secrets = FakeSecrets {
            corrupt_reads: true,
            ..FakeSecrets::default()
        };

        assert!(matches!(migrate(&file, &secrets), MigrationOutcome::KeptInFile(_)));
        assert_eq!(file.snapshot(), before);
    }

    #[test]
    fn a_failed_migration_succeeds_on_the_next_launch() {
        let file = file_from_an_older_version();
        let secrets = FakeSecrets::failing_next_writes(1);

        assert!(matches!(migrate(&file, &secrets), MigrationOutcome::KeptInFile(_)));
        assert_eq!(migrate(&file, &secrets), MigrationOutcome::Migrated);

        assert_eq!(secrets.stored(), Some(the_license()));
        assert_eq!(file.get(FILE_KEY_LEGACY_LICENSE), None);
    }

    #[test]
    fn migration_without_a_key_in_the_file_never_touches_the_secret_store() {
        let file = MapFile::default();
        let secrets = FakeSecrets::default();

        assert_eq!(migrate(&file, &secrets), MigrationOutcome::NothingToMigrate);
        assert_eq!(secrets.calls.load(Ordering::SeqCst), 0);
    }

    /// A Personal install has no marker, so status checks (which run often) never reach the
    /// Keychain, and never risk a Keychain prompt.
    #[test]
    fn loading_without_a_license_never_touches_the_secret_store() {
        let file = MapFile::default();
        let secrets = FakeSecrets::default();

        assert_eq!(load(&file, &secrets).unwrap(), None);
        assert_eq!(secrets.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn save_puts_the_key_in_the_secret_store_and_none_in_the_file() {
        let file = MapFile::default();
        let secrets = FakeSecrets::default();

        save(&file, &secrets, &the_license()).unwrap();

        assert_eq!(secrets.stored(), Some(the_license()));
        assert_eq!(file.get(FILE_KEY_LEGACY_LICENSE), None);
        assert_eq!(file.get(FILE_KEY_IN_SECRET_STORE), Some(serde_json::json!(true)));
        assert_eq!(load(&file, &secrets).unwrap(), Some(the_license()));
    }

    #[test]
    fn a_failed_save_changes_nothing() {
        let file = MapFile::default();
        let secrets = FakeSecrets::failing_next_writes(1);

        assert!(save(&file, &secrets, &the_license()).is_err());
        assert!(file.snapshot().is_empty());
    }

    /// After a downgrade, an older version can only have written its key to the file, so that's
    /// the newer one.
    #[test]
    fn a_key_back_in_the_file_wins_over_the_secret_store() {
        let file = MapFile::default();
        let secrets = FakeSecrets::default();
        save(
            &file,
            &secrets,
            &StoredLicense {
                license_key: "older.key".to_string(),
                short_code: None,
            },
        )
        .unwrap();
        file.set(FILE_KEY_LEGACY_LICENSE, serde_json::json!(KEY));
        file.set(FILE_KEY_LEGACY_SHORT_CODE, serde_json::json!("CMDR-AAAA-BBBB-CCCC"));

        assert_eq!(load(&file, &secrets).unwrap(), Some(the_license()));
        assert_eq!(migrate(&file, &secrets), MigrationOutcome::Migrated);
        assert_eq!(secrets.stored(), Some(the_license()));
    }

    #[test]
    fn a_marker_with_no_secret_reads_as_no_license() {
        let file = MapFile::default();
        file.set(FILE_KEY_IN_SECRET_STORE, serde_json::json!(true));

        assert_eq!(load(&file, &FakeSecrets::default()).unwrap(), None);
    }

    #[test]
    fn a_recovered_file_gets_its_marker_back_when_the_secret_store_still_holds_the_license() {
        let secrets = FakeSecrets::default();
        save(&MapFile::default(), &secrets, &the_license()).unwrap();
        let fresh_file = MapFile::default();
        secrets.calls.store(0, Ordering::SeqCst);

        assert_eq!(restore_marker(&fresh_file, &secrets, true), MarkerRestore::Restored);

        assert_eq!(
            secrets.calls.load(Ordering::SeqCst),
            1,
            "one read of the secret store, nothing more"
        );
        assert_eq!(fresh_file.get(FILE_KEY_IN_SECRET_STORE), Some(serde_json::json!(true)));
        assert_eq!(load(&fresh_file, &secrets).unwrap(), Some(the_license()));
    }

    #[test]
    fn a_recovered_file_stays_unlicensed_when_the_secret_store_holds_no_license() {
        let fresh_file = MapFile::default();
        let secrets = FakeSecrets::default();

        assert_eq!(restore_marker(&fresh_file, &secrets, true), MarkerRestore::NoKeyStored);

        assert_eq!(secrets.calls.load(Ordering::SeqCst), 1);
        assert!(fresh_file.snapshot().is_empty(), "no marker for a key that isn't there");
        assert_eq!(load(&fresh_file, &secrets).unwrap(), None);
    }

    #[test]
    fn a_secret_store_entry_that_isnt_a_license_restores_no_marker() {
        let fresh_file = MapFile::default();
        let secrets = FakeSecrets::default();
        secrets
            .entries
            .lock_ignore_poison()
            .insert(SECRET_KEY.to_string(), b"not json".to_vec());

        assert!(matches!(
            restore_marker(&fresh_file, &secrets, true),
            MarkerRestore::Unreadable(_)
        ));
        assert!(fresh_file.snapshot().is_empty());
    }

    /// The probe is for the launch after a recovery only: every other launch of a Personal
    /// install still never touches the Keychain.
    #[test]
    fn a_launch_without_a_recovery_never_probes_the_secret_store() {
        let secrets = FakeSecrets::default();
        save(&MapFile::default(), &secrets, &the_license()).unwrap();
        secrets.calls.store(0, Ordering::SeqCst);

        assert_eq!(
            restore_marker(&MapFile::default(), &secrets, false),
            MarkerRestore::NotNeeded
        );
        assert_eq!(secrets.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn clear_removes_the_license_from_both_places() {
        let file = file_from_an_older_version();
        let secrets = FakeSecrets::default();
        save(&file, &secrets, &the_license()).unwrap();
        file.set(FILE_KEY_LEGACY_LICENSE, serde_json::json!(KEY));

        clear(&file, &secrets);

        assert_eq!(secrets.stored(), None);
        assert_eq!(file.get(FILE_KEY_LEGACY_LICENSE), None);
        assert_eq!(file.get(FILE_KEY_LEGACY_SHORT_CODE), None);
        assert_eq!(file.get(FILE_KEY_IN_SECRET_STORE), None);
        assert_eq!(load(&file, &secrets).unwrap(), None);
    }
}
