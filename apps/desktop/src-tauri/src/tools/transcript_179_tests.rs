//! The volume facts of prod conversation 179, replayed: the acceptance test for
//! volume-aware tools (`DETAILS.md` § The location contract).
//!
//! One NAS, two ids. The Tailscale share is saved and not connected, last root
//! `/Volumes/naspi`, with a drive index on disk; the LAN share is live at
//! `/Volumes/naspi-1` with no drive index. Both rows record one server GUID, each
//! by a signed-in session (David connected both), one share name, and one
//! username. Only the Tailscale share's importance store scored `invoices paid`.
//! The model guessed `/Volumes/naspi/papers`, a dead mount point, and read
//! `root`'s fresh, empty, "authoritative" answer three times.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::volume::{ConnectionState, Volume};
use cmdr_index::importance::PathClass;
use cmdr_local::LocalPosixVolume;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::location::miss::LocationMissResult;
use crate::location::resolve::{IndexSources, LocationInput, VolumeFact, VolumeFacts, resolve};
use crate::location::share_identity::ShareIdentity;
use crate::network::server_guid::GuidProof;
use crate::test_support::TestDir;
use crate::tools::read::importance::important_folders_on;
use crate::tools::read::listing::{GatePolicy, LIVE_READ, ListOptions, LiveDeps, answer_read};
use crate::tools::resources::importance::tests::{scored_signals, write_db};
use crate::tools::resources::volumes::VolumeKind;

pub(super) const TAILSCALE: &str = "smb-100-127-48-122-445-naspi-david";
pub(super) const LAN: &str = "smb-192-168-1-111-445-naspi-david";
/// The NAS's server GUID, as both share rows record it.
const NASPOLYA: &str = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
/// When the Tailscale share's on-disk index last finished a walk.
const TAILSCALE_AS_OF: u64 = 1_761_000_000;
const NOW: u64 = 1_761_500_000;
pub(super) const INVOICES: &str = "papers/Rymdskottkärra AB/invoices paid";

pub(super) fn conversation_179() -> VolumeFacts {
    conversation_179_with("/Volumes/naspi-1", ConnectionState::Direct)
}

/// The facts with the LAN share at `lan_root` (a temp dir standing in for
/// `/Volumes/naspi-1`, so a live read has a real folder to list) in `lan_state`.
pub(super) fn conversation_179_with(lan_root: &str, lan_state: ConnectionState) -> VolumeFacts {
    let share = |id: &str, server: &str, root: &str, state: ConnectionState, stored: bool| VolumeFact {
        id: id.to_string(),
        name: format!("naspi on {server}"),
        kind: VolumeKind::Smb,
        root: root.to_string(),
        live: state != ConnectionState::Saved,
        connection_state: Some(state),
        earlier_roots: Vec::new(),
        index: IndexSources {
            stored,
            stored_as_of: stored.then_some(TAILSCALE_AS_OF),
            ..IndexSources::none(id)
        },
        share: Some(ShareIdentity::new(
            Some((NASPOLYA, GuidProof::Authenticated)),
            "naspi",
            Some("david"),
        )),
    };
    VolumeFacts {
        volumes: vec![
            VolumeFact {
                id: "root".to_string(),
                name: "Macintosh HD".to_string(),
                kind: VolumeKind::Local,
                root: "/".to_string(),
                live: true,
                connection_state: None,
                earlier_roots: Vec::new(),
                index: IndexSources {
                    live: true,
                    stored: true,
                    ..IndexSources::none("root")
                },
                share: None,
            },
            share(
                TAILSCALE,
                "100.127.48.122",
                "/Volumes/naspi",
                ConnectionState::Saved,
                true,
            ),
            share(LAN, "naspolya", lan_root, lan_state, false),
        ],
        mount_areas: vec!["/Volumes".to_string()],
    }
}

/// The Tailscale share's drive index as prod holds it: rooted at `/Volumes/naspi`,
/// its last walk having listed `invoices paid` and its two children.
fn tailscale_index_on_disk(data: &Path) {
    use cmdr_index::store::{IndexStore, ROOT_ID};

    let db_path = data.join(format!("index-{TAILSCALE}.db"));
    let _store = IndexStore::open(&db_path).expect("open store");
    let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
    IndexStore::update_meta(&conn, "volume_path", "/Volumes/naspi").expect("meta");
    let dir = |parent, name| {
        IndexStore::insert_entry_v2(&conn, parent, name, true, false, None, None, None, None).expect("insert dir")
    };
    let papers = dir(ROOT_ID, "papers");
    let company = dir(papers, "Rymdskottkärra AB");
    let invoices = dir(company, "invoices paid");
    dir(invoices, "2025");
    IndexStore::insert_entry_v2(
        &conn,
        invoices,
        "hetzner-2026-09.pdf",
        false,
        false,
        Some(4),
        Some(4),
        Some(1_759_000_000),
        None,
    )
    .expect("insert file");
    IndexStore::mark_dirs_listed(&conn, &[ROOT_ID, papers, company, invoices], 1).expect("mark listed");
}

/// The LAN mount's `invoices paid`, live: a folder and a file.
fn lan_mount() -> TestDir {
    let mount = TestDir::new("conversation-179-naspi-1");
    let invoices = mount.join(INVOICES);
    std::fs::create_dir_all(invoices.join("2025")).expect("dirs");
    std::fs::write(invoices.join("hetzner-2026-09.pdf"), b"%PDF").expect("file");
    mount
}

/// One `list_dir`, the way the handler answers it, with the LAN mount standing in
/// for the registered volume.
fn list_dir(facts: &VolumeFacts, mount: &Path, volume_id: &str, path: &str) -> Value {
    let input = LocationInput {
        volume_id: Some(volume_id.to_string()),
        path: path.to_string(),
    };
    let resolved = resolve(facts, &input).expect("a named volume resolves");
    let volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "naspi",
        mount,
        crate::volume_host::host(),
    ));
    let volume_for = |id: &str| (id == LAN).then(|| Arc::clone(&volume));
    let cancel = CancellationToken::new();
    let opts = ListOptions::default();
    let read = answer_read(
        facts,
        &resolved,
        &opts,
        LiveDeps {
            cancel: &cancel,
            cfg: LIVE_READ,
            policy: GatePolicy {
                fda_pending: false,
                is_protected: |_| false,
                denial_is_privacy: |_| false,
            },
            volume_for: &volume_for,
        },
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(read).expect("answers")
}

fn child_names(wire: &Value) -> Vec<&str> {
    wire["children"]
        .as_array()
        .expect("children")
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect()
}

/// The transcript's bare guess no longer reads as an empty, fresh, authoritative
/// folder: it's a typed miss whose next call is ready on the LAN share.
#[test]
fn the_bare_guess_under_the_dead_mount_is_a_miss_pointing_at_the_live_share() {
    let input = LocationInput {
        volume_id: None,
        path: "/Volumes/naspi/papers".to_string(),
    };
    let miss = resolve(&conversation_179(), &input).expect_err("nothing live is mounted at /Volumes/naspi");
    let wire: Value = serde_json::to_value(LocationMissResult::from(miss)).expect("serializes");

    assert_eq!(wire["miss"], "noVolumeAtPath");
    assert_eq!(wire["location"]["path"], "/Volumes/naspi/papers");
    assert!(wire.get("authoritative").is_none() && wire.get("children").is_none());
    let nearby = wire["nearby"].as_array().expect("nearby");
    let lan = nearby
        .iter()
        .find(|n| n["volumeId"] == LAN)
        .expect("the live LAN share is nearby");
    assert_eq!(lan["candidate"]["volumeId"], LAN);
    assert_eq!(lan["candidate"]["path"], "/Volumes/naspi-1/papers");
    assert_eq!(nearby[0]["volumeId"], LAN, "the live share leads");
}

/// The whole story. `important_folders` hands the model `invoices paid` on the
/// LAN share, `via` the Tailscale store that scored it, and `list_dir` on that
/// location lists its children from one live read of the share. Asking the
/// Tailscale id instead is answered the same way: the live share beats the offline
/// index, and the model never has to know there were two.
#[test]
fn important_folders_hands_over_the_live_share_and_list_dir_reads_it() {
    let _serialized = cmdr_index::testing::host::test_lock();
    let data = TestDir::new("conversation-179-data");
    let (_index, _installed) = cmdr_index::Index::builder().data_dir(&*data).install_for_test();
    tailscale_index_on_disk(&data);
    let store_path = format!("/{INVOICES}");
    write_db(
        &data,
        TAILSCALE,
        &[(store_path.as_str(), scored_signals(PathClass::UserContent, NOW))],
        NOW,
    );
    write_db(
        &data,
        LAN,
        &[("/photos", scored_signals(PathClass::UserContent, NOW))],
        NOW,
    );
    let mount = lan_mount();
    let lan_root = mount.to_string_lossy().into_owned();
    let facts = conversation_179_with(&lan_root, ConnectionState::Direct);

    let ranked = important_folders_on(&data, &facts, &json!({}), NOW).expect("answers");
    let invoices = ranked["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|f| {
            f["location"]["path"]
                .as_str()
                .is_some_and(|p| p.ends_with("invoices paid"))
        })
        .expect("invoices paid is ranked");
    assert_eq!(
        invoices["location"],
        json!({ "volumeId": LAN, "path": format!("{lan_root}/{INVOICES}") })
    );
    assert_eq!(invoices["via"], json!({ "volumeId": TAILSCALE }));

    let location = &invoices["location"];
    let wire = list_dir(
        &facts,
        &mount,
        location["volumeId"].as_str().expect("id"),
        location["path"].as_str().expect("path"),
    );
    assert_eq!(wire["coverage"]["source"], "live");
    assert_eq!(wire["location"], *location);
    assert!(wire.get("via").is_none(), "asked on the LAN share, answered there");
    assert_eq!(child_names(&wire), ["2025", "hetzner-2026-09.pdf"]);

    let asked_offline = list_dir(&facts, &mount, TAILSCALE, &format!("/Volumes/naspi/{INVOICES}"));
    assert_eq!(
        asked_offline["coverage"]["source"], "live",
        "the live share beats the offline index"
    );
    assert_eq!(asked_offline["location"], *location);
    assert_eq!(asked_offline["via"], json!({ "volumeId": TAILSCALE }));
}

/// With the LAN share disconnected too, the NAS is still answerable: `list_dir`
/// on the Tailscale share's `volumeId` reads `invoices paid` from the index its
/// last walk left, honestly dated, before anyone decides whether a connect is
/// even needed.
#[test]
fn with_the_lan_share_gone_the_tailscale_index_lists_invoices_paid_offline() {
    let _serialized = cmdr_index::testing::host::test_lock();
    let data = TestDir::new("conversation-179-drive-index");
    let (_index, _installed) = cmdr_index::Index::builder().data_dir(&*data).install_for_test();
    tailscale_index_on_disk(&data);
    let facts = conversation_179_with("/Volumes/naspi-1", ConnectionState::Saved);
    let path = format!("/Volumes/naspi/{INVOICES}");

    let wire = list_dir(&facts, &data, TAILSCALE, &path);

    assert_eq!(wire["coverage"]["source"], "offlineIndex");
    assert_eq!(wire["coverage"]["indexStatus"], "stale");
    assert_eq!(wire["coverage"]["authoritative"], false);
    assert_eq!(
        wire["coverage"]["asOf"],
        crate::search::format_timestamp(TAILSCALE_AS_OF)
    );
    assert_eq!(wire["location"], json!({ "volumeId": TAILSCALE, "path": path }));
    assert_eq!(child_names(&wire), ["2025", "hetzner-2026-09.pdf"]);
}

/// Asked on the LAN id while it's disconnected, the offline Tailscale index
/// answers in its place (`via`), rather than a miss.
#[test]
fn asked_on_the_disconnected_lan_share_the_tailscale_index_answers_via() {
    let _serialized = cmdr_index::testing::host::test_lock();
    let data = TestDir::new("conversation-179-via-offline");
    let (_index, _installed) = cmdr_index::Index::builder().data_dir(&*data).install_for_test();
    tailscale_index_on_disk(&data);
    let facts = conversation_179_with("/Volumes/naspi-1", ConnectionState::Saved);

    let wire = list_dir(&facts, &data, LAN, &format!("/Volumes/naspi-1/{INVOICES}"));

    assert_eq!(wire["coverage"]["source"], "offlineIndex");
    assert_eq!(wire["location"]["volumeId"], TAILSCALE);
    assert_eq!(wire["via"], json!({ "volumeId": LAN }));
    assert_eq!(child_names(&wire), ["2025", "hetzner-2026-09.pdf"]);
}

/// Had the Tailscale row's GUID only come from the background probe (whatever
/// answered its saved address), its index could be anyone's: asked on the
/// disconnected LAN share, the read misses with the Tailscale share as a
/// `sameShareAs` hint, and nothing answers in the LAN share's place.
#[test]
fn a_probe_filled_tailscale_guid_never_answers_for_the_lan_share() {
    let _serialized = cmdr_index::testing::host::test_lock();
    let data = TestDir::new("conversation-179-probe-only");
    let (_index, _installed) = cmdr_index::Index::builder().data_dir(&*data).install_for_test();
    tailscale_index_on_disk(&data);
    let mut facts = conversation_179_with("/Volumes/naspi-1", ConnectionState::Saved);
    for v in facts.volumes.iter_mut().filter(|v| v.id == TAILSCALE) {
        v.share = Some(ShareIdentity::new(
            Some((NASPOLYA, GuidProof::Unauthenticated)),
            "naspi",
            Some("david"),
        ));
    }

    let wire = list_dir(&facts, &data, LAN, &format!("/Volumes/naspi-1/{INVOICES}"));

    assert_eq!(wire["miss"], "volumeNotConnected");
    assert_eq!(wire["sameShareAs"], json!([TAILSCALE]));
    assert!(wire.get("children").is_none() && wire.get("via").is_none());
}
