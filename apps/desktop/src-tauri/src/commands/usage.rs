//! IPC commands over the launch-day ledger.
//!
//! Read-only. The ledger is appended by Rust at startup (`usage::record_launch`), so the frontend
//! only ever asks it a question: "has this person used Cmdr on at least N days?" See
//! `../usage/CLAUDE.md`.

use tokio::time::Duration;

use crate::commands::util::blocking_with_timeout;

/// The ledger is a small file in the app data dir, but that dir can sit on a hung network home, so
/// the read still carries the standard 2 s read deadline.
const READ_TIMEOUT: Duration = Duration::from_secs(2);

/// How many distinct local calendar days Cmdr has been launched on.
///
/// The gate for usage-gated hints ("you've used Cmdr for a few days now"). A missing, unreadable,
/// or slow ledger answers 0, which keeps every hint that reads this silent rather than firing on a
/// guess.
#[tauri::command]
#[specta::specta]
pub async fn get_launch_day_count(app: tauri::AppHandle) -> u32 {
    let Ok(data_dir) = crate::config::resolved_app_data_dir(&app) else {
        return 0;
    };
    blocking_with_timeout(READ_TIMEOUT, 0, move || crate::usage::launch_day_count(&data_dir)).await
}
