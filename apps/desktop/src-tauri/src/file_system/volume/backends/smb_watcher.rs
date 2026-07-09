//! Background SMB change watcher.
//!
//! Long-polls `CHANGE_NOTIFY` on the share root, debounces events, and feeds
//! them into `notify_directory_changed`. Spawned by `SmbVolume::spawn_watcher`
//! with its own dedicated smb2 session (a separate TCP connection from the
//! volume's primary client), so the watcher's long-polls don't multiplex with
//! heavy concurrent writes on the main connection.
//!
//! No internal reconnect: on `next_events` errors, the task returns and
//! `SmbVolume::attempt_reconnect` is the single source of truth for
//! re-establishing the session — it respawns the watcher when it succeeds.

use crate::file_system::listing::FileEntry;
use crate::file_system::listing::caching::{DirectoryChange, notify_directory_changed, refresh_archive_listings};
use log::{debug, info, warn};
use smb2::{ClientConfig, FileNotifyAction, SmbClient};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use unicode_normalization::UnicodeNormalization;

/// Maximum events for a single directory before emitting `FullRefresh`.
const WATCHER_BATCH_THRESHOLD: usize = 50;

/// Debounce window: after receiving a batch of events, wait this long for more.
const WATCHER_DEBOUNCE: Duration = Duration::from_millis(200);

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

/// Stats a file via the main SmbVolume connection (through VolumeManager).
async fn stat_via_volume(volume_id: &str, path: &Path) -> Option<FileEntry> {
    let vm = crate::file_system::get_volume_manager();
    let vol = vm.get(volume_id)?;
    vol.get_metadata(path).await.ok()
}

/// When a changed SMB path is a supported archive, refreshes any open listing
/// INSIDE it. The recursive share watch already refreshes the directory listing
/// showing the `.zip` itself (its new size/mtime); this adds the archive-inner
/// refresh a REMOTE parent otherwise never gets — a remote `.zip` has no local
/// `notify` transport, so `archive::watch` (the local-parent equivalent) can't
/// arm. Same `refresh_archive_listings` consumer, same parent-drive `volume_id`
/// the listing cache keys archive listings on, so no rekeying.
///
/// A no-op when the path isn't an archive or no inner listing is open (the
/// refresh scans the listing cache for keys at/inside the archive path). This is
/// purely a visible-listing UX nicety and a SEPARATE consumer from the write-op
/// fresh-listing oracle: `ArchiveVolume::listing_is_watched` stays `false` for a
/// remote parent regardless, because the SMB watcher is lossy under load and the
/// oracle must keep re-reading pre-flight scans honestly.
///
/// `archive_path` must already be the normalized display path (backslash→slash,
/// NFC→NFD) the cache lookups use — pass the `to_nfd_display_path` result, the
/// same normalization every other cache-facing path in this file goes through.
async fn maybe_refresh_archive_listings(volume_id: &str, archive_path: &Path) {
    let is_archive = archive_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(super::archive::has_supported_archive_extension);
    if is_archive {
        refresh_archive_listings(volume_id, archive_path).await;
    }
}

/// Processes a batch of collected events per directory into `DirectoryChange` notifications.
async fn process_event_batch(
    events_by_dir: HashMap<PathBuf, Vec<(FileNotifyAction, String)>>,
    volume_id: &str,
    mount_path: &Path,
) {
    for (parent_path, events) in &events_by_dir {
        if events.len() > WATCHER_BATCH_THRESHOLD {
            debug!(
                "smb_watcher: {} events for {}, emitting FullRefresh",
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
                    match stat_via_volume(volume_id, &entry_path).await {
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
                    match stat_via_volume(volume_id, &entry_path).await {
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
                    // An in-place rewrite of the backing `.zip` also refreshes any
                    // open archive-inner listing (independent of the stat above,
                    // which may fail mid-write — the refresh handles a truncated
                    // archive gracefully).
                    maybe_refresh_archive_listings(volume_id, &entry_path).await;
                }
                FileNotifyAction::RenamedOldName => {
                    pending_old_name = Some(file_name_only);
                }
                FileNotifyAction::RenamedNewName => {
                    let entry_path = to_nfd_display_path(mount_path, filename);
                    if let Some(old_name) = pending_old_name.take() {
                        match stat_via_volume(volume_id, &entry_path).await {
                            Some(new_entry) => {
                                notify_directory_changed(
                                    volume_id,
                                    parent_path,
                                    DirectoryChange::Renamed { old_name, new_entry },
                                );
                            }
                            None => {
                                // Couldn't stat new name: emit remove + skip add
                                notify_directory_changed(volume_id, parent_path, DirectoryChange::Removed(old_name));
                            }
                        }
                    } else {
                        // Got new name without old name, treating as add
                        if let Some(entry) = stat_via_volume(volume_id, &entry_path).await {
                            notify_directory_changed(volume_id, parent_path, DirectoryChange::Added(entry));
                        }
                    }
                    // A temp+rename swap over the backing `.zip` (the editor /
                    // safe-overwrite path) also refreshes any open inner listing.
                    maybe_refresh_archive_listings(volume_id, &entry_path).await;
                }
            }
        }

        // If we have a dangling old name with no new name, treat as remove
        if let Some(old_name) = pending_old_name {
            notify_directory_changed(volume_id, parent_path, DirectoryChange::Removed(old_name));
        }
    }
}

/// Runs the SMB change watcher on a dedicated smb2 session.
///
/// Exits on cancel (`cancel_rx`), on `next_events` error, or on a clean
/// watcher close. On error exit, the parent `SmbVolume`'s reconnect machinery
/// picks up via the next hot-path op observing the dead session and respawns
/// this task from `attempt_reconnect`.
pub(super) async fn run_smb_watcher(
    addr: String,
    share_name: String,
    username: String,
    password: String,
    volume_id: String,
    mount_path: PathBuf,
    cancel_rx: tokio::sync::oneshot::Receiver<()>,
) {
    // ── Main watcher loop ──────────────────────────────────────────

    let mut cancel_rx = cancel_rx;

    // Establish the dedicated watcher session. We do this once: on any error,
    // we bail and let SmbVolume's reconnect machinery respawn us.
    let config = ClientConfig {
        addr: addr.clone(),
        timeout: Duration::from_secs(10),
        username,
        password,
        domain: String::new(),
        auto_reconnect: false,
        compression: true,
        dfs_enabled: false,
        dfs_target_overrides: Default::default(),
    };
    // A watcher that can't even establish its session can't keep the index
    // Fresh, so each setup-failure return flips a Fresh index Stale. Cheap
    // no-op when the volume isn't indexed or is already Stale.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let mark_stale = || crate::indexing::on_smb_watcher_died(&volume_id);
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let mark_stale = || {};

    let mut client = match SmbClient::connect(config).await {
        Ok(c) => c,
        Err(e) => {
            warn!("smb_watcher({}): connect failed: {}", share_name, e);
            mark_stale();
            return;
        }
    };
    let tree = match client.connect_share(&share_name).await {
        Ok(t) => t,
        Err(e) => {
            warn!("smb_watcher({}): tree connect failed: {}", share_name, e);
            mark_stale();
            return;
        }
    };

    // Open the watcher handle on the share root (recursive). Since smb2 0.10
    // `Watcher` is `'static` (owns a `Connection` clone of `client`'s), and
    // keeps one CHANGE_NOTIFY request pre-issued at all times so events that
    // arrive while we process the previous batch don't fall in a re-arm gap.
    let mut watcher = match client.watch(&tree, "", true).await {
        Ok(w) => {
            info!("smb_watcher({}): connected, starting watch", share_name);
            w
        }
        Err(e) => {
            warn!("smb_watcher({}): failed to start watch: {}", share_name, e);
            mark_stale();
            return;
        }
    };

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
                // Collect events by parent directory, debouncing with a short wait.
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

                // Debounce: wait briefly for more events in the same batch. The
                // smb2 0.10 watcher already keeps one CHANGE_NOTIFY pre-issued
                // on the wire, so events that arrive during this debounce
                // window land in the next response, not a server-side gap.
                // The debounce here exists only to batch FE notifications.
                loop {
                    let more = tokio::select! {
                        result = tokio::time::timeout(WATCHER_DEBOUNCE, watcher.next_events()) => {
                            match result {
                                Ok(Ok(more_events)) => Some(more_events),
                                Ok(Err(_)) => None,
                                Err(_) => None, // timeout: done debouncing
                            }
                        },
                        _ = &mut cancel_rx => {
                            // Process what we have, then exit
                            process_event_batch(events_by_dir, &volume_id, &mount_path).await;
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
                        None => break, // timeout or error: process batch
                    }
                }

                let total_events: usize = events_by_dir.values().map(|v| v.len()).sum();
                debug!(
                    "smb_watcher({}): processing {} event(s) across {} dir(s)",
                    share_name,
                    total_events,
                    events_by_dir.len()
                );

                process_event_batch(events_by_dir, &volume_id, &mount_path).await;
            }
            Err(e) => {
                // Check for STATUS_NOTIFY_ENUM_DIR (buffer overflow).
                let is_enum_dir = matches!(
                    &e,
                    smb2::Error::Protocol { status, .. }
                        if *status == smb2::types::status::NtStatus::NOTIFY_ENUM_DIR
                );

                if is_enum_dir {
                    debug!(
                        "smb_watcher({}): STATUS_NOTIFY_ENUM_DIR, emitting FullRefresh for share root",
                        share_name
                    );
                    notify_directory_changed(&volume_id, &mount_path, DirectoryChange::FullRefresh);
                    // Index freshness: overflow means the server dropped change
                    // records we can't recover, so the index may have drifted.
                    // Mark it Stale (the index's overflow policy; the watcher
                    // itself keeps running — a different path from a disconnect).
                    #[cfg(any(target_os = "macos", target_os = "linux"))]
                    crate::indexing::on_smb_overflow(&volume_id);
                    // The pipelined-next CHANGE_NOTIFY is already outstanding,
                    // so events arriving during the consumer's re-scan land in
                    // it. Keep watching.
                    continue;
                }

                // Other errors mean the session is likely dead. Bail; the
                // SmbVolume reconnect cycle will respawn us with a fresh
                // session.
                warn!(
                    "smb_watcher({}): next_events failed: {} — bailing, SmbVolume reconnect will respawn",
                    share_name, e
                );
                // Index freshness: the live watch broke, so a Fresh index can no
                // longer be trusted. Flip it Stale (the `WatcherDied` freshness
                // seam). A later reconnect respawns the watcher but does NOT restore Fresh
                // — only a rescan does (the "admittedly stale" model).
                #[cfg(any(target_os = "macos", target_os = "linux"))]
                crate::indexing::on_smb_watcher_died(&volume_id);
                let _ = watcher.close().await;
                return;
            }
        }
    }
}

#[cfg(test)]
mod archive_refresh_test;
