//! Carrying out a plan: the secrets first, then every target, then an honest list of what's
//! still there. Pure apart from the filesystem, so it's tested against a scratch dir and a fake
//! secret store.
//!
//! **Crash safety.** A whole target is first RENAMED to a tombstone beside it
//! (`<name>.cmdr-wipe-<random>`), then the tombstone is removed. The rename is atomic, so a
//! crash at any moment leaves either the untouched original or a tombstone the app never opens:
//! never a half-deleted data dir that a relaunch would read (a SQLite file without its `-wal`,
//! a settings file without the store it points at). [`sweep_tombstones`] finishes an
//! interrupted wipe on the next launch.

use std::path::{Path, PathBuf};

use cmdr_fs::log_private::LogPrivate;

use crate::secrets::{SecretStore, SecretStoreError};

use super::plan::{TargetKind, WipeTarget};

/// The marker in a tombstone's name. Nothing else in Cmdr's folders carries it.
pub(super) const TOMBSTONE_MARKER: &str = ".cmdr-wipe-";

/// What the wipe couldn't remove, in terms the dialog can point a person at.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WipeLeftover {
    /// A folder or file that's still on disk, to delete in Finder.
    Path { path: String },
    /// Saved passwords or keys still in the OS store, under this item name.
    SecretItems { count: u32, item_name: String },
    /// The OS store didn't say what it holds, so some items may remain under this name.
    SecretsUnreadable { item_name: String },
}

/// The outcome of a wipe. Empty `leftovers` means everything went.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WipeReport {
    pub leftovers: Vec<WipeLeftover>,
}

/// The parts of a wipe that reach outside the filesystem and the secret store.
pub trait WipeHost {
    /// Drop this identifier's preferences through the OS (macOS: `defaults delete`), so a
    /// preferences daemon holding them in memory doesn't write the file straight back.
    fn forget_preferences(&self, identifier: &str);
}

/// Remove every secret, then every target, and report what's left. Never stops early: one
/// folder that won't go doesn't keep the others.
pub fn wipe(
    plan: &[WipeTarget],
    identifier: &str,
    secrets: &dyn SecretStore,
    secrets_item_name: &str,
    host: &dyn WipeHost,
) -> WipeReport {
    delete_every_secret(secrets);
    if plan.iter().any(|t| t.kind == TargetKind::Preferences) {
        host.forget_preferences(identifier);
    }

    let mut leftovers: Vec<WipeLeftover> = plan.iter().filter_map(remove_target).collect();

    // Asked again at the end, so the answer is what's really left: a file store's items went
    // with the data dir, and an item that refused a delete is still listed.
    match secrets.keys() {
        Ok(keys) if keys.is_empty() => {}
        Ok(keys) => leftovers.push(WipeLeftover::SecretItems {
            count: u32::try_from(keys.len()).unwrap_or(u32::MAX),
            item_name: secrets_item_name.to_string(),
        }),
        Err(e) => {
            log::warn!(target: "data_wipe", "couldn't list the secret store after the wipe: {e}");
            leftovers.push(WipeLeftover::SecretsUnreadable {
                item_name: secrets_item_name.to_string(),
            });
        }
    }
    WipeReport { leftovers }
}

fn delete_every_secret(secrets: &dyn SecretStore) {
    let keys = match secrets.keys() {
        Ok(keys) => keys,
        Err(e) => {
            log::warn!(target: "data_wipe", "couldn't list the secret store: {e}");
            return;
        }
    };
    let mut deleted = 0usize;
    for key in &keys {
        match secrets.delete(key) {
            Ok(()) | Err(SecretStoreError::NotFound(_)) => deleted += 1,
            // The key itself names a server or a provider, so it stays out of the log.
            Err(e) => log::warn!(target: "data_wipe", "a secret didn't delete: {e}"),
        }
    }
    log::info!(target: "data_wipe", "deleted {deleted} of {} secret(s)", keys.len());
}

/// Remove one target; `Some` names what's still on disk afterwards.
fn remove_target(target: &WipeTarget) -> Option<WipeLeftover> {
    remove_whole(&target.path).map(|path| {
        log::warn!(target: "data_wipe", "{:?} couldn't be removed: {}", target.kind, LogPrivate::path(&path));
        WipeLeftover::Path {
            path: path.to_string_lossy().into_owned(),
        }
    })
}

/// Rename to a tombstone, then remove the tombstone. Falls back to removing in place when the
/// rename itself is refused. Returns the path that's still there, if any.
fn remove_whole(path: &Path) -> Option<PathBuf> {
    if !exists(path) {
        return None;
    }
    let doomed = match tombstone_for(path) {
        Some(tombstone) if std::fs::rename(path, &tombstone).is_ok() => tombstone,
        _ => path.to_path_buf(),
    };
    if let Err(e) = remove_any(&doomed) {
        log::warn!(target: "data_wipe", "removing {} stopped: {e}", LogPrivate::path(&doomed));
    }
    if exists(&doomed) {
        return Some(doomed);
    }
    // Something wrote the original back between the rename and now. Removing it again is the
    // whole job, and the exit sweep gets a third go at it.
    if exists(path) && remove_any(path).is_err() {
        return Some(path.to_path_buf());
    }
    None
}

/// `<parent>/<name>.cmdr-wipe-<random>`: beside the original, so the rename never crosses a
/// volume, and hidden from nothing (a person looking for leftovers should find it).
fn tombstone_for(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    let name = path.file_name()?.to_string_lossy();
    Some(parent.join(format!("{name}{TOMBSTONE_MARKER}{}", uuid::Uuid::new_v4().simple())))
}

/// Removes leftover tombstones of the plan's targets: what a wipe that crashed halfway left
/// behind. Returns how many it removed.
pub fn sweep_tombstones(plan: &[WipeTarget]) -> usize {
    let mut removed = 0;
    for target in plan {
        let (Some(parent), Some(name)) = (target.path.parent(), target.path.file_name()) else {
            continue;
        };
        let prefix = format!("{}{TOMBSTONE_MARKER}", name.to_string_lossy());
        let Ok(entries) = std::fs::read_dir(parent) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(&prefix) && remove_any(&entry.path()).is_ok() {
                removed += 1;
            }
        }
    }
    removed
}

/// Removes each target in place, without reporting: the last pass at exit, for what the app's
/// own shutdown wrote back after the wipe (stores saved on exit, WebKit's caches).
pub fn remove_again(plan: &[WipeTarget]) {
    for target in plan {
        if exists(&target.path) {
            let _ = remove_any(&target.path);
        }
    }
}

fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// A symlink is removed as a link, never followed: a link to the person's own folder must not
/// take that folder.
fn remove_any(path: &Path) -> std::io::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;
    use crate::ignore_poison::IgnorePoison;
    use crate::test_support::TestDir;

    /// An in-memory secret store, with keys that refuse to delete and a listing that can fail.
    #[derive(Default)]
    struct FakeSecrets {
        entries: Mutex<HashMap<String, Vec<u8>>>,
        undeletable: Vec<String>,
        listing_fails: bool,
    }

    impl FakeSecrets {
        fn holding(keys: &[&str]) -> Self {
            let fake = Self::default();
            for key in keys {
                fake.entries
                    .lock_ignore_poison()
                    .insert((*key).to_string(), b"x".to_vec());
            }
            fake
        }
    }

    impl SecretStore for FakeSecrets {
        fn set(&self, key: &str, value: &[u8]) -> Result<(), SecretStoreError> {
            self.entries
                .lock_ignore_poison()
                .insert(key.to_string(), value.to_vec());
            Ok(())
        }
        fn get(&self, key: &str) -> Result<Vec<u8>, SecretStoreError> {
            self.entries
                .lock_ignore_poison()
                .get(key)
                .cloned()
                .ok_or_else(|| SecretStoreError::NotFound(key.to_string()))
        }
        fn delete(&self, key: &str) -> Result<(), SecretStoreError> {
            if self.undeletable.iter().any(|k| k == key) {
                return Err(SecretStoreError::AccessDenied("the user said no".to_string()));
            }
            self.entries
                .lock_ignore_poison()
                .remove(key)
                .map(|_| ())
                .ok_or_else(|| SecretStoreError::NotFound(key.to_string()))
        }
        fn keys(&self) -> Result<Vec<String>, SecretStoreError> {
            if self.listing_fails {
                return Err(SecretStoreError::AccessDenied("locked".to_string()));
            }
            Ok(self.entries.lock_ignore_poison().keys().cloned().collect())
        }
    }

    #[derive(Default)]
    struct FakeHost {
        forgotten: Mutex<Vec<String>>,
    }

    impl WipeHost for FakeHost {
        fn forget_preferences(&self, identifier: &str) {
            self.forgotten.lock_ignore_poison().push(identifier.to_string());
        }
    }

    fn whole(kind: TargetKind, path: PathBuf) -> WipeTarget {
        WipeTarget { kind, path }
    }

    /// A data dir the way the app leaves it: a database with its WAL, a settings file, and a
    /// nested folder.
    fn seed_data_dir(dir: &Path) {
        std::fs::create_dir_all(dir.join("ai").join("memory")).unwrap();
        std::fs::write(dir.join("main.db"), b"db").unwrap();
        std::fs::write(dir.join("main.db-wal"), b"wal").unwrap();
        std::fs::write(dir.join("settings.json"), b"{}").unwrap();
        std::fs::write(dir.join("ai").join("memory").join("note.md"), b"note").unwrap();
    }

    #[test]
    fn everything_goes_and_the_report_is_empty() {
        let root = TestDir::new("wipe-all");
        let data = root.join("Application Support").join("cmdr");
        let prefs = root.join("Preferences").join("cmdr.plist");
        seed_data_dir(&data);
        std::fs::create_dir_all(prefs.parent().unwrap()).unwrap();
        std::fs::write(&prefs, b"plist").unwrap();
        let plan = vec![
            whole(TargetKind::AppData, data.clone()),
            whole(TargetKind::Preferences, prefs.clone()),
            // Not there at all: nothing to do, and nothing to report.
            whole(TargetKind::WebKit, root.join("WebKit").join("cmdr")),
        ];
        let secrets = FakeSecrets::holding(&["license.key", "ai.apiKey.openai", "smb://nas/share"]);
        let host = FakeHost::default();

        let report = wipe(&plan, "cmdr", &secrets, "Cmdr", &host);

        assert_eq!(report, WipeReport::default());
        assert!(!data.exists());
        assert!(!prefs.exists());
        assert!(secrets.keys().unwrap().is_empty(), "every secret is deleted");
        assert_eq!(*host.forgotten.lock_ignore_poison(), vec!["cmdr".to_string()]);
        let siblings: Vec<_> = std::fs::read_dir(root.join("Application Support")).unwrap().collect();
        assert!(siblings.is_empty(), "no tombstone is left behind");
    }

    /// One folder that won't go doesn't stop the rest, and the report names exactly what's left.
    #[test]
    fn a_folder_that_refuses_is_reported_and_the_rest_still_goes() {
        use std::os::unix::fs::PermissionsExt;
        // Root ignores the permission bits this folder relies on to refuse (the Linux Docker lane runs as root).
        // SAFETY: (test) `geteuid` takes no arguments, shares no memory, and can't fail; it only reads this
        // process's effective uid.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let root = TestDir::new("wipe-partial");
        let stuck_parent = root.join("Logs");
        let stuck = stuck_parent.join("cmdr");
        std::fs::create_dir_all(&stuck).unwrap();
        std::fs::write(stuck.join("cmdr.log"), b"log").unwrap();
        // A folder without write permission keeps its entries: it renames to a tombstone, which
        // then can't be emptied.
        std::fs::set_permissions(&stuck, std::fs::Permissions::from_mode(0o500)).unwrap();
        let data = root.join("data");
        seed_data_dir(&data);
        let plan = vec![
            whole(TargetKind::Logs, stuck.clone()),
            whole(TargetKind::AppData, data.clone()),
        ];
        let secrets = FakeSecrets::default();

        let report = wipe(&plan, "cmdr", &secrets, "Cmdr", &FakeHost::default());

        assert!(!data.exists(), "the data dir still went");
        assert_eq!(report.leftovers.len(), 1, "{report:?}");
        let WipeLeftover::Path { path } = &report.leftovers[0] else {
            panic!("expected a path leftover, got {report:?}");
        };
        // Writable again, so the scratch dir can clean itself up.
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(
            Path::new(path).starts_with(&stuck_parent),
            "{path} should point at where the logs are"
        );
    }

    #[test]
    fn a_secret_that_refuses_is_counted_under_its_item_name() {
        let mut secrets = FakeSecrets::holding(&["license.key", "smb://nas/share", "ai.apiKey.openai"]);
        secrets.undeletable = vec!["smb://nas/share".to_string()];

        let report = wipe(&[], "cmdr", &secrets, "Cmdr-dev", &FakeHost::default());

        assert_eq!(
            report.leftovers,
            vec![WipeLeftover::SecretItems {
                count: 1,
                item_name: "Cmdr-dev".to_string()
            }]
        );
    }

    #[test]
    fn a_store_that_wont_list_its_items_is_reported_as_unreadable() {
        let mut secrets = FakeSecrets::holding(&["license.key"]);
        secrets.listing_fails = true;

        let report = wipe(&[], "cmdr", &secrets, "Cmdr", &FakeHost::default());

        assert_eq!(
            report.leftovers,
            vec![WipeLeftover::SecretsUnreadable {
                item_name: "Cmdr".to_string()
            }]
        );
    }

    /// A symlink inside the data dir is removed as a link: what it points at is the person's.
    #[test]
    fn a_symlink_inside_a_target_never_takes_what_it_points_at() {
        let root = TestDir::new("wipe-symlink");
        let precious = root.join("precious");
        std::fs::create_dir_all(&precious).unwrap();
        std::fs::write(precious.join("photo.jpg"), b"jpg").unwrap();
        let data = root.join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::os::unix::fs::symlink(&precious, data.join("link")).unwrap();
        let link_target = root.join("link-target");
        std::os::unix::fs::symlink(&precious, &link_target).unwrap();

        let plan = vec![
            whole(TargetKind::AppData, data.clone()),
            whole(TargetKind::WebKit, link_target.clone()),
        ];
        let report = wipe(&plan, "cmdr", &FakeSecrets::default(), "Cmdr", &FakeHost::default());

        assert_eq!(report, WipeReport::default());
        assert!(!data.exists() && !exists(&link_target));
        assert!(precious.join("photo.jpg").exists(), "the linked folder survives");
    }

    /// A wipe that crashed after the rename leaves a tombstone; the next launch removes it, and
    /// only it.
    #[test]
    fn a_tombstone_from_an_interrupted_wipe_is_swept() {
        let root = TestDir::new("wipe-tombstone");
        let data = root.join("cmdr");
        let tombstone = root.join(format!("cmdr{TOMBSTONE_MARKER}0123abcd"));
        seed_data_dir(&tombstone);
        std::fs::create_dir_all(&data).unwrap();
        let neighbor = root.join("com.other.app");
        std::fs::create_dir_all(&neighbor).unwrap();

        let removed = sweep_tombstones(&[whole(TargetKind::AppData, data.clone())]);

        assert_eq!(removed, 1);
        assert!(!tombstone.exists());
        assert!(data.exists(), "the live data dir isn't a tombstone");
        assert!(neighbor.exists());
    }

    #[test]
    fn remove_again_clears_what_shutdown_wrote_back() {
        let root = TestDir::new("wipe-again");
        let data = root.join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("settings.json"), b"{}").unwrap();

        remove_again(&[whole(TargetKind::AppData, data.clone())]);

        assert!(!data.exists());
    }
}
