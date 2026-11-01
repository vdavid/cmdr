//! `connect_volume`: brings a place the user SAVED to life, with the credentials
//! the user stored, on a turn the user started (`Access::Session`; the gate in
//! `view.rs` holds the turn rule, agent invariant 7 the promise).
//!
//! It takes the move `servers/connect-flow.ts` would for the volume's standing,
//! so the agent can't do what the frontend refuses to (re-dialing a registered
//! volume registers a SECOND one; dialing one the backoff loop owns races it):
//!
//! - **Serving** → `alreadyConnected`, no dial.
//! - **Registered, dropped** → no dial: the frontend's reconnect manager is asked
//!   to take it (`VolumeReconnectRequested`, connect-flow's arm 1), and the call
//!   waits up to [`RECONNECT_WAIT`] for how it settles. Recovery keeps its one
//!   owner.
//! - **Registered, waiting for a sign-in or a key** → `needsCredentials` /
//!   `needsApproval`, no dial.
//! - **Not registered** (a `saved` row) → the only dial, through
//!   `saved_servers::connect_saved_place` with no secret: one flight per
//!   place, shared with a pane's own dial (`network::dial_flights`).
//!
//! Before any of that, a share reached by two addresses is one share (`DETAILS.md`
//! § Connecting a saved place): a member whose
//! same-account sibling is serving answers `alreadyConnected` with that sibling's
//! location and `via`; with none serving, the dial goes to the member connected
//! most recently. Both through `VolumeFacts::connect_target`, on the same
//! `redirectable` proof every read uses.
//!
//! Stop drops the call (`agent::chat::runtime::dispatch`), which leaves the dial's
//! flight and calls it off when nobody else waits; the handler also races the
//! token itself so it answers in its own vocabulary.

use std::time::Duration;

use cmdr_fs::volume::ConnectionState;
use cmdr_fs::volume::host::events::VolumeConnection;
use futures_util::future::{BoxFuture, FutureExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Runtime};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::file_system::volume::manager::{ConnectionNews, connection_news, get_volume_manager};
use crate::location::Location;
use crate::location::resolve::{RootSources, VolumeFacts, volume_root};
use crate::location::same_share::Via;
use crate::saved_servers::{SavedPlaceDial, SavedPlaceRefusal, ServerConnectOutcome};
use crate::tools::{ToolError, ToolResult};

/// How long a call waits for a dropped volume's reconnect to settle. The backoff
/// cycle's first attempt goes out after 2 s; past this the model is told it's still
/// reconnecting, ❌ never to poll.
pub(crate) const RECONNECT_WAIT: Duration = Duration::from_secs(10);

/// What the tool answers, as `outcome`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ConnectOutcome {
    Connected,
    AlreadyConnected,
    StillReconnecting,
    NeedsCredentials,
    /// A host key or certificate the user approves in the app.
    NeedsApproval,
    /// The server, or the saved place's own settings, said no.
    Rejected,
    Unreachable,
    NotSaved,
    Cancelled,
}

/// The tool's result: always an answer, ❌ never a `problem`, so the model can
/// branch on `outcome`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConnectAnswer {
    pub outcome: ConnectOutcome,
    /// The volume's root, where its locations start: on `connected` and
    /// `alreadyConnected`. It may have moved since the model last saw it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    /// The volume asked, when another member of its share answered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<Via>,
    /// The dial's own outcome name, when `rejected`, `unreachable`, or
    /// `needsApproval` came from one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
    pub detail: &'static str,
}

impl ConnectAnswer {
    fn of(outcome: ConnectOutcome) -> Self {
        Self {
            outcome,
            location: None,
            via: None,
            reason: None,
            detail: detail(outcome),
        }
    }
}

/// How a registered volume stands, from the registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Standing {
    /// Nothing registered under the id: a saved place, or nothing at all.
    Absent,
    /// Registered; `None` is a local disk, always serving.
    Registered(Option<ConnectionState>),
}

/// How a reconnect the frontend took over ended, as far as the wait saw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Settled {
    Live,
    NeedsCredentials,
    NeedsApproval,
    /// The server offered a guest session for a named login, and Cmdr refused it.
    Refused,
    StillReconnecting,
}

/// What the tool reads and does, so its moves are testable without an app.
pub(crate) trait ConnectPorts: Sync {
    fn facts(&self) -> BoxFuture<'_, VolumeFacts>;
    /// When the saved row was last connected, in seconds since the epoch.
    fn last_connected(&self, volume_id: &str) -> Option<i64>;
    fn standing(&self, volume_id: &str) -> Standing;
    /// Hands a registered, dropped volume to the frontend's reconnect manager and
    /// waits up to [`RECONNECT_WAIT`] for how it settles.
    fn reconnect<'a>(&'a self, volume_id: &'a str) -> BoxFuture<'a, Settled>;
    /// Dials a saved place with its stored credentials.
    fn dial<'a>(&'a self, volume_id: &'a str) -> BoxFuture<'a, SavedPlaceDial>;
    /// A registered volume's root as an app path.
    fn root(&self, volume_id: &str) -> Option<String>;
}

/// `connect_volume`'s schema: one volume id, nothing else. ❌ No secret, account,
/// or address field: the tool only reaches what the user saved.
pub fn connect_volume_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "volumeId": { "type": "string" }
        },
        "required": ["volumeId"],
        "additionalProperties": false
    })
}

/// The registry handler. The agent's own calls come through
/// [`execute_connect_volume_cancellable`] with the turn's Stop.
pub async fn execute_connect_volume<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    execute_connect_volume_cancellable(app, params, &CancellationToken::new()).await
}

/// `connect_volume` under the turn's Stop.
pub(crate) async fn execute_connect_volume_cancellable<R: Runtime>(
    app: &AppHandle<R>,
    params: &Value,
    cancel: &CancellationToken,
) -> ToolResult {
    let volume_id = params
        .get("volumeId")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::invalid_params("connect_volume needs volumeId."))?;
    let answer = run(&AppPorts { app }, volume_id, cancel).await;
    log::info!(
        target: "agent::tools",
        "connect_volume answered {:?}{}",
        answer.outcome,
        answer.reason.map(|reason| format!(" ({reason})")).unwrap_or_default()
    );
    serde_json::to_value(answer).map_err(|e| ToolError::internal(e.to_string()))
}

/// Connect `volume_id`, or answer why not, until Stop.
pub(crate) async fn run(ports: &impl ConnectPorts, volume_id: &str, cancel: &CancellationToken) -> ConnectAnswer {
    tokio::select! {
        biased;
        () = cancel.cancelled() => ConnectAnswer::of(ConnectOutcome::Cancelled),
        answer = connect(ports, volume_id) => answer,
    }
}

async fn connect(ports: &impl ConnectPorts, asked: &str) -> ConnectAnswer {
    let facts = ports.facts().await;
    let Some(target) = facts.connect_target(asked, |id| ports.last_connected(id)) else {
        return ConnectAnswer::of(ConnectOutcome::NotSaved);
    };
    let target = target.id.clone();
    let via = (target != asked).then_some(asked);
    match ports.standing(&target) {
        Standing::Registered(None | Some(ConnectionState::Direct | ConnectionState::OsMount)) => {
            on_volume(ConnectOutcome::AlreadyConnected, &target, via, ports.root(&target))
        }
        // A registered `Saved` doesn't happen (a saved row is unregistered by
        // definition); if it ever does, it's the reconnect manager's, ❌ never a dial.
        Standing::Registered(Some(ConnectionState::Disconnected | ConnectionState::Saved)) => {
            match ports.reconnect(&target).await {
                Settled::Live => on_volume(ConnectOutcome::Connected, &target, via, ports.root(&target)),
                Settled::NeedsCredentials => ConnectAnswer::of(ConnectOutcome::NeedsCredentials),
                Settled::NeedsApproval => ConnectAnswer::of(ConnectOutcome::NeedsApproval),
                Settled::Refused => ConnectAnswer {
                    reason: Some("guest_session_refused"),
                    ..ConnectAnswer::of(ConnectOutcome::Rejected)
                },
                Settled::StillReconnecting => ConnectAnswer::of(ConnectOutcome::StillReconnecting),
            }
        }
        Standing::Registered(Some(ConnectionState::NeedsSignIn)) => ConnectAnswer::of(ConnectOutcome::NeedsCredentials),
        Standing::Registered(Some(ConnectionState::NeedsHostKeyApproval)) => {
            ConnectAnswer::of(ConnectOutcome::NeedsApproval)
        }
        Standing::Absent => answer_dial(via, ports.dial(&target).await, |id| ports.root(id)),
    }
}

/// A dial's answer, in the tool's vocabulary. `root` reads where the volume it
/// landed on starts.
pub(crate) fn answer_dial(
    via: Option<&str>,
    dial: SavedPlaceDial,
    root: impl Fn(&str) -> Option<String>,
) -> ConnectAnswer {
    match dial {
        Ok(ServerConnectOutcome::Connected { volume_id }) => {
            on_volume(ConnectOutcome::Connected, &volume_id, via, root(&volume_id))
        }
        Ok(outcome) => {
            let (class, reason) = classify(&outcome);
            ConnectAnswer {
                via: via.map(via_of),
                reason: matches!(
                    class,
                    ConnectOutcome::Rejected | ConnectOutcome::Unreachable | ConnectOutcome::NeedsApproval
                )
                .then_some(reason),
                ..ConnectAnswer::of(class)
            }
        }
        // A pane's dial registered it between the standing read and this one.
        Err(SavedPlaceRefusal::AlreadyConnected { volume_id }) => {
            on_volume(ConnectOutcome::AlreadyConnected, &volume_id, via, root(&volume_id))
        }
        Err(SavedPlaceRefusal::NoSuchServer { .. }) => ConnectAnswer::of(ConnectOutcome::NotSaved),
    }
}

/// Each dial outcome's class and its wire name. ❗ Exhaustive, no `_ =>`: a new
/// outcome is a compile error here until it's decided.
pub(crate) fn classify(outcome: &ServerConnectOutcome) -> (ConnectOutcome, &'static str) {
    use ConnectOutcome::{Cancelled, Connected, NeedsApproval, NeedsCredentials, Rejected, Unreachable};
    match outcome {
        ServerConnectOutcome::Connected { .. } => (Connected, "connected"),
        ServerConnectOutcome::NeedsHostKeyApproval(_) => (NeedsApproval, "needs_host_key_approval"),
        ServerConnectOutcome::NeedsCertificateApproval(_) => (NeedsApproval, "needs_certificate_approval"),
        ServerConnectOutcome::NeedsCredentials => (NeedsCredentials, "needs_credentials"),
        ServerConnectOutcome::HostKeyRevoked(_) => (Rejected, "host_key_revoked"),
        ServerConnectOutcome::AuthenticationRejected => (Rejected, "authentication_rejected"),
        ServerConnectOutcome::AuthMethodUnsupported => (Rejected, "auth_method_unsupported"),
        ServerConnectOutcome::CertificateUntrusted => (Rejected, "certificate_untrusted"),
        ServerConnectOutcome::NotAWebdavServer => (Rejected, "not_a_webdav_server"),
        ServerConnectOutcome::InvalidUrl => (Rejected, "invalid_url"),
        ServerConnectOutcome::AccessDenied => (Rejected, "access_denied"),
        ServerConnectOutcome::BucketListRefused => (Rejected, "bucket_list_refused"),
        ServerConnectOutcome::BucketNotFound => (Rejected, "bucket_not_found"),
        ServerConnectOutcome::RegionMismatch { .. } => (Rejected, "region_mismatch"),
        ServerConnectOutcome::ClockSkewed => (Rejected, "clock_skewed"),
        ServerConnectOutcome::NotAnS3Endpoint => (Rejected, "not_an_s3_endpoint"),
        ServerConnectOutcome::ExportNotFound => (Rejected, "export_not_found"),
        ServerConnectOutcome::ExportRefused => (Rejected, "export_refused"),
        ServerConnectOutcome::ReservedPortRequired => (Rejected, "reserved_port_required"),
        ServerConnectOutcome::NoNfsV3 => (Rejected, "no_nfs_v3"),
        ServerConnectOutcome::StartFolderOutsideRoot => (Rejected, "start_folder_outside_root"),
        ServerConnectOutcome::TimedOut => (Unreachable, "timed_out"),
        ServerConnectOutcome::SignInHelperStuck => (Unreachable, "sign_in_helper_stuck"),
        ServerConnectOutcome::Unreachable => (Unreachable, "unreachable"),
        ServerConnectOutcome::Cancelled => (Cancelled, "cancelled"),
    }
}

/// An answer about a volume that is (or just became) live: its root as the location.
fn on_volume(outcome: ConnectOutcome, volume_id: &str, via: Option<&str>, root: Option<String>) -> ConnectAnswer {
    ConnectAnswer {
        location: root.map(|path| Location {
            volume_id: volume_id.to_string(),
            path,
        }),
        via: via.map(via_of),
        ..ConnectAnswer::of(outcome)
    }
}

fn via_of(asked: &str) -> Via {
    Via {
        volume_id: asked.to_string(),
    }
}

/// What the model is told, per outcome. Model-facing, so English and terse; the
/// user reads the model's words, ❌ never these.
fn detail(outcome: ConnectOutcome) -> &'static str {
    match outcome {
        ConnectOutcome::Connected => "Connected. Its locations start at location.path.",
        ConnectOutcome::AlreadyConnected => {
            "Already connected; nothing was dialed. Its locations start at location.path."
        }
        ConnectOutcome::StillReconnecting => {
            "Cmdr is still reconnecting it. Tell the user it's reconnecting; don't call again to check."
        }
        ConnectOutcome::NeedsCredentials => {
            "It needs a password Cmdr doesn't have saved. The user can open it from the volume switcher and sign in."
        }
        ConnectOutcome::NeedsApproval => {
            "The server's identity (a host key or certificate) needs the user's approval. They can open it from the volume switcher."
        }
        ConnectOutcome::Rejected => "The server refused the connection; reason says why.",
        ConnectOutcome::Unreachable => "The server didn't answer; reason says how.",
        ConnectOutcome::NotSaved => {
            "No saved server or share has this volumeId. list_volumes has the ids; only places the user saved can be connected."
        }
        ConnectOutcome::Cancelled => "The user stopped this turn before the connect finished.",
    }
}

/// Wait for `volume_id`'s next transition that ends a reconnect, up to `within`.
/// `news` must be subscribed BEFORE the reconnect is asked for. `current` reads the
/// registry when the channel lagged past news it can't replay.
pub(crate) async fn settle(
    mut news: broadcast::Receiver<ConnectionNews>,
    volume_id: String,
    within: Duration,
    current: impl Fn() -> Option<Settled>,
) -> Settled {
    let wait = async {
        loop {
            match news.recv().await {
                Ok(item) if item.volume_id == volume_id => match item.connection {
                    VolumeConnection::Connected => return Settled::Live,
                    VolumeConnection::NeedsCredentials => return Settled::NeedsCredentials,
                    VolumeConnection::NeedsHostKeyApproval => return Settled::NeedsApproval,
                    VolumeConnection::GuestSessionRefused => return Settled::Refused,
                    // An attempt that didn't land; the cycle tries again.
                    VolumeConnection::Disconnected => {}
                },
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    if let Some(settled) = current() {
                        return settled;
                    }
                }
                // The sender lives for the process; nothing more will come.
                Err(broadcast::error::RecvError::Closed) => std::future::pending::<()>().await,
            }
        }
    };
    // A bare timeout is right here: dropping a channel receive leaves nothing half-done.
    tokio::time::timeout(within, wait)
        .await
        .unwrap_or(Settled::StillReconnecting)
}

/// The app behind [`ConnectPorts`].
struct AppPorts<'a, R: Runtime> {
    app: &'a AppHandle<R>,
}

impl<R: Runtime> ConnectPorts for AppPorts<'_, R> {
    fn facts(&self) -> BoxFuture<'_, VolumeFacts> {
        VolumeFacts::now().boxed()
    }

    fn last_connected(&self, volume_id: &str) -> Option<i64> {
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            let row = crate::network::known_shares::share_by_volume_id(volume_id)?;
            chrono::DateTime::parse_from_rfc3339(&row.last_connected_at)
                .ok()
                .map(|at| at.timestamp())
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            let _ = volume_id;
            None
        }
    }

    fn standing(&self, volume_id: &str) -> Standing {
        get_volume_manager().get(volume_id).map_or(Standing::Absent, |volume| {
            Standing::Registered(volume.connection_state())
        })
    }

    fn reconnect<'a>(&'a self, volume_id: &'a str) -> BoxFuture<'a, Settled> {
        async move {
            // Subscribed first, so a reconnect that lands at once is still heard.
            let news = connection_news();
            let now = || match self.standing(volume_id) {
                Standing::Registered(None | Some(ConnectionState::Direct | ConnectionState::OsMount)) => {
                    Some(Settled::Live)
                }
                Standing::Registered(Some(ConnectionState::NeedsSignIn)) => Some(Settled::NeedsCredentials),
                Standing::Registered(Some(ConnectionState::NeedsHostKeyApproval)) => Some(Settled::NeedsApproval),
                Standing::Registered(Some(ConnectionState::Disconnected | ConnectionState::Saved))
                | Standing::Absent => None,
            };
            request_reconnect(self.app, volume_id);
            settle(news, volume_id.to_string(), RECONNECT_WAIT, now).await
        }
        .boxed()
    }

    fn dial<'a>(&'a self, volume_id: &'a str) -> BoxFuture<'a, SavedPlaceDial> {
        async move {
            // Nothing outside this call cancels it: Stop drops the future, which
            // leaves the dial's flight (`network::dial_flights`).
            let attempt_id = format!("ask-cmdr-connect-{}", uuid::Uuid::new_v4());
            crate::saved_servers::connect_saved_place(volume_id, &attempt_id, None, None).await
        }
        .boxed()
    }

    fn root(&self, volume_id: &str) -> Option<String> {
        let volume = get_volume_manager().get(volume_id)?;
        let live = volume.root().to_string_lossy().into_owned();
        volume_root(RootSources {
            live: Some(&live),
            ..RootSources::default()
        })
    }
}

/// Asks the frontend's reconnect manager to take `volume_id`, the move connect-flow's
/// arm 1 makes. With no window listening, the wait answers `stillReconnecting`,
/// which is honest.
fn request_reconnect<R: Runtime>(app: &AppHandle<R>, volume_id: &str) {
    use tauri_specta::Event;
    let event = crate::network::VolumeReconnectRequested {
        volume_id: volume_id.to_string(),
    };
    if let Err(e) = event.emit(app) {
        log::warn!(target: "agent::tools", "connect_volume couldn't ask the frontend to reconnect {volume_id}: {e}");
    }
}

#[cfg(test)]
#[path = "connect_tests.rs"]
mod tests;
