//! ADB volume wiring: turns "the user opened this device" into a registered
//! `AdbVolume`, and keeps the cached device list following the server.
//!
//! ❗ **A backend never registers itself.** This module knows both the backend
//! and the volume registry, and neither of those knows this module: the same
//! shape `mtp::volume_wiring` and `network::sftp_volume_wiring` take.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cmdr_adb::{AdbConnectError, AdbConnectionParams, AdbEndpoint, AdbVolume, DeviceTracker};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::Volume;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::device_provider::{self, AdbDeviceProvider};
use crate::device_volumes::{notify_devices_changed, register_device_provider};
use crate::file_system::volume::manager::get_volume_manager;
use crate::network::connect_wiring::AttemptTable;

/// The `host:track-devices` subscription. Replaceable, not a `OnceLock`: the
/// tracker gives up when no `adb` binary exists, and a re-check has to be able
/// to put a fresh one in its place.
static TRACKER: Mutex<Option<DeviceTracker>> = Mutex::new(None);

/// Files ADB as a device provider. Call once at startup.
pub(crate) fn install_device_provider() {
    register_device_provider(Arc::new(AdbDeviceProvider));
}

/// Points the crate at the `adb` the user named in Settings, or back at the
/// platform search when the setting is empty.
///
/// ❗ Startup seeds this BEFORE the tracker starts, or the first subscription
/// runs against whatever the environment happened to offer.
pub fn set_adb_binary_path(configured: Option<String>) {
    let path = configured
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .map(PathBuf::from);
    cmdr_adb::set_adb_binary_override(path);
}

/// Applies both ADB settings live: where the binary is, and whether Cmdr
/// follows the device list at all.
///
/// Turning it off stops the subscription AND empties the cached list, which is
/// what retires the connected volumes and takes the rows off the switcher: a
/// stopped tracker on its own would leave the last list frozen on screen.
/// Turning it on (or changing the path) clears the crate's start-attempt memory
/// first, because a newly named binary deserves the one attempt an existing one
/// already spent.
pub async fn set_adb_settings(enabled: bool, binary_path: Option<String>) {
    apply_settings_at(AdbEndpoint::default_local(), enabled, binary_path).await;
}

/// [`set_adb_settings`] against whichever server `endpoint` names.
pub(crate) async fn apply_settings_at(endpoint: AdbEndpoint, enabled: bool, binary_path: Option<String>) {
    set_adb_binary_path(binary_path);
    stop_adb_tracker();
    if !enabled {
        log::info!(target: "volume", "ADB support turned off");
        device_provider::apply_device_list(Vec::new());
        return;
    }
    cmdr_adb::forget_start_attempt().await;
    start_tracker_at(endpoint);
}

/// Ends the `host:track-devices` subscription, if one is running.
fn stop_adb_tracker() {
    if let Some(tracker) = TRACKER.lock_ignore_poison().take() {
        tracker.stop();
    }
}

/// Starts following the ADB server's device list. A second call while one is
/// already running is a no-op.
///
/// Talks only to the local server socket, never to USB. With no `adb`
/// installed the tracker stops itself and says so at debug, so nothing reaches
/// the user at startup and nothing retries for the rest of the session;
/// [`recheck_adb_install`] is how it comes back.
pub fn start_adb_tracker() {
    start_tracker_at(AdbEndpoint::default_local());
}

/// [`start_adb_tracker`] against whichever server `endpoint` names.
pub(crate) fn start_tracker_at(endpoint: AdbEndpoint) {
    let mut slot = TRACKER.lock_ignore_poison();
    if slot.as_ref().is_some_and(DeviceTracker::is_running) {
        return;
    }
    *slot = Some(cmdr_adb::track_devices(
        endpoint,
        tauri::async_runtime::handle().inner().clone(),
        Arc::new(device_provider::apply_device_list),
    ));
}

/// Where Cmdr found the `adb` binary, and whether it is following the server.
///
/// Both halves are what a settings screen renders: a path to show, and whether
/// the device list is live. `binary_path` is `None` exactly when
/// `AdbConnectError::AdbNotInstalled` is what a connect would answer.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AdbInstallStatus {
    /// The `adb` binary in use, if one was found.
    pub binary_path: Option<String>,
    /// Whether the `host:track-devices` subscription is live.
    pub tracking: bool,
}

/// What Cmdr currently knows about the ADB install, without looking again.
pub fn adb_install_status() -> AdbInstallStatus {
    AdbInstallStatus {
        binary_path: cmdr_adb::locate_adb_binary().map(|p| p.display().to_string()),
        tracking: TRACKER
            .lock_ignore_poison()
            .as_ref()
            .is_some_and(DeviceTracker::is_running),
    }
}

/// Looks for `adb` again and restarts the tracker if it found one.
///
/// ❗ The one entry point allowed to retry `adb start-server`: it stands for a
/// person saying "I installed it now", so it is one attempt per click, never a
/// loop.
pub async fn recheck_adb_install() -> AdbInstallStatus {
    cmdr_adb::forget_start_attempt().await;
    start_adb_tracker();
    adb_install_status()
}

// ============================================================================
// Calling a connect off
// ============================================================================

/// The dials a user could still call off. ADB's OWN table, never a shared one,
/// for the reason `network/connect_wiring.rs` gives: one table would let a stray
/// cancel from another backend's sign-in reach in here.
static ATTEMPTS: AttemptTable = AttemptTable::new("an adb");

/// Calls off the dial filed under `attempt_id`, answering whether one was
/// running. An id nobody is holding is a plain `false`.
pub fn cancel_connect(attempt_id: &str) -> bool {
    ATTEMPTS.cancel(attempt_id)
}

/// The id a navigation's own dial is filed under.
///
/// A pane that walks onto `adb://<serial>` dials without anyone having minted an
/// attempt id, and per-serial keeps a second navigation onto the same device
/// from cancelling the first: [`AttemptTable`]'s serial makes the repeat replace
/// only its own entry.
fn navigation_attempt_id(serial: &str) -> String {
    format!("adb-navigation:{serial}")
}

/// Dials the device with `serial`, registers its volume, and answers the volume
/// id. Already connected is answered without a second dial.
///
/// `attempt_id` is the CALLER's own name for this dial, and what
/// [`cancel_connect`] needs to call it off: a phone can sit on its "Allow USB
/// debugging?" prompt for as long as nobody picks it up, so the pane has to be
/// able to arm a cancel button before this answers.
pub async fn connect_adb_device(serial: &str, attempt_id: &str) -> Result<String, AdbConnectError> {
    connect_device_at(AdbConnectionParams::new(serial), attempt_id).await
}

/// The dial itself, against whichever server `params` names.
///
/// ❗ **At most one wire dial per serial.** A caller arriving while one runs
/// JOINS it and gets its answer; the Allow tap is exactly when several callers
/// reach for one phone at once. The cancel stays per-attempt: a called-off
/// attempt answers `Cancelled` at once, and the wire dial is called off only
/// when no joined attempt still wants it. A called-off dial leaves nothing
/// behind: no volume registered, nothing remembered, no `volumes-changed`.
pub(crate) async fn connect_device_at(
    params: AdbConnectionParams,
    attempt_id: &str,
) -> Result<String, AdbConnectError> {
    if let Some(volume) = device_provider::connected_volume(&params.serial) {
        return Ok(volume.volume_id().to_string());
    }
    let (cancel, _attempt) = ATTEMPTS.register(attempt_id);
    let mut claim = match join_dial(params) {
        Joined::AlreadyOpen(volume_id) => return Ok(volume_id),
        Joined::Waiting(claim) => claim,
    };
    tokio::select! {
        biased;
        outcome = claim.outcome() => return outcome,
        () = cancel.cancelled() => {}
    }
    // Called off. A dial that answered in the same instant keeps its real
    // answer, because its volume is really there.
    claim.withdraw().unwrap_or(Err(AdbConnectError::Cancelled))
}

// ============================================================================
// One wire dial per phone
// ============================================================================

/// What a finished wire dial answers every attempt that joined it; `None` while
/// it runs.
type DialOutcome = Option<Result<String, AdbConnectError>>;

/// The wire dial running for one serial, and how many attempts still want it.
struct InFlightDial {
    /// Tells this dial apart from a later one for the same serial, so a dial
    /// winding down after being called off only ever removes its OWN entry.
    generation: u64,
    /// Calls the wire dial off. ❗ Fired only when the last claim is withdrawn,
    /// ❌ never by one attempt's cancel.
    wire: CancellationToken,
    wanted_by: usize,
    outcome: watch::Receiver<DialOutcome>,
}

/// The dials running now, by serial.
///
/// ❗ One lock covers joining, withdrawing, and a dial's registering and
/// publishing, so a withdrawal either lands while the dial still runs (and a
/// dial nobody wants then registers nothing) or finds its answer published.
static IN_FLIGHT: Mutex<BTreeMap<String, InFlightDial>> = Mutex::new(BTreeMap::new());

static NEXT_DIAL_GENERATION: AtomicU64 = AtomicU64::new(0);

/// What getting in line for a phone's dial handed back.
enum Joined {
    /// A dial finished while the caller was getting in line.
    AlreadyOpen(String),
    /// Waiting on the dial running for this serial.
    Waiting(DialClaim),
}

/// One attempt's claim on a phone's dial.
///
/// ❗ Withdrawn however the attempt ends (answered, cancelled, or its future
/// dropped), so the last attempt to stop wanting a dial always calls it off.
struct DialClaim {
    serial: String,
    generation: u64,
    outcome: watch::Receiver<DialOutcome>,
    withdrawn: bool,
}

impl DialClaim {
    /// The dial's answer, once it has one.
    async fn outcome(&mut self) -> Result<String, AdbConnectError> {
        let published = self
            .outcome
            .wait_for(Option::is_some)
            .await
            .map(|outcome| (*outcome).clone());
        match published {
            Ok(Some(result)) => result,
            // The sender went away unanswered: the dial task itself is gone.
            Ok(None) | Err(_) => Err(AdbConnectError::Transport(
                "the dial ended without answering".to_string(),
            )),
        }
    }

    /// Stops wanting the dial, calling it off if nobody else does. Answers the
    /// dial's own outcome when it had already published one.
    fn withdraw(&mut self) -> Option<Result<String, AdbConnectError>> {
        if std::mem::replace(&mut self.withdrawn, true) {
            return None;
        }
        let mut dials = IN_FLIGHT.lock_ignore_poison();
        match dials.get_mut(&self.serial).filter(|d| d.generation == self.generation) {
            Some(dial) => {
                dial.wanted_by = dial.wanted_by.saturating_sub(1);
                if dial.wanted_by == 0 {
                    dial.wire.cancel();
                }
                None
            }
            // The dial left the table, and it publishes before letting go of
            // the lock, so its answer is already there.
            None => self.outcome.borrow().clone(),
        }
    }
}

impl Drop for DialClaim {
    fn drop(&mut self) {
        // allowed-discarded-outcome: an attempt that already has its answer, or whose future was dropped, has nobody to hand the dial's answer to.
        let _ = self.withdraw();
    }
}

/// Joins the dial running for `params.serial`, or starts one.
fn join_dial(params: AdbConnectionParams) -> Joined {
    let mut dials = IN_FLIGHT.lock_ignore_poison();
    // ❗ Asked again under the lock: a dial registers and leaves the table under
    // it, so a volume that landed since the caller's first look is found here
    // rather than dialed a second time.
    if let Some(volume) = device_provider::connected_volume(&params.serial) {
        return Joined::AlreadyOpen(volume.volume_id().to_string());
    }
    // A dial everyone already walked away from is winding down; the new caller
    // gets a fresh one rather than an answer of `Cancelled` it never asked for.
    if let Some(dial) = dials.get_mut(&params.serial).filter(|d| !d.wire.is_cancelled()) {
        dial.wanted_by += 1;
        return Joined::Waiting(DialClaim {
            serial: params.serial.clone(),
            generation: dial.generation,
            outcome: dial.outcome.clone(),
            withdrawn: false,
        });
    }
    let generation = NEXT_DIAL_GENERATION.fetch_add(1, Ordering::Relaxed);
    let wire = CancellationToken::new();
    let (answer, outcome) = watch::channel(None);
    let serial = params.serial.clone();
    dials.insert(
        serial.clone(),
        InFlightDial {
            generation,
            wire: wire.clone(),
            wanted_by: 1,
            outcome: outcome.clone(),
        },
    );
    tokio::spawn(run_dial(params, generation, wire, answer));
    Joined::Waiting(DialClaim {
        serial,
        generation,
        outcome,
        withdrawn: false,
    })
}

/// The one wire dial for a serial: connects, registers, and answers everyone
/// who joined.
async fn run_dial(
    params: AdbConnectionParams,
    generation: u64,
    wire: CancellationToken,
    answer: watch::Sender<DialOutcome>,
) {
    let serial = params.serial.clone();
    let dialed = cmdr_adb::connect_adb_volume(params, crate::volume_host::host(), wire.clone()).await;
    let opened = {
        let mut dials = IN_FLIGHT.lock_ignore_poison();
        let outcome = match dialed {
            // Every claim was withdrawn while the last round-trip answered.
            Ok(_) if wire.is_cancelled() => Err(AdbConnectError::Cancelled),
            Ok(volume) => Ok(install_volume(&serial, volume)),
            Err(error) => Err(error),
        };
        if dials.get(&serial).is_some_and(|d| d.generation == generation) {
            dials.remove(&serial);
        }
        let opened = outcome.is_ok();
        answer.send_replace(Some(outcome));
        opened
    };
    if opened {
        notify_devices_changed("adb");
    }
}

/// Registers a freshly dialed volume and remembers the SAME `Arc` by serial, so
/// eject, `note_device_gone`, and `space_for_path` reach the volume panes list.
///
/// `register_if_absent`, never `register`: an ADB device has no OS mount, so
/// nothing else can pre-register its id, and a connect must not retire a volume
/// a pane is using.
fn install_volume(serial: &str, volume: AdbVolume) -> String {
    let volume_id = volume.volume_id().to_string();
    let volume = Arc::new(volume);
    if get_volume_manager().register_if_absent(&volume_id, Arc::clone(&volume) as Arc<dyn Volume>) {
        device_provider::remember_volume(serial, volume);
        log::info!(target: "volume", "registered ADB volume {volume_id}");
    } else {
        // ❗ Not remembered: the provider only ever names the volume the
        // registry holds, and the registry kept its incumbent.
        log::warn!(target: "volume", "ADB volume {volume_id} was already registered; keeping that one");
    }
    volume_id
}

/// How many attempts are waiting on the dial for `serial`, for cells that have
/// to know every caller joined before the dial may answer.
#[cfg(test)]
fn attempts_waiting_on(serial: &str) -> usize {
    IN_FLIGHT
        .lock_ignore_poison()
        .get(serial)
        .map_or(0, |dial| dial.wanted_by)
}

/// Whether a wire dial for `serial` is still running.
#[cfg(test)]
fn dial_in_flight(serial: &str) -> bool {
    IN_FLIGHT.lock_ignore_poison().contains_key(serial)
}

/// The volume id for an `adb://<serial>[/…]` path, dialing the device on first
/// use. `None` for a path that isn't `adb://` at all.
pub async fn volume_id_for_path(path: &str) -> Option<Result<String, AdbConnectError>> {
    let serial = device_provider::serial_of_path(path)?;
    Some(connect_adb_device(serial, &navigation_attempt_id(serial)).await)
}

#[cfg(test)]
#[path = "volume_wiring_test.rs"]
mod volume_wiring_test;
