//! How long `path_exists` waits: a live session's slow answer is still an answer,
//! and a volume with nothing under it that can tell keeps the 2 s read tier.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{ConnectionState, InMemoryVolume, Volume};
use crate::test_support::SlowVolume;

use super::path_exists;

/// Registers a volume holding `/share/photos` whose every read takes `delay`,
/// and hands back its id. Unregister when done.
async fn slow_share(state: Option<ConnectionState>, delay: Duration) -> String {
    let id = format!("path-exists-budget-{}", uuid::Uuid::new_v4());
    let mut volume = InMemoryVolume::new("Share");
    if let Some(state) = state {
        volume = volume.with_connection_state(state);
    }
    volume.create_directory(Path::new("/share")).await.unwrap();
    volume.create_directory(Path::new("/share/photos")).await.unwrap();
    get_volume_manager().register(&id, Arc::new(SlowVolume::new(volume, delay)) as Arc<dyn Volume>);
    id
}

/// A busy NAS holds one `stat` for seconds while its session stays healthy. A
/// "couldn't tell" there sends a volume switch to the share root instead of the
/// folder the user left, and a walk-up past a parent that's still there.
#[tokio::test(start_paused = true)]
async fn a_slow_stat_on_a_live_session_still_answers() {
    let id = slow_share(Some(ConnectionState::Direct), Duration::from_secs(3)).await;
    let answer = path_exists(Some(id.clone()), "/share/photos".to_string()).await;
    get_volume_manager().unregister(&id);
    assert!(answer.data && !answer.timed_out, "{answer:?}");
}

/// No session of its own (a phone, a kernel mount): the 2 s tier stands, since a
/// wedged mount would otherwise hold every caller for minutes.
#[tokio::test(start_paused = true)]
async fn a_slow_stat_without_a_session_still_times_out() {
    let id = slow_share(None, Duration::from_secs(3)).await;
    let answer = path_exists(Some(id.clone()), "/share/photos".to_string()).await;
    get_volume_manager().unregister(&id);
    assert!(answer.timed_out, "{answer:?}");
}
