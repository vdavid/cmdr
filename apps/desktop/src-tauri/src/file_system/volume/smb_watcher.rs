//! Background SMB change watcher using a dedicated smb2 connection.
//!
//! Monitors a share for external changes via `CHANGE_NOTIFY`, debounces
//! events, and feeds them into `notify_directory_changed`. Runs as a
//! `tokio::spawn`ed task with its own smb2 session (separate from the
//! main `SmbVolume` connection).

use crate::file_system::listing::FileEntry;
use crate::file_system::listing::caching::{DirectoryChange, notify_directory_changed};
use log::{debug, info, warn};
use smb2::client::tree::Tree;
use smb2::{ClientConfig, FileNotifyAction, SmbClient};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use unicode_normalization::UnicodeNormalization;

/// Maximum events for a single directory before emitting `FullRefresh`.
const WATCHER_BATCH_THRESHOLD: usize = 50;

/// Debounce window: after receiving a batch of events, wait this long for more.
const WATCHER_DEBOUNCE: Duration = Duration::from_millis(200);

/// Delay between reconnection attempts when the watcher connection drops.
const WATCHER_RECONNECT_DELAY: Duration = Duration::from_secs(5);

/// Maximum reconnection attempts before giving up.
const WATCHER_MAX_RECONNECT_ATTEMPTS: u32 = 3;

/// Runs a background SMB change watcher on a dedicated smb2 connection.
///
/// Establishes its own connection to the same server/share and uses
/// `CHANGE_NOTIFY` to detect external changes. Events are debounced,
/// converted to `DirectoryChange`, and fed into `notify_directory_changed`.
///
/// The task exits when `cancel_rx` fires, the connection is permanently lost,
/// or an unrecoverable error occurs.
pub(super) async fn run_smb_watcher(
    addr: String,
    share_name: String,
    username: String,
    password: String,
    volume_id: String,
    mount_path: PathBuf,
    cancel_rx: tokio::sync::oneshot::Receiver<()>,
) {
    /// Establishes a watcher connection and returns (client, tree).
    async fn connect_watcher(
        addr: &str,
        share_name: &str,
        username: &str,
        password: &str,
    ) -> Result<(SmbClient, Tree), smb2::Error> {
        let config = ClientConfig {
            addr: addr.to_string(),
            timeout: Duration::from_secs(10),
            username: username.to_string(),
            password: password.to_string(),
            domain: String::new(),
            auto_reconnect: false,
            compression: true,
            dfs_enabled: false,
            dfs_target_overrides: Default::default(),
        };
        let mut client = SmbClient::connect(config).await?;
        let tree = client.connect_share(share_name).await?;
        Ok((client, tree))
    }

    /// Converts a watcher filename (NFC from server) to an NFD display path
    /// suitable for macOS mount paths.
    fn to_nfd_display_path(mount_path: &Path, relative: &str) -> PathBuf {
        let nfd: String = relative.nfd().collect();
        if nfd.is_empty() {
            mount_path.to_path_buf()
        } else {
            mount_path.join(&nfd)
        }
    }

    /// Processes a batch of collected events per directory into `DirectoryChange` notifications.
    ///
    /// Runs on a blocking thread (via `spawn_blocking`) because both `stat_via_volume`
    /// and `notify_directory_changed(FullRefresh)` call `Volume::list_directory` which
    /// uses `Handle::block_on` — that panics if called from an async task.
    fn process_event_batch(
        events_by_dir: HashMap<PathBuf, Vec<(FileNotifyAction, String)>>,
        volume_id: &str,
        mount_path: &Path,
    ) {
        for (parent_path, events) in &events_by_dir {
            if events.len() > WATCHER_BATCH_THRESHOLD {
                debug!(
                    "smb_watcher: {} events for {} — emitting FullRefresh",
                    events.len(),
                    parent_path.display()
                );
                notify_directory_changed(volume_id, parent_path, DirectoryChange::FullRefresh);
                continue;
            }

            let mut pending_old_name: Option<String> = None;

            for (action, filename) in events {
                let file_name_only: String = Path::new(filename)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| filename.clone());

                // Skip macOS safe-save temp files (like "file.txt.sb-1e64c894-vFWIzN").
                // These are transient artifacts from TextEdit/Preview/etc. that create a
                // temp dir, write the new version, then atomically swap. Showing them in
                // the listing confuses users. Controlled by advanced.filterSafeSaveArtifacts.
                if crate::file_system::is_filter_safe_save_artifacts_enabled() && file_name_only.contains(".sb-") {
                    continue;
                }

                match action {
                    FileNotifyAction::Added => {
                        let entry_path = to_nfd_display_path(mount_path, filename);
                        match stat_via_volume(volume_id, &entry_path) {
                            Some(entry) => {
                                notify_directory_changed(volume_id, parent_path, DirectoryChange::Added(entry));
                            }
                            None => {
                                debug!(
                                    "smb_watcher: couldn't stat added file {}, skipping",
                                    entry_path.display()
                                );
                            }
                        }
                    }
                    FileNotifyAction::Removed => {
                        notify_directory_changed(volume_id, parent_path, DirectoryChange::Removed(file_name_only));
                    }
                    FileNotifyAction::Modified => {
                        let entry_path = to_nfd_display_path(mount_path, filename);
                        match stat_via_volume(volume_id, &entry_path) {
                            Some(entry) => {
                                notify_directory_changed(volume_id, parent_path, DirectoryChange::Modified(entry));
                            }
                            None => {
                                debug!(
                                    "smb_watcher: couldn't stat modified file {}, skipping",
                                    entry_path.display()
                                );
                            }
                        }
                    }
                    FileNotifyAction::RenamedOldName => {
                        pending_old_name = Some(file_name_only);
                    }
                    FileNotifyAction::RenamedNewName => {
                        let entry_path = to_nfd_display_path(mount_path, filename);
                        if let Some(old_name) = pending_old_name.take() {
                            match stat_via_volume(volume_id, &entry_path) {
                                Some(new_entry) => {
                                    notify_directory_changed(
                                        volume_id,
                                        parent_path,
                                        DirectoryChange::Renamed { old_name, new_entry },
                                    );
                                }
                                None => {
                                    // Couldn't stat new name — emit remove + skip add
                                    notify_directory_changed(
                                        volume_id,
                                        parent_path,
                                        DirectoryChange::Removed(old_name),
                                    );
                                }
                            }
                        } else {
                            // Got new name without old name — treat as add
                            if let Some(entry) = stat_via_volume(volume_id, &entry_path) {
                                notify_directory_changed(volume_id, parent_path, DirectoryChange::Added(entry));
                            }
                        }
                    }
                }
            }

            // If we have a dangling old name with no new name, treat as remove
            if let Some(old_name) = pending_old_name {
                notify_directory_changed(volume_id, parent_path, DirectoryChange::Removed(old_name));
            }
        }
    }

    /// Stats a file via the main SmbVolume connection (through VolumeManager).
    ///
    /// Must be called from a blocking thread (not an async task), because
    /// `SmbVolume::get_metadata` uses `Handle::block_on` internally.
    fn stat_via_volume(volume_id: &str, path: &Path) -> Option<FileEntry> {
        let vm = crate::file_system::get_volume_manager();
        let vol = vm.get(volume_id)?;
        tokio::runtime::Handle::current().block_on(vol.get_metadata(path)).ok()
    }

    // ── Main watcher loop ──────────────────────────────────────────

    let mut cancel_rx = cancel_rx;
    let mut reconnect_attempts = 0u32;

    'outer: loop {
        // Establish the dedicated watcher connection
        let (mut client, tree) = match connect_watcher(&addr, &share_name, &username, &password).await {
            Ok(pair) => {
                if reconnect_attempts > 0 {
                    info!(
                        "smb_watcher({}): reconnected after {} attempt(s)",
                        share_name, reconnect_attempts
                    );
                } else {
                    info!("smb_watcher({}): connected, starting watch", share_name);
                }
                reconnect_attempts = 0;
                pair
            }
            Err(e) => {
                reconnect_attempts += 1;
                if reconnect_attempts > WATCHER_MAX_RECONNECT_ATTEMPTS {
                    warn!(
                        "smb_watcher({}): failed to connect after {} attempts, giving up: {}",
                        share_name, WATCHER_MAX_RECONNECT_ATTEMPTS, e
                    );
                    return;
                }
                warn!(
                    "smb_watcher({}): connection failed (attempt {}/{}): {}, retrying in {:?}",
                    share_name, reconnect_attempts, WATCHER_MAX_RECONNECT_ATTEMPTS, e, WATCHER_RECONNECT_DELAY
                );
                tokio::select! {
                    _ = tokio::time::sleep(WATCHER_RECONNECT_DELAY) => continue 'outer,
                    _ = &mut cancel_rx => {
                        debug!("smb_watcher({}): cancelled during reconnect wait", share_name);
                        return;
                    }
                }
            }
        };

        // Start watching from the share root (recursive)
        let mut watcher = match client.watch(&tree, "", true).await {
            Ok(w) => w,
            Err(e) => {
                warn!("smb_watcher({}): failed to start watch: {}", share_name, e);
                reconnect_attempts += 1;
                if reconnect_attempts > WATCHER_MAX_RECONNECT_ATTEMPTS {
                    warn!("smb_watcher({}): giving up after repeated watch failures", share_name);
                    return;
                }
                tokio::select! {
                    _ = tokio::time::sleep(WATCHER_RECONNECT_DELAY) => continue 'outer,
                    _ = &mut cancel_rx => {
                        debug!("smb_watcher({}): cancelled during watch retry wait", share_name);
                        return;
                    }
                }
            }
        };

        // Event loop
        loop {
            let events_result = tokio::select! {
                result = watcher.next_events() => result,
                _ = &mut cancel_rx => {
                    debug!("smb_watcher({}): cancelled, closing watcher", share_name);
                    if let Err(e) = watcher.close().await {
                        debug!("smb_watcher({}): error closing watcher: {}", share_name, e);
                    }
                    return;
                }
            };

            match events_result {
                Ok(events) => {
                    // Collect events by parent directory, debouncing with a short wait
                    let mut events_by_dir: HashMap<PathBuf, Vec<(FileNotifyAction, String)>> = HashMap::new();

                    for event in &events {
                        // SMB watcher filenames use backslashes; normalize to forward slashes
                        let normalized_filename = event.filename.replace('\\', "/");
                        let parent = Path::new(&normalized_filename)
                            .parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let parent_display = to_nfd_display_path(&mount_path, &parent);

                        events_by_dir
                            .entry(parent_display)
                            .or_default()
                            .push((event.action, normalized_filename));
                    }

                    // Debounce: wait briefly for more events in the same batch
                    loop {
                        let more = tokio::select! {
                            result = tokio::time::timeout(WATCHER_DEBOUNCE, watcher.next_events()) => {
                                match result {
                                    Ok(Ok(more_events)) => Some(more_events),
                                    Ok(Err(_)) => None,
                                    Err(_) => None, // timeout — done debouncing
                                }
                            },
                            _ = &mut cancel_rx => {
                                // Process what we have, then exit
                                {
                                    let vid = volume_id.clone();
                                    let mp = mount_path.clone();
                                    let _ = tokio::task::spawn_blocking(move || {
                                        process_event_batch(events_by_dir, &vid, &mp);
                                    }).await;
                                }
                                debug!("smb_watcher({}): cancelled during debounce, closing", share_name);
                                if let Err(e) = watcher.close().await {
                                    debug!("smb_watcher({}): error closing watcher: {}", share_name, e);
                                }
                                return;
                            }
                        };

                        match more {
                            Some(more_events) => {
                                for event in &more_events {
                                    let normalized_filename = event.filename.replace('\\', "/");
                                    let parent = Path::new(&normalized_filename)
                                        .parent()
                                        .map(|p| p.to_string_lossy().to_string())
                                        .unwrap_or_default();
                                    let parent_display = to_nfd_display_path(&mount_path, &parent);

                                    events_by_dir
                                        .entry(parent_display)
                                        .or_default()
                                        .push((event.action, normalized_filename));
                                }
                            }
                            None => break, // timeout or error — process batch
                        }
                    }

                    let total_events: usize = events_by_dir.values().map(|v| v.len()).sum();
                    debug!(
                        "smb_watcher({}): processing {} event(s) across {} dir(s)",
                        share_name,
                        total_events,
                        events_by_dir.len()
                    );

                    {
                        let vid = volume_id.clone();
                        let mp = mount_path.clone();
                        let _ = tokio::task::spawn_blocking(move || {
                            process_event_batch(events_by_dir, &vid, &mp);
                        })
                        .await;
                    }
                }
                Err(e) => {
                    // Check for STATUS_NOTIFY_ENUM_DIR (buffer overflow)
                    let is_enum_dir = matches!(
                        &e,
                        smb2::Error::Protocol { status, .. }
                            if *status == smb2::types::status::NtStatus::NOTIFY_ENUM_DIR
                    );

                    if is_enum_dir {
                        debug!(
                            "smb_watcher({}): STATUS_NOTIFY_ENUM_DIR — emitting FullRefresh for share root",
                            share_name
                        );
                        notify_directory_changed(&volume_id, &mount_path, DirectoryChange::FullRefresh);
                        // Continue watching — the server is still alive
                        continue;
                    }

                    // Connection lost or other error — try to reconnect
                    warn!("smb_watcher({}): error from next_events: {}", share_name, e);

                    // Close the watcher handle (best-effort, connection may be dead)
                    let _ = watcher.close().await;

                    reconnect_attempts += 1;
                    if reconnect_attempts > WATCHER_MAX_RECONNECT_ATTEMPTS {
                        warn!("smb_watcher({}): too many errors, giving up", share_name);
                        return;
                    }

                    tokio::select! {
                        _ = tokio::time::sleep(WATCHER_RECONNECT_DELAY) => continue 'outer,
                        _ = &mut cancel_rx => {
                            debug!("smb_watcher({}): cancelled during error reconnect wait", share_name);
                            return;
                        }
                    }
                }
            }
        }
    }
}
