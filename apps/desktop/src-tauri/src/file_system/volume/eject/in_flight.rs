//! One eject at a time per volume. A request for a volume whose eject is still
//! running JOINS it and gets the same answer, with no second teardown: a slow
//! `diskutil` (10.5 s in one user's log) invites repeat clicks, and every caller
//! (the header chip, a dropdown row, the native menu, MCP) lands in [`super::eject`].
//!
//! The volumes with an eject in flight are the EJECTING set, pushed to the
//! frontend as `volumes-ejecting-changed` on every change and bootstrapped through
//! `get_ejecting_volume_ids`, the same shape as the busy set's
//! `volumes-busy-changed` (`write_operations/status_cache.rs`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock};

use futures_util::FutureExt;
use futures_util::future::{BoxFuture, Shared};

use super::EjectError;
use crate::ignore_poison::IgnorePoison;

/// An eject in flight, awaitable by every caller that joined it.
pub(super) type Flight = Shared<BoxFuture<'static, Result<(), EjectError>>>;

struct InFlight {
    /// Tells this flight's own landing apart from a newer flight's for the same volume.
    flight_id: u64,
    flight: Flight,
}

static IN_FLIGHT: LazyLock<Mutex<HashMap<String, InFlight>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_FLIGHT_ID: AtomicU64 = AtomicU64::new(0);

/// Typed `volumes-ejecting-changed` Tauri event. Wraps the ID list in a struct
/// because `tauri_specta::Event` payloads must be named types; the struct name
/// kebab-cases to the wire name.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct VolumesEjectingChanged {
    /// IDs of volumes whose eject is still running (sorted).
    pub volume_ids: Vec<String>,
}

/// App handle for emitting `volumes-ejecting-changed`. Absent in unit tests, where
/// the set is still queryable through [`ejecting_volume_ids`].
static EJECTING_APP: OnceLock<tauri::AppHandle> = OnceLock::new();

/// Stores the app handle used to broadcast `volumes-ejecting-changed`. Call once at
/// app setup, before the frontend can ask for an eject.
pub fn init_ejecting_volume_emitter(app: &tauri::AppHandle) {
    let _ = EJECTING_APP.set(app.clone());
}

/// The volumes whose eject is still running (sorted). Backs the
/// `get_ejecting_volume_ids` bootstrap and the native menus' disabled Eject item.
pub fn ejecting_volume_ids() -> Vec<String> {
    sorted_ids(&IN_FLIGHT.lock_ignore_poison())
}

/// Joins the eject in flight for `volume_id`, or starts one from `start`.
///
/// Synchronous on purpose: the join-or-start decision is made under the lock
/// before this returns, so no second request can slip in between and start a
/// second teardown. `start` is called only when nothing is in flight, and its
/// future runs on its own task, so it finishes even if every caller stops
/// waiting (an unmount can't be taken back halfway), and a panic inside it
/// answers [`EjectError::Unexpected`] instead of stranding the volume in the set.
pub(super) fn join_or_start<F, Fut>(volume_id: &str, start: F) -> Flight
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<(), EjectError>> + Send + 'static,
{
    let mut in_flight = IN_FLIGHT.lock_ignore_poison();
    if let Some(running) = in_flight.get(volume_id) {
        log::debug!(target: "eject", "An eject of {volume_id} is already running; joining it");
        return running.flight.clone();
    }

    let flight_id = NEXT_FLIGHT_ID.fetch_add(1, Ordering::Relaxed);
    let landing = Landing {
        volume_id: volume_id.to_string(),
        flight_id,
    };
    let work = start();
    let task = tokio::spawn(async move {
        // Dropped the moment the teardown ends, a panic included, and before any
        // caller sees the answer, so a caller never reads a stale "ejecting".
        let _landing = landing;
        work.await
    });
    let flight = async move {
        task.await.unwrap_or_else(|join_err| {
            Err(EjectError::Unexpected {
                detail: format!("the eject task failed: {join_err}"),
            })
        })
    }
    .boxed()
    .shared();

    in_flight.insert(
        volume_id.to_string(),
        InFlight {
            flight_id,
            flight: flight.clone(),
        },
    );
    emit_changed(&in_flight);
    flight
}

/// Takes its flight out of the ejecting set when the teardown ends.
struct Landing {
    volume_id: String,
    flight_id: u64,
}

impl Drop for Landing {
    fn drop(&mut self) {
        let mut in_flight = IN_FLIGHT.lock_ignore_poison();
        if in_flight
            .get(&self.volume_id)
            .is_some_and(|running| running.flight_id == self.flight_id)
        {
            in_flight.remove(&self.volume_id);
            emit_changed(&in_flight);
        }
    }
}

fn sorted_ids(in_flight: &HashMap<String, InFlight>) -> Vec<String> {
    let mut ids: Vec<String> = in_flight.keys().cloned().collect();
    ids.sort();
    ids
}

/// Broadcasts the set. Called with the lock held, so two changes can't reach the
/// frontend out of order.
fn emit_changed(in_flight: &HashMap<String, InFlight>) {
    let Some(app) = EJECTING_APP.get() else {
        return;
    };
    use tauri_specta::Event as _;
    let payload = VolumesEjectingChanged {
        volume_ids: sorted_ids(in_flight),
    };
    if let Err(e) = payload.emit(app) {
        crate::log_error!(target: "eject", "Failed to emit volumes-ejecting-changed: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::wait_until_async;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn explode() -> Result<(), EjectError> {
        panic!("teardown blew up")
    }

    #[tokio::test]
    async fn a_second_eject_joins_the_one_in_flight_and_gets_its_answer() {
        let vid = "volumes-cmdr-test-joins-in-flight";
        let runs = Arc::new(AtomicUsize::new(0));
        let (release, held) = tokio::sync::oneshot::channel::<()>();

        let first = join_or_start(vid, {
            let runs = Arc::clone(&runs);
            move || async move {
                runs.fetch_add(1, Ordering::SeqCst);
                let _ = held.await;
                Err(EjectError::UnmountRefused {
                    detail: "held by sleep".to_string(),
                })
            }
        });
        assert!(
            ejecting_volume_ids().contains(&vid.to_string()),
            "the volume reads as ejecting while its flight runs"
        );
        wait_until_async(Duration::from_secs(5), "the first teardown to start", || {
            runs.load(Ordering::SeqCst) == 1
        })
        .await;

        // Arrives while the first is still held: it must join, not run its own.
        let second = join_or_start(vid, {
            let runs = Arc::clone(&runs);
            move || async move {
                runs.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        });

        let _ = release.send(());
        let (first, second) = tokio::join!(first, second);

        assert_eq!(
            runs.load(Ordering::SeqCst),
            1,
            "a joined request must not run a second teardown"
        );
        for result in [first, second] {
            assert!(
                matches!(result, Err(EjectError::UnmountRefused { ref detail }) if detail == "held by sleep"),
                "both callers get the one flight's answer, got {result:?}"
            );
        }
        assert!(
            !ejecting_volume_ids().contains(&vid.to_string()),
            "the set clears once the flight lands"
        );
    }

    #[tokio::test]
    async fn a_request_after_the_flight_landed_starts_a_new_one() {
        // A retry after a refusal really retries: nothing caches the old answer.
        let vid = "volumes-cmdr-test-fresh-flight";
        let first = join_or_start(vid, || async { Err(EjectError::TimedOut) }).await;
        assert!(matches!(first, Err(EjectError::TimedOut)), "got {first:?}");

        let second = join_or_start(vid, || async { Ok(()) }).await;
        assert!(second.is_ok(), "got {second:?}");
    }

    #[tokio::test]
    async fn a_teardown_that_panics_answers_unexpected_and_clears_the_set() {
        let vid = "volumes-cmdr-test-panicking-flight";
        let result = join_or_start(vid, || async { explode() }).await;
        assert!(matches!(result, Err(EjectError::Unexpected { .. })), "got {result:?}");
        assert!(
            !ejecting_volume_ids().contains(&vid.to_string()),
            "a panic can't strand the volume in the ejecting set"
        );
    }
}
