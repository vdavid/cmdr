//! The hub's passwords through a scripted parent: the credential store asked
//! before a prompt and written only after a verified unlock, and a forget that
//! wins over every piece of work started before it. A child of
//! `lifecycle_tests.rs`, sharing its rig.

use super::*;
use cmdr_fs::volume::LockedContainer;

// ── Passwords and the credential store ─────────────────────────────────────

/// A store that counts how often it's asked.
#[derive(Default)]
struct CountingStore {
    inner: InMemoryCredentials,
    lookups: AtomicUsize,
}

impl CredentialStore for CountingStore {
    fn credentials(&self, service: &str, scope: Option<&str>) -> Option<StoredCredentials> {
        self.lookups.fetch_add(1, Ordering::SeqCst);
        self.inner.credentials(service, scope)
    }

    fn save_credentials(
        &self,
        service: &str,
        scope: Option<&str>,
        credentials: &StoredCredentials,
    ) -> Result<(), CredentialsNotStored> {
        self.inner.save_credentials(service, scope, credentials)
    }

    fn forget_credentials(&self, service: &str, scope: Option<&str>) -> Result<(), CredentialsNotForgotten> {
        self.inner.forget_credentials(service, scope)
    }
}

async fn locked_rig(store: Arc<dyn CredentialStore>) -> Rig {
    Rig::new(RigOptions {
        credentials: Some(store),
        unlock: false,
        ..RigOptions::default()
    })
    .await
}

/// A remembered password opens the repo with no prompt at all.
#[tokio::test(flavor = "multi_thread")]
async fn a_stored_password_opens_the_repo_without_asking() {
    let store = InMemoryCredentials::new().with_entry(KEYCHAIN_SERVICE, Some(&scope(ROOT)), &scope(ROOT), PASSWORD);
    let rig = locked_rig(Arc::new(store)).await;

    let rows = rig.volume.list_directory(&view(ROOT), None).await.expect("lists");
    assert_eq!(rows.len(), BACKUPS as usize + 1, "every snapshot plus `latest`");
}

/// ❗ Every listing, stall retry, and pane would otherwise ask the store again
/// (a Keychain read each, maybe a prompt): a miss is remembered.
#[tokio::test(flavor = "multi_thread")]
async fn a_store_miss_is_asked_once() {
    let store = Arc::new(CountingStore::default());
    let rig = locked_rig(Arc::clone(&store) as Arc<dyn CredentialStore>).await;

    for _ in 0..3 {
        let listing = rig.volume.list_directory(&view(ROOT), None).await;
        assert!(
            matches!(
                listing,
                Err(VolumeError::NeedsPassword {
                    container: LockedContainer::ResticRepo,
                    wrong_attempt: false,
                })
            ),
            "got {listing:?}"
        );
    }
    assert_eq!(store.lookups.load(Ordering::SeqCst), 1);
}

/// A remembered password that no longer opens the repo asks as a wrong attempt.
#[tokio::test(flavor = "multi_thread")]
async fn a_stored_password_that_stopped_working_asks_as_a_wrong_attempt() {
    let store = InMemoryCredentials::new().with_entry(KEYCHAIN_SERVICE, Some(&scope(ROOT)), &scope(ROOT), "stale");
    let rig = locked_rig(Arc::new(store)).await;

    let listing = rig.volume.list_directory(&view(ROOT), None).await;
    assert!(
        matches!(
            listing,
            Err(VolumeError::NeedsPassword {
                container: LockedContainer::ResticRepo,
                wrong_attempt: true,
            })
        ),
        "got {listing:?}"
    );
}

/// ❗ An unlock alone files nothing: a host can still drop it, or the person
/// cancel, after it answered. Only `remember_password`, which the host calls once
/// it holds an `Unlocked` nobody cancelled, files it, scoped to this repo on
/// this volume.
#[tokio::test(flavor = "multi_thread")]
async fn only_remember_password_files_a_verified_password() {
    let store = Arc::new(InMemoryCredentials::new());
    let rig = locked_rig(Arc::clone(&store) as Arc<dyn CredentialStore>).await;
    let locked = rig.volume.as_password_locked().expect("a repo is password-locked");

    let outcome = locked.unlock(PASSWORD.to_string()).await.expect("asks the repo");
    assert_eq!(outcome, UnlockOutcome::Unlocked);
    assert!(
        store.credentials(KEYCHAIN_SERVICE, Some(&scope(ROOT))).is_none(),
        "the unlock itself files nothing"
    );

    locked.remember_password(PASSWORD.to_string(), true).await;

    let filed = store
        .credentials(KEYCHAIN_SERVICE, Some(&scope(ROOT)))
        .expect("filed under the repo's scope");
    assert_eq!(filed.secret, PASSWORD);
    assert!(rig.volume.list_directory(&view(ROOT), None).await.is_ok());
}

/// An unlock dropped mid-scrypt (the dialog's Cancel) keeps nothing, a wrong
/// password keeps nothing, and Remember off never seeds the store.
#[tokio::test(flavor = "multi_thread")]
async fn nothing_is_kept_or_filed_for_a_cancelled_unlock_a_wrong_password_or_remember_off() {
    let store = Arc::new(InMemoryCredentials::new());
    let rig = locked_rig(Arc::clone(&store) as Arc<dyn CredentialStore>).await;
    let locked = rig.volume.as_password_locked().expect("a repo is password-locked");

    let cancelled = tokio::time::timeout(Duration::from_millis(1), locked.unlock(PASSWORD.to_string())).await;
    assert!(cancelled.is_err(), "scrypt takes longer than a millisecond");
    // allowed-test-sleep: the dropped unlock's scrypt finishes on the blocking
    // pool; give it the time to (wrongly) commit anything
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(store.credentials(KEYCHAIN_SERVICE, Some(&scope(ROOT))).is_none());
    assert!(
        matches!(
            rig.volume.list_directory(&view(ROOT), None).await,
            Err(VolumeError::NeedsPassword { .. })
        ),
        "a cancelled unlock keeps no password"
    );

    let wrong = locked.unlock("incorrect".to_string()).await.expect("asks the repo");
    assert_eq!(wrong, UnlockOutcome::WrongPassword);
    assert!(store.credentials(KEYCHAIN_SERVICE, Some(&scope(ROOT))).is_none());

    let right = locked.unlock(PASSWORD.to_string()).await.expect("asks the repo");
    assert_eq!(right, UnlockOutcome::Unlocked);
    locked.remember_password(PASSWORD.to_string(), false).await;
    assert!(
        store.credentials(KEYCHAIN_SERVICE, Some(&scope(ROOT))).is_none(),
        "remember off never seeds the store"
    );
}

/// ❗ The host cancels an unlock by dropping it. A drop after scrypt returned but
/// while a concurrent open still holds the slot's open lock (a listing that
/// started with a stale stored password) must keep nothing: the password and the
/// opened repo land together, with no wait between them.
#[tokio::test(flavor = "multi_thread")]
async fn an_unlock_dropped_while_another_open_runs_keeps_nothing() {
    let rig = locked_rig(Arc::new(InMemoryCredentials::new())).await;
    let slot = rig.volume.slot_for_tests();
    let busy = slot.opened.lock().await;
    let locked = rig.volume.as_password_locked().expect("a repo is password-locked");

    let answered = tokio::time::timeout(Duration::from_secs(3), locked.unlock(PASSWORD.to_string())).await;
    drop(busy);

    assert!(
        matches!(answered, Ok(Ok(UnlockOutcome::Unlocked))),
        "the unlock commits without waiting on the other open, so no drop can land between keeping and answering: {answered:?}"
    );
    assert!(rig.volume.list_directory(&view(ROOT), None).await.is_ok());
}

/// ❗ Forgetting the saved password locks the repo again EVERYWHERE: the store
/// entry goes, and so do the password, the opened repo, the cached snapshot list,
/// and the index, any of which would otherwise keep serving listings inside a
/// snapshot without asking. The next unlock opens it as before.
#[tokio::test(flavor = "multi_thread")]
async fn forgetting_the_saved_password_locks_the_repo_until_the_next_unlock() {
    let store =
        Arc::new(InMemoryCredentials::new().with_entry(KEYCHAIN_SERVICE, Some(&scope(ROOT)), &scope(ROOT), PASSWORD));
    let rig = locked_rig(Arc::clone(&store) as Arc<dyn CredentialStore>).await;
    let file = find_file(&rig.volume, latest(ROOT), "a.txt").await;
    let inside = file.parent().expect("a folder inside the snapshot").to_path_buf();
    let locked = rig.volume.as_password_locked().expect("a repo is password-locked");

    assert_eq!(locked.forget_saved_password().await, PasswordForgotten::Everywhere);

    assert!(
        store.credentials(KEYCHAIN_SERVICE, Some(&scope(ROOT))).is_none(),
        "the store entry is gone"
    );
    for path in [view(ROOT), inside.clone()] {
        let listing = rig.volume.list_directory(&path, None).await;
        assert!(
            matches!(
                listing,
                Err(VolumeError::NeedsPassword {
                    container: LockedContainer::ResticRepo,
                    wrong_attempt: false,
                })
            ),
            "{} asks again: {listing:?}",
            path.display()
        );
    }

    let outcome = locked.unlock(PASSWORD.to_string()).await.expect("asks the repo");
    assert_eq!(outcome, UnlockOutcome::Unlocked);
    assert!(rig.volume.list_directory(&inside, None).await.is_ok());
}

/// A store that won't let go still leaves the repo locked for this session, and
/// says the store kept its copy.
#[tokio::test(flavor = "multi_thread")]
async fn a_store_that_refuses_to_forget_still_locks_the_repo() {
    let store = Arc::new(RefusesToForget(InMemoryCredentials::new().with_entry(
        KEYCHAIN_SERVICE,
        Some(&scope(ROOT)),
        &scope(ROOT),
        PASSWORD,
    )));
    let rig = locked_rig(Arc::clone(&store) as Arc<dyn CredentialStore>).await;
    rig.volume
        .list_directory(&view(ROOT), None)
        .await
        .expect("opens with the stored password");
    let locked = rig.volume.as_password_locked().expect("a repo is password-locked");

    assert_eq!(locked.forget_saved_password().await, PasswordForgotten::StoreKeptIt);
    assert!(matches!(
        rig.volume.list_directory(&view(ROOT), None).await,
        Err(VolumeError::NeedsPassword {
            container: LockedContainer::ResticRepo,
            wrong_attempt: false,
        })
    ));
}

/// A store whose delete always says no (a locked keychain).
struct RefusesToForget(InMemoryCredentials);

impl CredentialStore for RefusesToForget {
    fn credentials(&self, service: &str, scope: Option<&str>) -> Option<StoredCredentials> {
        self.0.credentials(service, scope)
    }

    fn save_credentials(
        &self,
        service: &str,
        scope: Option<&str>,
        credentials: &StoredCredentials,
    ) -> Result<(), CredentialsNotStored> {
        self.0.save_credentials(service, scope, credentials)
    }

    fn forget_credentials(&self, _service: &str, _scope: Option<&str>) -> Result<(), CredentialsNotForgotten> {
        Err(CredentialsNotForgotten)
    }
}

// ── A forget wins over work that started before it ─────────────────────────

/// A repo whose password the store holds, with listings of `keys/` held at a
/// gate: every open (a listing's, an unlock's) waits there until it opens.
async fn rig_with_opens_held(store: Arc<dyn CredentialStore>) -> Rig {
    Rig::new(RigOptions {
        credentials: Some(store),
        unlock: false,
        list_gate_open: false,
        gated_dir: "keys",
        ..RigOptions::default()
    })
    .await
}

fn stored(password: &str) -> Arc<InMemoryCredentials> {
    Arc::new(InMemoryCredentials::new().with_entry(KEYCHAIN_SERVICE, Some(&scope(ROOT)), &scope(ROOT), password))
}

/// ❗ An open in flight when the person forgets (a listing that read the stored
/// password a moment before) must keep nothing: it would re-install the opened
/// repo, and the root would list again with no password. And the forget answers
/// at once, ❌ never waiting on that open (a wedged NAS would hang it forever,
/// with the Keychain entry still there).
#[tokio::test(flavor = "multi_thread")]
async fn a_forget_answers_at_once_and_an_open_in_flight_keeps_nothing() {
    let store = stored(PASSWORD);
    let rig = rig_with_opens_held(Arc::clone(&store) as Arc<dyn CredentialStore>).await;
    let parent = Arc::clone(&rig.parent);
    let listing = spawn_listing(&rig.volume, view(ROOT), None);
    wait_until_async(WITHIN, "the listing's open waits on keys/", || {
        parent.gated_listings_started.load(Ordering::SeqCst) == 1
    })
    .await;
    let locked = rig.volume.as_password_locked().expect("a repo is password-locked");

    let forgot = tokio::time::timeout(Duration::from_secs(2), locked.forget_saved_password()).await;
    assert_eq!(
        forgot.ok(),
        Some(PasswordForgotten::Everywhere),
        "answers without waiting on the open"
    );
    assert!(store.credentials(KEYCHAIN_SERVICE, Some(&scope(ROOT))).is_none());

    rig.list_gate.send_replace(true);
    let _ = listing.await.expect("task");
    let after = rig.volume.list_directory(&view(ROOT), None).await;
    assert!(
        matches!(
            after,
            Err(VolumeError::NeedsPassword {
                container: LockedContainer::ResticRepo,
                wrong_attempt: false,
            })
        ),
        "the open that started before the forget kept nothing: {after:?}"
    );
}

/// ❗ An index load another waiter keeps alive (the other pane, a copy) is
/// cancelled by a forget, and installs nothing: every path inside a snapshot
/// would otherwise read without asking.
#[tokio::test(flavor = "multi_thread")]
async fn a_forget_cancels_an_index_load_in_flight_and_it_installs_nothing() {
    let rig = Rig::new(RigOptions {
        gate_open: false,
        ..RigOptions::default()
    })
    .await;
    let parent = Arc::clone(&rig.parent);
    let listing = spawn_listing(&rig.volume, latest(ROOT), None);
    wait_until_async(WITHIN, "the load is reading", || parent.in_flight() > 0).await;
    let locked = rig.volume.as_password_locked().expect("a repo is password-locked");

    locked.forget_saved_password().await;
    rig.open_gate();

    assert!(listing.await.expect("task").is_err(), "the load was cancelled");
    let slot = rig.volume.slot_for_tests();
    wait_until_async(WITHIN, "the load's reads wind down", || parent.in_flight() == 0).await;
    assert!(!ResticRepos::is_indexed(&slot), "nothing installed behind the forget");
}

/// ❗ An unlock in flight when the person forgets (the MCP unlock tool, say)
/// keeps nothing, and filing its password afterwards files nothing: the forget
/// came later, so it wins.
#[tokio::test(flavor = "multi_thread")]
async fn an_unlock_in_flight_during_a_forget_keeps_and_files_nothing() {
    let store = Arc::new(InMemoryCredentials::new());
    let rig = rig_with_opens_held(Arc::clone(&store) as Arc<dyn CredentialStore>).await;
    let parent = Arc::clone(&rig.parent);
    let unlocking = {
        let volume = Arc::clone(&rig.volume);
        tokio::spawn(async move {
            let locked = volume.as_password_locked().expect("a repo is password-locked");
            locked.unlock(PASSWORD.to_string()).await
        })
    };
    wait_until_async(WITHIN, "the unlock's open waits on keys/", || {
        parent.gated_listings_started.load(Ordering::SeqCst) == 1
    })
    .await;
    let locked = rig.volume.as_password_locked().expect("a repo is password-locked");

    locked.forget_saved_password().await;
    rig.list_gate.send_replace(true);
    let unlocked = unlocking.await.expect("task");
    locked.remember_password(PASSWORD.to_string(), true).await;

    assert!(
        !matches!(unlocked, Ok(UnlockOutcome::Unlocked)),
        "an unlock the forget overtook doesn't count: {unlocked:?}"
    );
    assert!(
        store.credentials(KEYCHAIN_SERVICE, Some(&scope(ROOT))).is_none(),
        "nothing filed"
    );
    assert!(matches!(
        rig.volume.list_directory(&view(ROOT), None).await,
        Err(VolumeError::NeedsPassword {
            container: LockedContainer::ResticRepo,
            wrong_attempt: false,
        })
    ));
}
