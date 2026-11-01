//! `connect_volume`'s moves: which member of a share it acts on, the move each
//! standing allows (only a saved place is dialed), the reconnect wait, Stop, and
//! every dial outcome in the model's vocabulary.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use cmdr_fs::volume::ConnectionState;
use futures_util::future::{BoxFuture, FutureExt};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::*;
use crate::location::resolve::{IndexSources, VolumeFact, VolumeFacts};
use crate::location::share_identity::ShareIdentity;
use crate::network::server_guid::GuidProof;
use crate::tools::resources::volumes::VolumeKind;

const BOX: &str = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";

/// The app as one test sees it: volumes, how each stands, and what a dial or a
/// reconnect would answer. Counts every dial and reconnect it's asked for.
struct Fake {
    volumes: Vec<VolumeFact>,
    standings: Vec<(&'static str, Standing)>,
    recency: Vec<(&'static str, i64)>,
    dial_answers: SavedPlaceDial,
    reconnect_settles: Settled,
    dialed: Mutex<Vec<String>>,
    reconnects: AtomicUsize,
    /// When set, a dial or reconnect never answers: Stop is the only way out.
    hang: bool,
}

impl Fake {
    fn new(volumes: Vec<VolumeFact>) -> Self {
        Self {
            volumes,
            standings: Vec::new(),
            recency: Vec::new(),
            dial_answers: Ok(ServerConnectOutcome::Unreachable),
            reconnect_settles: Settled::StillReconnecting,
            dialed: Mutex::new(Vec::new()),
            reconnects: AtomicUsize::new(0),
            hang: false,
        }
    }

    fn standing_of(mut self, id: &'static str, standing: Standing) -> Self {
        self.standings.push((id, standing));
        self
    }

    fn dialed(&self) -> Vec<String> {
        self.dialed.lock().expect("test lock").clone()
    }
}

impl ConnectPorts for Fake {
    fn facts(&self) -> BoxFuture<'_, VolumeFacts> {
        let facts = VolumeFacts {
            volumes: self.volumes.clone(),
            mount_areas: vec!["/Volumes".to_string()],
        };
        async move { facts }.boxed()
    }

    fn last_connected(&self, volume_id: &str) -> Option<i64> {
        self.recency.iter().find(|(id, _)| *id == volume_id).map(|(_, at)| *at)
    }

    fn standing(&self, volume_id: &str) -> Standing {
        self.standings
            .iter()
            .find(|(id, _)| *id == volume_id)
            .map_or(Standing::Absent, |(_, standing)| *standing)
    }

    fn reconnect<'a>(&'a self, _volume_id: &'a str) -> BoxFuture<'a, Settled> {
        self.reconnects.fetch_add(1, Ordering::SeqCst);
        let (hang, settles) = (self.hang, self.reconnect_settles);
        async move {
            if hang {
                std::future::pending::<()>().await;
            }
            settles
        }
        .boxed()
    }

    fn dial<'a>(&'a self, volume_id: &'a str) -> BoxFuture<'a, SavedPlaceDial> {
        self.dialed.lock().expect("test lock").push(volume_id.to_string());
        let (hang, answer) = (self.hang, self.dial_answers.clone());
        async move {
            if hang {
                std::future::pending::<()>().await;
            }
            answer
        }
        .boxed()
    }

    fn root(&self, volume_id: &str) -> Option<String> {
        match volume_id {
            "lan" => Some("/Volumes/naspi-2".to_string()),
            "ts" => Some("/Volumes/naspi".to_string()),
            _ => None,
        }
    }
}

fn volume(id: &str, root: &str, live: bool, state: ConnectionState, username: &str) -> VolumeFact {
    VolumeFact {
        id: id.to_string(),
        name: id.to_string(),
        kind: VolumeKind::Smb,
        root: root.to_string(),
        live,
        connection_state: Some(state),
        earlier_roots: Vec::new(),
        index: IndexSources::none(id),
        share: Some(ShareIdentity::new(
            Some((BOX, GuidProof::Authenticated)),
            "naspi",
            Some(username),
        )),
    }
}

fn saved(id: &str, root: &str) -> VolumeFact {
    volume(id, root, false, ConnectionState::Saved, "david")
}

async fn connect(ports: &Fake, volume_id: &str) -> Value {
    serde_json::to_value(run(ports, volume_id, &CancellationToken::new()).await).expect("serializes")
}

// ── Which member ──────────────────────────────────────────────────────

#[tokio::test]
async fn a_member_whose_sibling_is_live_answers_already_connected_via_it_and_never_dials() {
    let ports = Fake::new(vec![
        saved("ts", "/Volumes/naspi"),
        volume("lan", "/Volumes/naspi-1", true, ConnectionState::Direct, "david"),
    ])
    .standing_of("lan", Standing::Registered(Some(ConnectionState::Direct)));

    let answer = connect(&ports, "ts").await;

    assert_eq!(answer["outcome"], "alreadyConnected");
    assert_eq!(
        answer["location"],
        json!({ "volumeId": "lan", "path": "/Volumes/naspi-2" })
    );
    assert_eq!(answer["via"], json!({ "volumeId": "ts" }));
    assert!(ports.dialed().is_empty());
}

#[tokio::test]
async fn with_no_member_live_the_more_recently_connected_one_is_dialed() {
    let mut ports = Fake::new(vec![saved("ts", "/Volumes/naspi"), saved("lan", "/Volumes/naspi-1")]);
    ports.recency = vec![("ts", 1_000), ("lan", 9_000)];
    ports.dial_answers = Ok(ServerConnectOutcome::Connected {
        volume_id: "lan".to_string(),
    });

    let answer = connect(&ports, "ts").await;

    assert_eq!(ports.dialed(), ["lan"]);
    assert_eq!(answer["outcome"], "connected");
    assert_eq!(
        answer["location"],
        json!({ "volumeId": "lan", "path": "/Volumes/naspi-2" }),
        "the root it landed on, which may have moved"
    );
    assert_eq!(answer["via"], json!({ "volumeId": "ts" }));
}

#[tokio::test]
async fn another_accounts_member_is_never_picked() {
    let mut ports = Fake::new(vec![
        saved("ts", "/Volumes/naspi"),
        volume("lan", "/Volumes/naspi-1", true, ConnectionState::Direct, "dori"),
    ])
    .standing_of("lan", Standing::Registered(Some(ConnectionState::Direct)));
    ports.recency = vec![("ts", 1_000), ("lan", 9_000)];

    let answer = connect(&ports, "ts").await;

    assert_eq!(ports.dialed(), ["ts"]);
    assert!(answer.get("via").is_none(), "{answer}");
}

// ── One move per standing ─────────────────────────────────────────────

#[tokio::test]
async fn a_live_volume_answers_already_connected_without_a_dial() {
    let ports = Fake::new(vec![volume(
        "lan",
        "/Volumes/naspi-1",
        true,
        ConnectionState::Direct,
        "david",
    )])
    .standing_of("lan", Standing::Registered(Some(ConnectionState::Direct)));

    let answer = connect(&ports, "lan").await;

    assert_eq!(answer["outcome"], "alreadyConnected");
    assert_eq!(answer["location"]["path"], "/Volumes/naspi-2");
    assert!(answer.get("via").is_none());
    assert!(ports.dialed().is_empty());
    assert_eq!(ports.reconnects.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_dropped_volume_is_handed_to_the_reconnect_manager_once_and_never_dialed() {
    let mut ports = Fake::new(vec![volume(
        "lan",
        "/Volumes/naspi-1",
        true,
        ConnectionState::Disconnected,
        "david",
    )])
    .standing_of("lan", Standing::Registered(Some(ConnectionState::Disconnected)));
    ports.reconnect_settles = Settled::Live;

    let answer = connect(&ports, "lan").await;

    assert_eq!(ports.reconnects.load(Ordering::SeqCst), 1);
    assert!(ports.dialed().is_empty());
    assert_eq!(answer["outcome"], "connected");
    assert_eq!(answer["location"]["path"], "/Volumes/naspi-2");
}

#[tokio::test]
async fn a_reconnect_still_going_says_so_and_never_asks_to_be_polled() {
    let ports = Fake::new(vec![volume(
        "lan",
        "/Volumes/naspi-1",
        true,
        ConnectionState::Disconnected,
        "david",
    )])
    .standing_of("lan", Standing::Registered(Some(ConnectionState::Disconnected)));

    let answer = connect(&ports, "lan").await;

    assert_eq!(answer["outcome"], "stillReconnecting");
    assert!(answer.get("location").is_none());
}

#[tokio::test]
async fn a_volume_waiting_for_a_sign_in_needs_credentials_without_a_dial() {
    let ports = Fake::new(vec![volume(
        "lan",
        "/Volumes/naspi-1",
        true,
        ConnectionState::NeedsSignIn,
        "david",
    )])
    .standing_of("lan", Standing::Registered(Some(ConnectionState::NeedsSignIn)));

    let answer = connect(&ports, "lan").await;

    assert_eq!(answer["outcome"], "needsCredentials");
    assert!(ports.dialed().is_empty());
    assert_eq!(ports.reconnects.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_saved_place_is_the_only_standing_that_dials() {
    let mut ports = Fake::new(vec![saved("ts", "/Volumes/naspi")]);
    ports.dial_answers = Ok(ServerConnectOutcome::Connected {
        volume_id: "ts".to_string(),
    });

    let answer = connect(&ports, "ts").await;

    assert_eq!(ports.dialed(), ["ts"]);
    assert_eq!(answer["outcome"], "connected");
    assert!(answer.get("via").is_none());
}

#[tokio::test]
async fn an_id_nothing_saved_answers_not_saved() {
    let ports = Fake::new(Vec::new());

    let answer = connect(&ports, "smb-who-knows").await;

    assert_eq!(answer["outcome"], "notSaved");
    assert!(ports.dialed().is_empty());
}

// ── Stop ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn stop_ends_a_dial_and_answers_cancelled() {
    let mut ports = Fake::new(vec![saved("ts", "/Volumes/naspi")]);
    ports.hang = true;
    let cancel = CancellationToken::new();

    let call = run(&ports, "ts", &cancel);
    tokio::pin!(call);
    assert!(futures_util::poll!(&mut call).is_pending(), "the dial is out");
    cancel.cancel();
    let answer = serde_json::to_value(call.await).expect("serializes");

    assert_eq!(answer["outcome"], "cancelled");
    assert_eq!(ports.dialed(), ["ts"]);
}

#[tokio::test]
async fn stop_ends_a_reconnect_wait_and_answers_cancelled() {
    let mut ports = Fake::new(vec![volume(
        "lan",
        "/Volumes/naspi-1",
        true,
        ConnectionState::Disconnected,
        "david",
    )])
    .standing_of("lan", Standing::Registered(Some(ConnectionState::Disconnected)));
    ports.hang = true;
    let cancel = CancellationToken::new();

    let call = run(&ports, "lan", &cancel);
    tokio::pin!(call);
    assert!(futures_util::poll!(&mut call).is_pending());
    cancel.cancel();

    assert_eq!(
        serde_json::to_value(call.await).expect("serializes")["outcome"],
        "cancelled"
    );
}

// ── The reconnect wait ────────────────────────────────────────────────

fn news(volume_id: &str, connection: VolumeConnection) -> ConnectionNews {
    ConnectionNews {
        volume_id: volume_id.to_string(),
        connection,
    }
}

#[tokio::test(start_paused = true)]
async fn a_reconnect_that_lands_inside_the_window_answers_live() {
    let (sender, receiver) = broadcast::channel(8);
    let wait = tokio::spawn(settle(receiver, "lan".to_string(), RECONNECT_WAIT, || None));

    sender
        .send(news("other", VolumeConnection::Connected))
        .expect("a listener");
    sender
        .send(news("lan", VolumeConnection::Disconnected))
        .expect("a listener");
    sender
        .send(news("lan", VolumeConnection::Connected))
        .expect("a listener");

    assert_eq!(wait.await.expect("joins"), Settled::Live);
}

#[tokio::test(start_paused = true)]
async fn a_reconnect_that_stops_on_a_credential_or_a_key_says_which() {
    for (connection, settled) in [
        (VolumeConnection::NeedsCredentials, Settled::NeedsCredentials),
        (VolumeConnection::NeedsHostKeyApproval, Settled::NeedsApproval),
        (VolumeConnection::GuestSessionRefused, Settled::Refused),
    ] {
        let (sender, receiver) = broadcast::channel(8);
        let wait = tokio::spawn(settle(receiver, "lan".to_string(), RECONNECT_WAIT, || None));
        sender.send(news("lan", connection)).expect("a listener");
        assert_eq!(wait.await.expect("joins"), settled, "{connection:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_reconnect_with_no_news_in_the_window_is_still_reconnecting() {
    let (_sender, receiver) = broadcast::channel(8);

    assert_eq!(
        settle(receiver, "lan".to_string(), Duration::from_secs(10), || None).await,
        Settled::StillReconnecting
    );
}

// ── Every dial outcome ────────────────────────────────────────────────

/// Every variant, so a new one the mapping doesn't know fails here as well as in
/// the exhaustive match.
#[test]
fn every_dial_outcome_maps_to_the_models_vocabulary() {
    let host_key = cmdr_sftp::transport::HostKeyPrompt {
        host: "nas.local".to_string(),
        port: 22,
        algorithm: "ssh-ed25519".to_string(),
        fingerprint: "SHA256:abc".to_string(),
        kind: cmdr_sftp::transport::HostKeyPromptKind::Unknown,
    };
    let certificate = cmdr_fs::volume::tls::CertificatePrompt {
        host: "nas.local".to_string(),
        port: 5006,
        fingerprint: "AB:CD".to_string(),
        kind: cmdr_fs::volume::tls::CertificatePromptKind::Unknown,
    };
    let revoked = crate::saved_servers::outcome::SftpHostKeyIdentity {
        algorithm: "ssh-ed25519".to_string(),
        fingerprint: "SHA256:abc".to_string(),
    };
    let cases: Vec<(ServerConnectOutcome, ConnectOutcome)> = vec![
        (
            ServerConnectOutcome::Connected {
                volume_id: "v".to_string(),
            },
            ConnectOutcome::Connected,
        ),
        (
            ServerConnectOutcome::NeedsHostKeyApproval(host_key),
            ConnectOutcome::NeedsApproval,
        ),
        (
            ServerConnectOutcome::NeedsCertificateApproval(certificate),
            ConnectOutcome::NeedsApproval,
        ),
        (ServerConnectOutcome::HostKeyRevoked(revoked), ConnectOutcome::Rejected),
        (ServerConnectOutcome::AuthenticationRejected, ConnectOutcome::Rejected),
        (ServerConnectOutcome::NeedsCredentials, ConnectOutcome::NeedsCredentials),
        (ServerConnectOutcome::AuthMethodUnsupported, ConnectOutcome::Rejected),
        (ServerConnectOutcome::CertificateUntrusted, ConnectOutcome::Rejected),
        (ServerConnectOutcome::NotAWebdavServer, ConnectOutcome::Rejected),
        (ServerConnectOutcome::InvalidUrl, ConnectOutcome::Rejected),
        (ServerConnectOutcome::AccessDenied, ConnectOutcome::Rejected),
        (ServerConnectOutcome::BucketListRefused, ConnectOutcome::Rejected),
        (ServerConnectOutcome::BucketNotFound, ConnectOutcome::Rejected),
        (
            ServerConnectOutcome::RegionMismatch { region: None },
            ConnectOutcome::Rejected,
        ),
        (ServerConnectOutcome::ClockSkewed, ConnectOutcome::Rejected),
        (ServerConnectOutcome::NotAnS3Endpoint, ConnectOutcome::Rejected),
        (ServerConnectOutcome::ExportNotFound, ConnectOutcome::Rejected),
        (ServerConnectOutcome::ExportRefused, ConnectOutcome::Rejected),
        (ServerConnectOutcome::ReservedPortRequired, ConnectOutcome::Rejected),
        (ServerConnectOutcome::NoNfsV3, ConnectOutcome::Rejected),
        (ServerConnectOutcome::StartFolderOutsideRoot, ConnectOutcome::Rejected),
        (ServerConnectOutcome::TimedOut, ConnectOutcome::Unreachable),
        (ServerConnectOutcome::SignInHelperStuck, ConnectOutcome::Unreachable),
        (ServerConnectOutcome::Unreachable, ConnectOutcome::Unreachable),
        (ServerConnectOutcome::Cancelled, ConnectOutcome::Cancelled),
    ];
    for (outcome, expected) in cases {
        let (got, reason) = classify(&outcome);
        assert_eq!(got, expected, "{outcome:?}");
        let wire = serde_json::to_value(&outcome).expect("serializes");
        assert_eq!(
            Some(reason),
            wire["outcome"].as_str(),
            "the reason is the outcome's own wire name"
        );
    }
}

#[test]
fn the_saved_place_refusals_map_too() {
    let already = answer_dial(
        None,
        Err(SavedPlaceRefusal::AlreadyConnected {
            volume_id: "ts".to_string(),
        }),
        |_| Some("/Volumes/naspi".to_string()),
    );
    assert_eq!(already.outcome, ConnectOutcome::AlreadyConnected);
    assert_eq!(
        already.location.map(|location| location.path).as_deref(),
        Some("/Volumes/naspi")
    );
    let gone = answer_dial(
        None,
        Err(SavedPlaceRefusal::NoSuchServer {
            volume_id: "ts".to_string(),
        }),
        |_| None,
    );
    assert_eq!(gone.outcome, ConnectOutcome::NotSaved);
}
