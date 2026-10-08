//! Every `tauri_specta::Event` payload the network module emits, and the enums
//! only those payloads carry.
//!
//! They live in this always-compiled module, ❌ never in a macOS/Linux-only
//! backend: `collect_events!` in `ipc.rs` can't cfg-gate inline, so each type has
//! to resolve on every platform. `mod.rs` re-exports them, so callers name them
//! `crate::network::X`.
//!
//! ❗ A struct name kebab-cases to its wire event name, so renaming one here
//! silently renames the event the frontend listens for.

use serde::{Deserialize, Serialize};
use tauri_specta::Event;

use super::smb_connect_failure;
use super::{DiscoveryState, NetworkHost};

/// Typed `network-host-found` Tauri event. The payload is the bare `NetworkHost`
/// object (flattened), matching the historic `emit("network-host-found", &host)`
/// wire shape. Struct name kebab-cases to `network-host-found`.
#[derive(Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct NetworkHostFound {
    #[serde(flatten)]
    pub host: NetworkHost,
}

/// Typed `network-host-resolved` Tauri event. Same flattened-host payload as
/// `network-host-found`, emitted when a host's hostname / IP is resolved.
#[derive(Clone, Serialize, Deserialize, specta::Type, Event)]
pub struct NetworkHostResolved {
    #[serde(flatten)]
    pub host: NetworkHost,
}

/// Typed `network-host-lost` Tauri event. Carries the gone host's id.
#[derive(Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct NetworkHostLost {
    pub id: String,
}

/// Typed `network-discovery-state-changed` Tauri event.
#[derive(Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct NetworkDiscoveryStateChanged {
    pub state: DiscoveryState,
}

/// What the user picked in an SMB host's context menu.
///
/// ❗ A typed enum, ❌ never a free string: the frontend branches on every one of
/// these, and a misspelling would go to the one place a compiler never looks.
///
/// ❗ `ForgetSecret` is spelled the way
/// [`crate::volume_broadcast::VolumeContextActionKind`] spells the same act on a
/// server ROW. One act, one name on both sides: two spellings for "stop
/// remembering this password" is how a handler ends up wired to one of them and
/// silently missing the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkHostContextActionKind {
    /// Drop a manually-added host. ❗ A discovered one has nothing to forget.
    ForgetServer,
    /// Stop remembering this host's credential, keeping the host.
    ForgetSecret,
    /// Unmount every share mounted from this host.
    Disconnect,
    /// Open the edit sheet on a SAVED host (its name, for now).
    Edit,
}

/// Typed `network-host-context-action` Tauri event. Emitted to the `main` window
/// when the user picks an action from a network host's native context menu.
/// Window-scoped, so it's emitted via `Event::emit_to` from `menu::menu_handlers`.
#[derive(Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct NetworkHostContextAction {
    /// Which item was picked.
    pub action: NetworkHostContextActionKind,
    /// The servers hub row the menu was opened on, exactly as the hub keys it.
    /// ❗ What every answer acts on: two rows can share one discovered host (a
    /// saved host and the share history of the same machine), so the host id
    /// alone can't say which row was right-clicked.
    pub row_id: String,
    /// The discovered host behind that row, which Disconnect unmounts from.
    pub host_id: String,
    pub host_name: String,
}

/// How a connecting volume's session looks to the frontend, as carried by
/// `volume-connection-changed`.
///
/// Deliberately wider than any backend's internal state machine. SMB's
/// `ConnectionState` (`crates/cmdr-smb/src/volume/state.rs`) is binary,
/// `Direct ⇄ Disconnected`, and widens into `Connected` / `Disconnected` through a
/// `From` impl there. `NeedsCredentials` has no counterpart in it: no backend ever
/// rests in that state, it rides alongside a failed reconnect attempt. The OS-mount
/// fallback likewise lives only at the outer `ConnectionState` layer (driven by
/// `enrich_from_volume_registry`), never here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum VolumeConnection {
    /// The backend holds a live session; operations take the fast path.
    Connected,
    /// The session dropped. The frontend runs the per-volume backoff cycle.
    Disconnected,
    /// A reconnect gave up because the saved credentials no longer work (the password
    /// changed on the server), so the reconnect manager shows a "Sign in" prompt
    /// instead of the generic "unreachable" banner. Transient: it accompanies a failed
    /// attempt rather than describing a state the volume settles in.
    NeedsCredentials,
    /// The server's SSH host key isn't the one this machine trusts for it, so the
    /// backend stopped rather than reconnecting. ❗ Never collapsed into
    /// [`NeedsCredentials`](Self::NeedsCredentials): a changed key is the shape a
    /// man-in-the-middle takes, and a sign-in prompt in front of one is how a password
    /// gets typed into it. Recovery is the user opening the server again, where the
    /// connect command's (`connectServer` / `connectSavedPlace`) typed outcome
    /// carries the fingerprint to look at.
    ///
    /// ❗ Payload-free, and it stays that way: this enum is `Copy` on both sides of
    /// `events::volume_mapping::wire_state`.
    NeedsHostKeyApproval,
}

/// Typed `volume-connection-changed` Tauri event. The frontend reconnect manager
/// listens for this and runs the per-volume backoff cycle.
///
/// Backend-neutral on purpose: SMB is the only emitter today, and the next connecting
/// backend (FTP, S3, SFTP) reuses this channel instead of adding a parallel one.
/// The backend emit site builds and emits it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct VolumeConnectionChanged {
    pub volume_id: String,
    pub state: VolumeConnection,
}

/// Typed `smb-fell-back-to-os-mount` Tauri event: a share Cmdr tried to take over
/// with its own smb2 session is staying on the macOS kernel mount instead.
///
/// This is the one moment nothing else in the app announces. The yellow
/// `connectionState` dot shows the resulting STATE, but a fallback that happens
/// while the user is elsewhere (the startup pass, an auto-remount) is otherwise
/// silent, and the share keeps working at a fraction of the speed. The frontend
/// raises a notice with a retry button.
///
/// Emitted at most once per server per app run (`network::os_mount_notice`), so a
/// NAS whose password went stale speaks once rather than once per mounted share.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct SmbFellBackToOsMount {
    /// The volume that stayed on the kernel mount. This is what the notice's retry
    /// button hands to `upgrade_to_smb_volume`.
    pub volume_id: String,
    /// The share's name, which is what the notice names (`archive`, not `//nas/archive`).
    pub share: String,
    /// Why the direct connection didn't happen, so the notice can tell a
    /// condition that may pass from one that won't. Only
    /// `ShareNotOnServer` can't be fixed by pressing the button again, and the
    /// notice drops the button for it rather than offering a retry that is
    /// certain to land on the same answer.
    pub reason: smb_connect_failure::UpgradeFailure,
    /// The server's friendly name (mDNS hostname, else the address), for the
    /// sentence that names the server: `BlockedByThisMac` says what Cmdr couldn't
    /// connect to.
    pub display_name: String,
}

/// Typed `smb-os-mount-notice-withdrawn` Tauri event: the notice a
/// [`SmbFellBackToOsMount`] raised for this volume has nothing left to offer, so
/// the frontend takes it down.
///
/// Emitted when the share's "Use Cmdr's fast direct connection" switch goes off
/// (`smb_direct_switch`): the notice's button would do exactly what the user just
/// opted out of. The other two ways a notice goes moot (the share goes direct, or
/// leaves the list) already ride `volumes-changed`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct SmbOsMountNoticeWithdrawn {
    /// The volume the withdrawn notice named, matching its `SmbFellBackToOsMount::volume_id`.
    pub volume_id: String,
}
