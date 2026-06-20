//! Per-request cancellation tokens for in-flight `stream_folder_suggestions` calls.
//!
//! Why a separate registry rather than reusing `MANAGER`: this state is purely about
//! AI streaming task lifecycle, not file-manager AI state. Keeping it isolated avoids
//! expanding `ManagerState` and lets us drop entries on task end without a wider lock.

use crate::ignore_poison::IgnorePoison;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use tokio_util::sync::CancellationToken;

static STREAM_CANCEL_TOKENS: LazyLock<Mutex<HashMap<String, CancellationToken>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Registers a fresh `CancellationToken` for `request_id` and returns a clone for the
/// task to await. If `request_id` collides with an existing entry (UUID collision is
/// astronomically unlikely; buggy frontend possible) the prior token is silently
/// orphaned; the prior task will keep running until natural completion.
pub(super) fn register_stream(request_id: &str) -> CancellationToken {
    let token = CancellationToken::new();
    STREAM_CANCEL_TOKENS
        .lock_ignore_poison()
        .insert(request_id.to_owned(), token.clone());
    token
}

/// Removes the token for `request_id` from the registry. The task calls this from its
/// RAII guard; safe to call even if `cancel_stream` already removed the entry.
pub(super) fn unregister_stream(request_id: &str) {
    STREAM_CANCEL_TOKENS.lock_ignore_poison().remove(request_id);
}

/// Cancels and removes the token for `request_id`. Idempotent: missing id is a no-op
/// (the stream may have already completed and unregistered). `CancellationToken::cancel`
/// is itself idempotent.
pub fn cancel_stream(request_id: &str) {
    if let Some(token) = STREAM_CANCEL_TOKENS.lock_ignore_poison().remove(request_id) {
        token.cancel();
    }
}
