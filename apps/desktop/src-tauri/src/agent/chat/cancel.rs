//! The one registry of in-flight turns, keyed by conversation.
//!
//! Single-flight means at most one turn per thread, so the conversation id is a sufficient
//! key, and a stop that names a thread nothing is running in is a no-op rather than a race.
//!
//! ⚠️ **It lives here, below `commands/`, because a WAKE has to register too.** A wake is a
//! multi-second background turn spending the user's money, which `docs/design-principles.md`
//! requires be cancelable; its runner sits in `agent/wake/` and may not import upward. Both
//! halves registering in one place is also what lets `ask_cmdr_cancel` stop either one without
//! knowing which it is.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use tokio_util::sync::CancellationToken;

use crate::ignore_poison::IgnorePoison;

static CANCELS: LazyLock<Mutex<HashMap<i64, CancellationToken>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register a fresh cancel token for a conversation and return the clone the turn owns.
///
/// Called BEFORE the turn's thread is spawned, so a stop arriving immediately still lands on a
/// registered token rather than on an empty map.
pub fn register_cancel(conversation_id: i64) -> CancellationToken {
    let token = CancellationToken::new();
    CANCELS.lock_ignore_poison().insert(conversation_id, token.clone());
    token
}

/// Drop a finished turn's entry. Idempotent.
pub fn unregister_cancel(conversation_id: i64) {
    CANCELS.lock_ignore_poison().remove(&conversation_id);
}

/// Trip the in-flight turn for a thread, if there is one. Idempotent: an unknown id (already
/// finished, or never started) is a no-op. A clean stop at the next tool boundary or stream
/// chunk, ❌ never a hard abort.
pub fn cancel_turn(conversation_id: i64) {
    if let Some(token) = CANCELS.lock_ignore_poison().get(&conversation_id) {
        token.cancel();
    }
}

/// Unregister on drop, so a turn that returns early or panics can't leave a stale token behind
/// for the next turn on the same thread to be cancelled by.
pub struct CancelGuard(i64);

impl CancelGuard {
    pub fn new(conversation_id: i64) -> Self {
        Self(conversation_id)
    }
}

impl Drop for CancelGuard {
    fn drop(&mut self) {
        unregister_cancel(self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry is a process-global, so the ids here are deliberately far from anything the
    /// other tests use.
    #[test]
    fn a_registered_turn_can_be_stopped_and_an_unknown_one_is_a_no_op() {
        let token = register_cancel(-9_001);
        assert!(!token.is_cancelled());

        cancel_turn(-9_002); // nothing running under that id
        assert!(
            !token.is_cancelled(),
            "a stop for another thread must not touch this one"
        );

        cancel_turn(-9_001);
        assert!(token.is_cancelled());
        unregister_cancel(-9_001);
    }

    /// ⚠️ A turn that returned early must not leave its token behind: the next turn on the same
    /// thread would then be cancellable by a stop meant for the previous one.
    #[test]
    fn the_guard_clears_the_entry_when_the_turn_ends() {
        let token = register_cancel(-9_003);
        {
            let _guard = CancelGuard::new(-9_003);
        }

        cancel_turn(-9_003);
        assert!(
            !token.is_cancelled(),
            "the entry was gone, so there was nothing to trip"
        );
    }
}
