//! IPC commands for user-editable favorites.
//!
//! Thin pass-throughs over the `favorites::store` module. Each mutation persists `favorites.json`
//! (a filesystem write, so it runs on the blocking pool with a timeout) and then re-emits
//! `volumes-changed` so both panes' switchers refresh live (subscribe-don't-poll). Listing rides
//! the existing `list_volumes` / `volumes-changed` path, so there's no `list_favorites` command.

use tokio::time::Duration;

use crate::commands::util::{DeadlineError, blocking_typed_result_with_timeout};
use crate::favorites::store;

/// 5s matches the write timeout other persisting commands use. The store write is local-only, but a
/// hung data-dir mount must never freeze the IPC thread.
///
/// The store swallows its own write errors (a favorite that doesn't persist still
/// applies in memory), so a missed deadline is the ONLY thing these four commands
/// can report; hence [`DeadlineError`] rather than a vocabulary of their own.
const PERSIST_TIMEOUT: Duration = Duration::from_secs(5);

/// Adds a favorite for `path`, deduping by normalized path. When `name` is omitted, the label
/// defaults to the path's file name.
#[tauri::command]
#[specta::specta]
pub async fn add_favorite(path: String, name: Option<String>) -> Result<(), DeadlineError> {
    persist(move || {
        store::add(&path, name);
        Ok(())
    })
    .await?;
    crate::volume_broadcast::emit_volumes_changed();
    Ok(())
}

/// Removes a favorite by id. No-op when the id isn't present.
#[tauri::command]
#[specta::specta]
pub async fn remove_favorite(id: String) -> Result<(), DeadlineError> {
    persist(move || {
        store::remove(&id);
        Ok(())
    })
    .await?;
    crate::volume_broadcast::emit_volumes_changed();
    Ok(())
}

/// Renames a favorite by id. No-op when the id isn't present.
#[tauri::command]
#[specta::specta]
pub async fn rename_favorite(id: String, name: String) -> Result<(), DeadlineError> {
    persist(move || {
        store::rename(&id, &name);
        Ok(())
    })
    .await?;
    crate::volume_broadcast::emit_volumes_changed();
    Ok(())
}

/// Reorders the favorites to match `ordered_ids`. Unknown ids are ignored; favorites missing from
/// the list are appended in their current order, so a stale order never drops an entry.
#[tauri::command]
#[specta::specta]
pub async fn reorder_favorites(ordered_ids: Vec<String>) -> Result<(), DeadlineError> {
    persist(move || {
        store::reorder(&ordered_ids);
        Ok(())
    })
    .await?;
    crate::volume_broadcast::emit_volumes_changed();
    Ok(())
}

/// Runs one favorites write under the shared deadline, in the typed vocabulary
/// the four commands answer with.
async fn persist(f: impl FnOnce() -> Result<(), DeadlineError> + Send + 'static) -> Result<(), DeadlineError> {
    blocking_typed_result_with_timeout(
        PERSIST_TIMEOUT,
        || DeadlineError::TimedOut,
        |detail| DeadlineError::Unexpected { detail },
        f,
    )
    .await
}
