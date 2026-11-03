//! The hub's passwords: an unlock, filing one in the credential store, the
//! person forgetting it, and looking it up before a prompt. DETAILS §
//! "Passwords and the credential store".

use super::{Bound, KEYCHAIN_SERVICE, RepoSlot, ResticRepos, lock};
use crate::repo::{ResticError, ResticRepo};
use cmdr_fs::volume::secret_store::{self, SignIn};
use cmdr_fs::volume::{LockedContainer, Volume, VolumeError};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use zeroize::Zeroizing;

impl ResticRepos {
    /// Tries `password` on the repo in `slot`, through `parent`: `Ok(true)` when
    /// it unlocks (the password and the opened repo are kept), `Ok(false)` when
    /// it doesn't (nothing is kept). `Err` when the repo couldn't be asked.
    ///
    /// Commits only after the open returns, the password and the opened repo in
    /// one step with no await between them and the answer, so a caller dropping
    /// this future either kept nothing or already holds the answer. The opened
    /// repo is kept only when no other open holds the slot's lock (that one
    /// keeps its own), ❌ never waited for.
    pub(crate) async fn unlock(
        &self,
        slot: &RepoSlot,
        parent: &Arc<dyn Volume>,
        password: &str,
    ) -> Result<bool, VolumeError> {
        let forgets = slot.forgets();
        let opened = ResticRepo::open(Arc::clone(parent), slot.key.root.clone(), password, None, &self.host).await;
        match opened {
            Ok(repo) => {
                {
                    // A forget that landed while this unlock ran wins: keep nothing,
                    // and answer so the host files nothing either.
                    let mut kept = lock(&slot.password);
                    if slot.forgets() != forgets {
                        return Err(VolumeError::Cancelled(slot.key.root.display().to_string().into()));
                    }
                    *kept = Some(Zeroizing::new(password.to_string()));
                }
                slot.store_missed.store(false, Ordering::Relaxed);
                if let Ok(mut opened) = slot.opened.try_lock() {
                    *opened = Some(Bound::new(parent, Arc::new(repo), forgets));
                }
                slot.touch();
                self.evict_opened(slot);
                Ok(true)
            }
            Err(ResticError::WrongPassword) => Ok(false),
            Err(err) => Err(err.into_volume_error(&slot.key.root)),
        }
    }

    /// Files a password that just unlocked `slot`'s repo in the credential store,
    /// the way the dialog's Remember box asks (`secret_store::keep_signed_in_secret`:
    /// on saves, off only refreshes an entry already there). ❗ Only after a
    /// verified unlock. Answers whether the store took it.
    ///
    /// Files nothing unless the slot still holds this password (a forget since
    /// the unlock cleared it), and takes the entry back out if a forget lands
    /// while the store writes it: the forget came later, so it wins.
    pub(crate) async fn keep_password(&self, slot: &RepoSlot, password: &str, remember: bool) -> bool {
        let forgets = slot.forgets();
        if slot.password_in_memory().is_none_or(|kept| kept.as_str() != password) {
            return false;
        }
        let sign_in = SignIn {
            username: slot.key.store_scope(),
            secret: password.to_string(),
            remember,
        };
        let kept = secret_store::keep_signed_in_secret(&self.host, KEYCHAIN_SERVICE, &sign_in).await;
        if slot.forgets() != forgets {
            // allowed-discarded-outcome: the forget reports its own delete; this
            // one only undoes the save that raced it.
            let _undone =
                secret_store::forget_stored_secret(&self.host, KEYCHAIN_SERVICE, &slot.key.store_scope()).await;
            return false;
        }
        kept
    }

    /// Locks `slot`'s repo again, as the person asked: the password goes from
    /// memory and from the credential store, and the opened repo, the snapshot
    /// list, and the index go with it, since each would otherwise keep serving
    /// reads without asking. The store isn't asked again this session. Answers
    /// whether the store let go of its copy; the repo is locked either way.
    ///
    /// Wins over every piece of work that started before it: it bumps the
    /// slot's forget count first, and an open, refresh, index load, store read,
    /// or unlock in flight commits nothing once the count moved
    /// (`RepoSlot::forgets`). So it waits on none of them, ❌ never an open's
    /// lock (an open wedged on a dead NAS would hang the forget, with the
    /// Keychain entry still there). An index load in flight is cancelled.
    pub(crate) async fn forget_saved_password(&self, slot: &RepoSlot) -> bool {
        {
            let mut password = lock(&slot.password);
            slot.forgets.fetch_add(1, Ordering::SeqCst);
            *password = None;
        }
        slot.store_missed.store(true, Ordering::Relaxed);
        slot.drop_opened_repo(&self.host.runtime());
        *lock(&slot.snapshots) = None;
        let loading = lock(&slot.index).loading.take();
        if let Some(load) = loading {
            load.cancel_now();
        }
        secret_store::forget_stored_secret(&self.host, KEYCHAIN_SERVICE, &slot.key.store_scope()).await
    }

    /// The repo's password: the one in memory, else the credential store's
    /// (asked once per session: a miss is remembered until an unlock), else
    /// `NeedsPassword`.
    ///
    /// The store is asked under the slot's own lookup lock, so concurrent
    /// listings (and a stalled listing's second attempt) ask it once, and ❌ no
    /// hub-wide lock is held across a Keychain prompt.
    pub(super) async fn password(&self, slot: &RepoSlot) -> Result<Zeroizing<String>, VolumeError> {
        let forgets = slot.forgets();
        if let Some(password) = slot.password_in_memory() {
            return Ok(password);
        }
        let _asking = slot.store_lookup.lock().await;
        if let Some(password) = slot.password_in_memory() {
            return Ok(password);
        }
        if slot.store_missed.load(Ordering::Relaxed) {
            return Err(VolumeError::NeedsPassword {
                container: LockedContainer::ResticRepo,
                wrong_attempt: false,
            });
        }
        let scope = slot.key.store_scope();
        match secret_store::stored_secret(&self.host, KEYCHAIN_SERVICE, &scope).await {
            Some(stored) => {
                let password = Zeroizing::new(stored.secret);
                let mut kept = lock(&slot.password);
                // Read before a forget that deleted it since: it's gone.
                if slot.forgets() != forgets {
                    return Err(VolumeError::NeedsPassword {
                        container: LockedContainer::ResticRepo,
                        wrong_attempt: false,
                    });
                }
                *kept = Some(password.clone());
                Ok(password)
            }
            None => {
                slot.store_missed.store(true, Ordering::Relaxed);
                Err(VolumeError::NeedsPassword {
                    container: LockedContainer::ResticRepo,
                    wrong_attempt: false,
                })
            }
        }
    }

    /// The [`VolumeError`] for a failed open of `slot`'s repo. A password that
    /// stopped working (a stored one, or the repo's keys changed) is forgotten,
    /// and the store isn't asked again this session: the person types one.
    pub(super) fn refused(slot: &RepoSlot, err: ResticError) -> VolumeError {
        if matches!(err, ResticError::WrongPassword) {
            *lock(&slot.password) = None;
            slot.store_missed.store(true, Ordering::Relaxed);
        }
        err.into_volume_error(&slot.key.root)
    }
}
