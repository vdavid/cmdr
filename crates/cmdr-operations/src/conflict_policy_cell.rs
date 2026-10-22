//! The conflict policy an operation is running under RIGHT NOW, as every window
//! shows it: [`ConflictPolicyCell`].
//!
//! A copy or move starts with the policy its dialog confirmed, and a person can
//! change it mid-run by answering a clash with "… all" (Overwrite all, Skip all,
//! and so on). From then on the latch decides, so a progress dialog that kept
//! saying "Ask for each" would describe a policy that no longer runs. The cell
//! holds the effective one and says so on `operations-changed`, which is how a
//! view that ADOPTED the operation (and never saw its start config) learns it.
//!
//! What it holds is the SAME-KIND answer: the one a file landing on a file
//! obeys. The two cross-type latches (`conflict::ApplyToAll`) answer narrower
//! questions a one-line caption can't word, and they never widen the policy
//! for ordinary clashes, so leaving them out keeps the caption true for the
//! clashes it describes.

use std::sync::{Mutex, OnceLock};

use cmdr_fs::ignore_poison::IgnorePoison;

use super::types::ConflictResolution;

/// Says a policy change out loud. The manager's broadcast, handed in as a
/// closure so this cell never names the manager (whose registry holds the
/// state that holds this cell: naming it back would close a module cycle).
type OnChange = Box<dyn Fn() + Send + Sync>;

/// One operation's effective conflict policy, plus who to tell when it
/// changes. `None` for an operation with no policy at all (a delete, a trash, a
/// rename), which is also what the snapshot reports for it.
// DEFAULT-OK: no policy and no listener yet is the state of every operation that has none,
// and of a transfer before `with_conflict_policy` and registration fill it in.
#[derive(Default)]
pub(crate) struct ConflictPolicyCell {
    policy: Mutex<Option<ConflictResolution>>,
    /// Set when the manager registers the operation. The closure holds the
    /// manager weakly: its registry holds the state that holds this cell.
    on_change: OnceLock<OnChange>,
}

impl ConflictPolicyCell {
    /// A cell that starts on `policy`: the one the operation was confirmed with.
    pub(crate) fn starting_on(policy: ConflictResolution) -> Self {
        Self {
            policy: Mutex::new(Some(policy)),
            on_change: OnceLock::new(),
        }
    }

    /// The policy in force now, or `None` for an operation that has none.
    pub(crate) fn current(&self) -> Option<ConflictResolution> {
        *self.policy.lock_ignore_poison()
    }

    /// Wires what broadcasts a change. The manager's call, once, at
    /// registration.
    pub(crate) fn attach(&self, on_change: impl Fn() + Send + Sync + 'static) {
        let _ = self.on_change.set(Box::new(on_change));
    }

    /// A person latched `policy` for every clash to come. Re-broadcasts the
    /// operation list when that changed what the operation runs under; a repeat
    /// of the same answer says nothing.
    pub(crate) fn latch(&self, policy: ConflictResolution) {
        {
            let mut current = self.policy.lock_ignore_poison();
            if *current == Some(policy) {
                return;
            }
            *current = Some(policy);
        }
        // Outside the lock: the broadcast reads `current()` back through the
        // manager's snapshot.
        if let Some(on_change) = self.on_change.get() {
            on_change();
        }
    }
}
