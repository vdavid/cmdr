//! Reporting a long listing's progress while it runs.
//!
//! [`Volume::list_directory`](super::Volume::list_directory) answers once, with
//! every row, and its `on_progress` callback is `Sync` but not `Send`, so it
//! can't ride into the blocking task doing the reading. The reader publishes a
//! running count somewhere shared instead, and [`report_while`] samples it on a
//! timer from the async side that owns the callback. Without it a pane sits on
//! "Opening folder…" for a big listing's whole read.

use std::future::Future;
use std::time::Duration;

use super::ListingProgress;

/// Awaits `listing`, handing `on_progress` a `sample()` every `interval` until
/// it lands, and answers what `listing` answered.
///
/// A sample with nothing counted yet is skipped: the blocking pool may not
/// have picked the reader up, and a zero would render as "Loaded 0 items". With
/// no `on_progress`, it only awaits. The final, exact count is the caller's to
/// report from the rows it got back.
pub async fn report_while<T>(
    listing: impl Future<Output = T>,
    interval: Duration,
    sample: impl Fn() -> ListingProgress,
    on_progress: Option<&(dyn Fn(ListingProgress) + Sync)>,
) -> T {
    let Some(on_progress) = on_progress else {
        return listing.await;
    };
    let mut listing = std::pin::pin!(listing);
    loop {
        tokio::select! {
            // Biased so a listing that finished during the tick answers instead of
            // reporting one more approximate count.
            biased;
            answer = &mut listing => return answer,
            () = crate::rt::time::sleep(interval) => {
                let progress = sample();
                if progress.entries() > 0 {
                    on_progress(progress);
                }
            }
        }
    }
}
