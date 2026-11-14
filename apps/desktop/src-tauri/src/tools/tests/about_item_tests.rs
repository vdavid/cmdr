//! `about_item` through the real gate (`execute_tool`): who may call it, what a
//! turn Cmdr started may touch, and what a protected path lets out.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{BackendKind, Volume, VolumeError};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::agent::types::TurnOrigin;
use crate::file_system::volume::manager::get_volume_manager;
use crate::security_log::{ContentTool, SecurityEvent};
use crate::test_support::TestDir;
use crate::tools::{Caller, ToolCtx, execute_tool};

fn ctx(caller: Caller) -> ToolCtx {
    ToolCtx {
        caller,
        cancel: CancellationToken::new(),
        conversation_id: None,
        asked: None,
    }
}

const ON_A_USER_TURN: Caller = Caller::Agent {
    origin: TurnOrigin::Text,
};
const ON_A_WAKE: Caller = Caller::Agent {
    origin: TurnOrigin::Wake,
};

/// The boot disk, as the app registers it, so the resolver places a scratch path.
fn ensure_root_volume() {
    get_volume_manager().register_if_absent(
        "root",
        Arc::new(cmdr_local::LocalPosixVolume::local_folder(
            "Test root",
            "/",
            crate::volume_host::host(),
        )),
    );
}

async fn about(caller: Caller, params: Value) -> Result<Value, crate::tools::ToolError> {
    let app = tauri::test::mock_app();
    execute_tool(app.handle(), &ctx(caller), "about_item", &params).await
}

fn path_of(path: &Path) -> Value {
    json!({ "path": path.to_string_lossy() })
}

/// The agent on a turn the user started gets the card's facts, with where each
/// comes from.
#[tokio::test]
async fn the_agent_on_a_user_turn_gets_the_facts() {
    ensure_root_volume();
    let dir = TestDir::new("about-item-plain");
    let file = dir.as_ref().join("notes.txt");
    std::fs::write(&file, "hello").expect("write the fixture");

    let answer = about(ON_A_USER_TURN, path_of(&file)).await.expect("it answers");

    assert_eq!(answer["location"]["path"], file.to_string_lossy().as_ref());
    assert_eq!(answer["about"]["target"], "file");
    let identity = &answer["about"]["identity"];
    assert_eq!(identity["state"], "known", "{answer:#}");
    assert_eq!(identity["value"]["name"], "notes.txt");
    assert_eq!(identity["value"]["logicalSize"], 5);
    assert!(identity["value"]["logicalSizeHuman"].is_string());
    assert!(answer["howCmdrKnows"]["identity"].is_string());
    assert!(
        answer.get("stillGathering").is_none(),
        "a local card settles: {answer:#}"
    );
    // Every row the card planned says where it comes from.
    for (row, _) in answer["howCmdrKnows"].as_object().expect("an object") {
        assert!(answer["about"].get(row).is_some(), "{row} has no row: {answer:#}");
    }
}

/// Off direct mode, an outside client can't learn through Cmdr's Full Disk
/// Access where a file came from or when it was opened.
#[tokio::test]
async fn an_outside_client_off_direct_mode_is_refused() {
    let error = about(Caller::External { direct: false }, json!({ "path": "/tmp" }))
        .await
        .expect_err("refused");
    assert_eq!(error.data, Some(json!({ "reason": "directModeOff" })));
}

/// In direct mode it answers, and the read lands in the security log by tool.
#[tokio::test]
async fn a_direct_client_gets_it_and_the_read_is_logged_by_tool() {
    ensure_root_volume();
    let dir = TestDir::new("about-item-direct");
    let file = dir.as_ref().join("notes.txt");
    std::fs::write(&file, "x").expect("write the fixture");
    let capture = crate::security_log::testing::capture_events();

    about(Caller::External { direct: true }, path_of(&file))
        .await
        .expect("direct mode reads it");

    assert_eq!(
        capture.events(),
        vec![SecurityEvent::McpContentRead {
            tool: ContentTool::AboutItem
        }]
    );
}

/// A share Cmdr dials, counting every call made to it.
struct CountingShare {
    root: PathBuf,
    calls: AtomicUsize,
}

impl Volume for CountingShare {
    fn name(&self) -> &str {
        "counting share"
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn backend_kind(&self) -> BackendKind {
        BackendKind::Sftp
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(cmdr_fs::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move { Err(VolumeError::NotFound(path.display().to_string())) })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move { Err(VolumeError::NotFound(path.display().to_string())) })
    }
}

fn counting_share(id: &str) -> Arc<CountingShare> {
    let share = Arc::new(CountingShare {
        root: PathBuf::from(format!("/{id}")),
        calls: AtomicUsize::new(0),
    });
    get_volume_manager().register(id, Arc::clone(&share) as Arc<dyn Volume>);
    share
}

/// A wake asks a network volume nothing (a background `stat` can wake a NAS
/// nobody asked about): its rows say why, and a user's turn does ask it.
#[tokio::test]
async fn a_wake_makes_no_call_to_a_network_volume() {
    let share = counting_share("about-item-wake-share");
    let params = json!({ "path": "/about-item-wake-share/report.pdf", "volumeId": "about-item-wake-share" });

    let answer = about(ON_A_WAKE, params.clone()).await.expect("it answers");

    assert_eq!(share.calls.load(Ordering::SeqCst), 0, "{answer:#}");
    assert_eq!(
        answer["about"]["identity"],
        json!({ "state": "unknown", "value": "refusedInBackground" })
    );
    assert!(
        answer["notes"].as_array().is_some_and(|notes| !notes.is_empty()),
        "{answer:#}"
    );

    about(ON_A_USER_TURN, params).await.expect("it answers");
    assert!(share.calls.load(Ordering::SeqCst) > 0, "a user's turn asks the share");
    get_volume_manager().unregister("about-item-wake-share");
}

/// A protected path leaves name-level facts only, also when the path reaches it
/// through a symlink (the gate follows it, as `inspect_file`'s does).
#[cfg(unix)]
#[tokio::test]
async fn a_protected_path_answers_name_level_facts_even_through_a_link() {
    ensure_root_volume();
    let dir = TestDir::new("about-item-protected");
    let keys = dir.as_ref().join(".ssh");
    std::fs::create_dir(&keys).expect("make the folder");
    std::fs::write(keys.join("id_ed25519"), "x").expect("write the key");
    let link = dir.as_ref().join("keys");
    std::os::unix::fs::symlink(&keys, &link).expect("link to it");

    for path in [keys.join("id_ed25519"), link.join("id_ed25519")] {
        let answer = about(ON_A_USER_TURN, path_of(&path)).await.expect("it answers");
        assert_eq!(
            answer["about"]["contentsStayHome"], "credentials",
            "{path:?}: {answer:#}"
        );
        assert_eq!(answer["about"]["identity"]["value"]["name"], "id_ed25519");
        for (row, fact) in answer["about"].as_object().expect("an object") {
            if ["identity", "owner", "recursiveSize"].contains(&row.as_str()) || !fact.is_object() {
                continue;
            }
            if fact.get("state").is_some() {
                assert_eq!(fact["value"], "protected", "{row} left: {answer:#}");
            }
        }
    }
}

/// A path the resolver can't place answers the location contract's miss.
#[tokio::test]
async fn a_path_on_no_volume_answers_a_typed_miss() {
    let answer = about(
        ON_A_USER_TURN,
        json!({ "path": "/x", "volumeId": "about-item-no-such-volume" }),
    )
    .await
    .expect("a miss is an answer");
    assert_eq!(answer["miss"], "unknownVolume");
}
