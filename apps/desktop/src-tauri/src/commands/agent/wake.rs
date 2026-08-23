//! The wake loop's IPC surface: the live-apply push behind its three settings.
//!
//! The loop owns its own thread, its own inbox, and a timer parked against a deadline, so a
//! settings change has to REACH it rather than being noticed the next time something happens.
//! Everything here is a message on the never-dropped control lane; nothing touches the inbox.

use crate::agent::wake::{WakeControl, send_control};

/// Tell the wake loop its settings moved, so it re-reads them and re-arms.
///
/// Called from `settings-applier.ts` for each of the three `askCmdr` cadence rows. ⚠️ **Not
/// optional, and not the same shape as the other two `askCmdr.*` settings**, which the backend
/// reads fresh at send time: these drive a SLEEPING timer. Without this push, turning the
/// proactive gate on would leave a parked scheduler that never notices, and a lengthened
/// cadence would never reach the rows already waiting (the inbox merge is min-only on purpose).
///
/// No value crosses: the loop re-reads `settings.json` itself, so whatever is on disk when the
/// message is serviced wins and two changes racing can't leave the loop on the older one.
#[tauri::command]
#[specta::specta]
pub fn ask_cmdr_wake_settings_changed() {
    send_control(WakeControl::SettingsChanged);
}
