//! Finder tags on a volume: the I/O half of `tags.rs`, routed through `Volume`'s xattr
//! methods, so a direct SMB share reads over smb2 and never needs its macOS mount.
//!
//! Every network-bound wait here is bounded from outside the work
//! (`cmdr_fs::deadline`): a reply can give up while the read or write lands behind it,
//! and it's the work that patches the listing. Flow and decisions:
//! `file_system/DETAILS.md` § "Finder tags".

use super::tags::TagToggleOutcome;
use cmdr_fs::entry::TagRef;
use cmdr_fs::log_error::LogError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(target_os = "macos")]
use super::tags::{
    TAGS_XATTR, TagReadError, TagSkip, emit_tag_analytics, encode_tags_plist, parse_tags_plist,
    parse_tags_plist_strict, system_color_name, toggled_tags,
};
use cmdr_fs::deadline::{BlockingBudget, TimedOut, io_budget, timeout_detached_typed};
use cmdr_fs::volume::{Volume, VolumeError, XattrReadMode};
use cmdr_listing::{ListingId, RowFacts};
use std::time::Duration;

/// Local-disk tier of the read wait; `io_budget` lifts it on a live session.
const READ_TIMEOUT: Duration = Duration::from_secs(2);
/// The 5 s "write" tier per `commands/CLAUDE.md`, lifted the same way.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the right-click menu's tag read holds a worker. The menu shows its circles
/// whenever this answers, so it bounds the worker, not the menu.
#[cfg(target_os = "macos")]
const MENU_READ_TIMEOUT: Duration = Duration::from_secs(2);

/// Every tag read in flight, from every caller: the visible range, the sweep's 500-path
/// chunks, and the watcher's re-reads. A chunk that outruns its IPC wait keeps its place
/// here, so a slow share slows the sweep down and can't pile up behind it.
static TAG_READS: BlockingBudget = BlockingBudget::new(4);

/// The volume that stores tags for the rows in one folder: the drive itself. An archive's
/// insides, the `.git` portal, and a restic view hold none, and neither does a backend
/// without xattrs.
///
/// Asked about the row's PARENT because a row can be a boundary itself: `photos.zip` is a
/// file on the drive, and resolving it directly routes to the archive inside it. `resolve`,
/// ❌ never a bare `get`, so a path that crosses one of those boundaries can't reach the
/// drive under a path that doesn't exist there.
///
/// With no pane to ask (`volume_id` is `None` or isn't a real volume: a search hit), the
/// registered volume with the longest root over the path answers.
async fn tag_store_for(volume_id: Option<&str>, row: &Path) -> Option<Arc<dyn Volume>> {
    let manager = super::volume::manager::get_volume_manager();
    let parent = row.parent().unwrap_or(row);
    if let Some(volume_id) = volume_id {
        let resolved = manager.resolve(volume_id, parent).await;
        if resolved.is_routed() || resolved.unconfirmed.is_some() {
            return None;
        }
        if let Some(volume) = resolved.volume {
            return volume.supports_xattrs().then_some(volume);
        }
    }
    manager
        .list_volumes_with_handles()
        .into_iter()
        .filter(|(_, volume)| row.starts_with(volume.root()))
        .max_by_key(|(_, volume)| volume.root().components().count())
        .map(|(_, volume)| volume)
        .filter(|volume| volume.supports_xattrs())
}

/// One volume's share of a batch of paths.
struct VolumeGroup {
    volume: Arc<dyn Volume>,
    paths: Vec<String>,
}

/// `paths` by the volume that stores their tags, and the ones nothing stores tags for.
/// Each folder resolves once, so a 500-path chunk costs one route.
async fn group_by_store(volume_id: Option<&str>, paths: Vec<String>) -> (Vec<VolumeGroup>, Vec<String>) {
    let mut by_parent: HashMap<Option<PathBuf>, Option<Arc<dyn Volume>>> = HashMap::new();
    let mut groups: Vec<VolumeGroup> = Vec::new();
    let mut nowhere = Vec::new();
    for path in paths {
        let row = Path::new(&path);
        let parent = row.parent().map(Path::to_path_buf);
        let store = match by_parent.get(&parent) {
            Some(store) => store.clone(),
            None => {
                let store = tag_store_for(volume_id, row).await;
                by_parent.insert(parent, store.clone());
                store
            }
        };
        match store {
            None => nowhere.push(path),
            Some(volume) => match groups.iter_mut().find(|group| Arc::ptr_eq(&group.volume, &volume)) {
                Some(group) => group.paths.push(path),
                None => groups.push(VolumeGroup {
                    volume,
                    paths: vec![path],
                }),
            },
        }
    }
    (groups, nowhere)
}

/// The volume a listing reads through, `None` for an empty id or a listing that's gone.
fn volume_of_listing(listing_id: &str) -> Option<String> {
    if listing_id.is_empty() {
        return None;
    }
    let location = crate::listing_host::listings().location(&ListingId::new(listing_id.to_string()));
    location.map(|(volume_id, _)| volume_id)
}

/// How long a reply waits on `volume_id`: the local tier, or the live-session floor.
fn wait_on(volume_id: Option<&str>, local: Duration) -> Duration {
    let state = volume_id
        .and_then(|id| super::volume::manager::get_volume_manager().get(id))
        .and_then(|volume| volume.connection_state());
    io_budget(state, local)
}

/// Reads tags as a pane shows them: `(path, tags)` for every path the store answered.
///
/// ❌ A failed read leaves its path OUT rather than reading as "no tags": the listing
/// replaces unconditionally, so a dropped connection would blank every dot in the pane.
/// A value that doesn't decode does show as none: there's nothing to display.
#[cfg(target_os = "macos")]
async fn read_on_group(group: &VolumeGroup, mode: XattrReadMode) -> Vec<(String, Vec<TagRef>)> {
    let wire: Vec<PathBuf> = group.paths.iter().map(PathBuf::from).collect();
    let answers = group.volume.read_xattr_many(&wire, TAGS_XATTR, mode).await;
    group
        .paths
        .iter()
        .zip(answers)
        .filter_map(|(path, answer)| match answer {
            Ok(Some(bytes)) => Some((path.clone(), parse_tags_plist(&bytes))),
            Ok(None) => Some((path.clone(), Vec::new())),
            Err(e) => {
                log::debug!(
                    target: "tags",
                    "couldn't read the tags on {path:?}: {e:?}",
                    path = cmdr_fs::log_private::LogPrivate::path(path),
                    e = LogError(&e),
                );
                None
            }
        })
        .collect()
}

/// Tags for rows a listing just named, so a row gone since then reads as none.
#[cfg(target_os = "macos")]
async fn read_for_display(volume_id: Option<&str>, paths: Vec<String>) -> Vec<(String, Vec<TagRef>)> {
    let (groups, _nowhere) = group_by_store(volume_id, paths).await;
    let mut out = Vec::new();
    for group in &groups {
        out.extend(read_on_group(group, XattrReadMode::AssumePathsExist).await);
    }
    out
}

/// Reads tags off `paths` and patches them into `listing_ids`.
#[cfg(target_os = "macos")]
async fn read_and_apply(volume_id: Option<String>, paths: Vec<String>, listing_ids: Vec<ListingId>) {
    let runtime = tokio::runtime::Handle::current();
    let budgeted = TAG_READS.run(move || {
        // On a blocking thread, holding the budget's permit until the read really ends:
        // that's what keeps a dead mount's stragglers counted.
        let updates = runtime.block_on(read_for_display(volume_id.as_deref(), paths));
        if updates.is_empty() {
            return;
        }
        let listings = crate::listing_host::listings();
        for listing_id in &listing_ids {
            listings.apply_row_facts(listing_id, RowFacts::Tags(updates.clone()));
        }
    });
    if let Err(e) = budgeted.await {
        log::warn!(target: "tags", "a tag read panicked: {e}");
    }
}

/// The `enrich_tags` command: reads tags for `paths` and patches them into the listing.
/// The reply gives up after [`wait_on`]; the read runs on and still lands.
pub async fn enrich_listing(listing_id: String, paths: Vec<String>) -> TimedOut<()> {
    #[cfg(target_os = "macos")]
    {
        let volume_id = volume_of_listing(&listing_id);
        let wait = wait_on(volume_id.as_deref(), READ_TIMEOUT);
        let work = async move {
            read_and_apply(volume_id, paths, vec![ListingId::new(listing_id)]).await;
            Ok::<(), ()>(())
        };
        let timed_out = timeout_detached_typed(wait, || (), |_| (), work).await.is_err();
        TimedOut { data: (), timed_out }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (listing_id, paths);
        TimedOut {
            data: (),
            timed_out: false,
        }
    }
}

/// A watcher saw `paths` change on `volume_id` (`ListingHost::row_xattrs_may_have_changed`).
/// Reads them again for the panes showing `parent`, and costs nothing when none does.
pub fn refresh_rows(volume_id: &str, parent: &Path, paths: Vec<PathBuf>) {
    #[cfg(target_os = "macos")]
    {
        let showing = crate::listing_host::listings().listings_for_path_on_volume(volume_id, parent);
        if showing.is_empty() {
            return;
        }
        let volume_id = volume_id.to_string();
        // A batch names one file per event, and one stream write is several events.
        let paths: std::collections::BTreeSet<String> =
            paths.iter().map(|path| path.to_string_lossy().into_owned()).collect();
        let paths = paths.into_iter().collect();
        tauri::async_runtime::spawn(read_and_apply(Some(volume_id), paths, showing));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (volume_id, parent, paths);
}

/// Which colors every one of `paths` carries, for the right-click menu's circles. Called
/// from a worker thread, so it blocks; a store that doesn't answer in time reads as none.
#[cfg(target_os = "macos")]
pub fn applied_colors_blocking(listing_id: String, paths: Vec<String>) -> [bool; 8] {
    tauri::async_runtime::block_on(async move {
        let work = async move {
            let volume_id = volume_of_listing(&listing_id);
            let count = paths.len();
            let (groups, nowhere) = group_by_store(volume_id.as_deref(), paths).await;
            let mut per_path = vec![Vec::new(); nowhere.len()];
            for group in &groups {
                per_path.extend(
                    read_on_group(group, XattrReadMode::AssumePathsExist)
                        .await
                        .into_iter()
                        .map(|(_, tags)| tags),
                );
            }
            // A path that didn't answer holds none we know of, and "every path" says so.
            per_path.resize(count, Vec::new());
            Ok::<_, ()>(super::tags::applied_colors(&per_path))
        };
        timeout_detached_typed(MENU_READ_TIMEOUT, || (), |_| (), work)
            .await
            .unwrap_or([false; 8])
    })
}

/// Why a tag write didn't finish.
#[derive(Debug)]
pub enum TagWriteError {
    /// A tag color index other than the seven real ones.
    NotAColor(u8),
    /// The store refused a write. Earlier files keep their new tags.
    Refused { path: String, reason: VolumeError },
}

impl std::fmt::Display for TagWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAColor(color) => write!(f, "tag color index must be 1..=7, got {color}"),
            Self::Refused { path, reason } => write!(
                f,
                "couldn't write the tags on {path:?}: {reason:?}",
                path = cmdr_fs::log_private::LogPrivate::path(path),
                reason = LogError(reason),
            ),
        }
    }
}

/// One file's tags as read for a write that merges into them.
#[cfg(target_os = "macos")]
pub(crate) struct CurrentTags {
    volume: Arc<dyn Volume>,
    pub(crate) path: String,
    pub(crate) tags: Vec<TagRef>,
}

/// Reads every file a merging write will touch. A file whose tags can't be read or fully
/// decoded goes in the skip list, never the keep list: writing it would replace tags
/// nobody saw (#541). So does one nothing stores tags for.
#[cfg(target_os = "macos")]
pub(crate) async fn read_for_merge(volume_id: Option<&str>, paths: Vec<String>) -> (Vec<CurrentTags>, Vec<TagSkip>) {
    let (groups, nowhere) = group_by_store(volume_id, paths).await;
    let mut current = Vec::new();
    let mut skipped: Vec<TagSkip> = nowhere
        .into_iter()
        .map(|path| TagSkip {
            path,
            reason: TagReadError::Unreadable(VolumeError::NotSupported),
        })
        .collect();
    for group in &groups {
        let wire: Vec<PathBuf> = group.paths.iter().map(PathBuf::from).collect();
        let answers = group
            .volume
            .read_xattr_many(&wire, TAGS_XATTR, XattrReadMode::Verified)
            .await;
        for (path, answer) in group.paths.iter().cloned().zip(answers) {
            let tags = match answer {
                Ok(Some(bytes)) => parse_tags_plist_strict(&bytes),
                Ok(None) => Ok(Vec::new()),
                Err(e) => Err(TagReadError::Unreadable(e)),
            };
            match tags {
                Ok(tags) => current.push(CurrentTags {
                    volume: group.volume.clone(),
                    path,
                    tags,
                }),
                Err(reason) => skipped.push(TagSkip { path, reason }),
            }
        }
    }
    (current, skipped)
}

/// Writes the full tag set to a file's `_kMDItemUserTags`. An empty set writes an empty
/// plist array, which is what macOS itself does to clear them (verified on macOS 27.0.1,
/// `NSURLTagNamesKey` on APFS, 2026-10-31), and what a QNAP needs: it answers success to a
/// delete and keeps the attribute (`crates/cmdr-smb/DETAILS.md` § "Extended attributes").
///
/// Touches ONLY `_kMDItemUserTags`, never `com.apple.FinderInfo` (D11): that 32-byte blob
/// holds `kHasCustomIcon` plus type/creator codes, and modern Finder reads tags straight
/// from `_kMDItemUserTags`, so a custom folder icon survives a tag write. Each write is
/// atomic per attribute, so there's no partial-write window for a single file.
#[cfg(target_os = "macos")]
async fn write_tags(volume: &dyn Volume, path: &str, tags: &[TagRef]) -> Result<(), TagWriteError> {
    let refused = |reason| TagWriteError::Refused {
        path: path.to_string(),
        reason,
    };
    let bytes = encode_tags_plist(tags).map_err(|e| refused(VolumeError::from_io_at(&e, path)))?;
    volume
        .write_xattr(Path::new(path), TAGS_XATTR, &bytes)
        .await
        .map_err(refused)
}

/// Writes `desired(tags)` to each file, skipping every file already there so it doesn't
/// churn the modification date on a server that bumps it (`streams_xattr` does), and
/// answers each file's resulting set. On a failure, earlier files keep their new tags.
#[cfg(target_os = "macos")]
pub(crate) async fn apply_merge(
    current: Vec<CurrentTags>,
    desired: impl Fn(&[TagRef]) -> Vec<TagRef>,
) -> Result<Vec<(String, Vec<TagRef>)>, TagWriteError> {
    let mut results = Vec::with_capacity(current.len());
    for CurrentTags { volume, path, tags } in current {
        let new_tags = desired(&tags);
        if new_tags != tags {
            write_tags(volume.as_ref(), &path, &new_tags).await?;
        }
        results.push((path, new_tags));
    }
    Ok(results)
}

/// Clears every tag on `paths`. Reads nothing to keep, since the caller asked for all of
/// them gone: only "already none" skips the write, so a value that doesn't decode is
/// overwritten and a file with no attribute stays without one.
#[cfg(target_os = "macos")]
pub(crate) async fn clear_tags(
    volume_id: Option<&str>,
    paths: Vec<String>,
) -> Result<Vec<(String, Vec<TagRef>)>, TagWriteError> {
    let (groups, nowhere) = group_by_store(volume_id, paths).await;
    if let Some(path) = nowhere.into_iter().next() {
        return Err(TagWriteError::Refused {
            path,
            reason: VolumeError::NotSupported,
        });
    }
    let mut cleared = Vec::new();
    for group in &groups {
        let wire: Vec<PathBuf> = group.paths.iter().map(PathBuf::from).collect();
        let answers = group
            .volume
            .read_xattr_many(&wire, TAGS_XATTR, XattrReadMode::Verified)
            .await;
        for (path, answer) in group.paths.iter().zip(answers) {
            let already_clear = match answer {
                Ok(None) => true,
                Ok(Some(bytes)) => parse_tags_plist_strict(&bytes).is_ok_and(|tags| tags.is_empty()),
                Err(_) => false,
            };
            if !already_clear {
                write_tags(group.volume.as_ref(), path, &[]).await?;
            }
            cleared.push((path.clone(), Vec::new()));
        }
    }
    Ok(cleared)
}

/// Toggles one system color across a selection, preserving every OTHER tag on every file,
/// and answers the resulting per-path sets for the listing cache.
///
/// Finder's multi-file rule, mirrored here: if ALL readable paths already carry the color,
/// remove it from all; otherwise add it to all (`tags::toggled_tags`). A file whose current
/// tags can't be read is left untouched, reported in `skipped`, and doesn't count toward
/// the all-have decision.
#[cfg(target_os = "macos")]
pub async fn toggle_color(
    volume_id: Option<&str>,
    paths: Vec<String>,
    color: u8,
) -> Result<TagToggleOutcome, TagWriteError> {
    let color_name = system_color_name(color).ok_or(TagWriteError::NotAColor(color))?;
    let path_count = paths.len();
    let (current, skipped) = read_for_merge(volume_id, paths).await;
    let all_have = !current.is_empty()
        && current
            .iter()
            .all(|file| file.tags.iter().any(|tag| tag.color == color));
    let add = !all_have;
    let written = apply_merge(current, |tags| toggled_tags(tags, color, color_name, add)).await;
    emit_tag_analytics(add, color_name, path_count, written.is_ok() && skipped.is_empty());
    Ok(TagToggleOutcome {
        updates: written?,
        skipped,
    })
}

/// Non-macOS: no Finder tags, so nothing to toggle.
#[cfg(not(target_os = "macos"))]
pub async fn toggle_color(
    _volume_id: Option<&str>,
    _paths: Vec<String>,
    _color: u8,
) -> Result<TagToggleOutcome, TagWriteError> {
    Ok(TagToggleOutcome::default())
}

/// Toggles `color` on the rows of `listing_id` and patches the listing. The keyboard
/// command and the context-menu circles both land here. A write failure is logged, not
/// surfaced: tags are low-stakes, each write is atomic, and the pane shows what's on disk
/// at its next read.
async fn toggle_and_patch(app: tauri::AppHandle, listing_id: String, paths: Vec<String>, color: u8) {
    let volume_id = volume_of_listing(&listing_id);
    match toggle_color(volume_id.as_deref(), paths, color).await {
        Ok(outcome) => {
            super::tags::report_skips(&app, &outcome.skipped);
            if !outcome.updates.is_empty() {
                crate::listing_host::listings()
                    .apply_row_facts(&ListingId::new(listing_id), RowFacts::Tags(outcome.updates));
            }
        }
        Err(e) => log::warn!(target: "tags", "tag toggle failed (color={color}): {e}"),
    }
}

/// The `toggle_tags` command. The reply gives up after the write tier; the write lands.
pub async fn toggle_for_command(
    app: tauri::AppHandle,
    listing_id: String,
    paths: Vec<String>,
    color: u8,
) -> TimedOut<()> {
    let wait = wait_on(volume_of_listing(&listing_id).as_deref(), WRITE_TIMEOUT);
    let work = async move {
        toggle_and_patch(app, listing_id, paths, color).await;
        Ok::<(), ()>(())
    };
    let timed_out = timeout_detached_typed(wait, || (), |_| (), work).await.is_err();
    TimedOut { data: (), timed_out }
}

/// A context-menu circle's click: off the main (menu) thread, with nobody waiting.
#[cfg(target_os = "macos")]
pub fn toggle_from_menu(app: tauri::AppHandle, listing_id: String, paths: Vec<String>, color: u8) {
    tauri::async_runtime::spawn(toggle_and_patch(app, listing_id, paths, color));
}
