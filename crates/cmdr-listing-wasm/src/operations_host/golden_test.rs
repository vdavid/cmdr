//! The volume engine's golden traces on the demo's own host: a real `Demo`
//! (its volume registry, engine events, name order, panes, and operation log),
//! with the fixtures' `src` and `dst` registered beside the page's volume,
//! replaying every scenario in `cmdr_operations::golden` against the fixtures
//! the native engine blesses (`crates/cmdr-operations/tests/golden/`).
//!
//! Natively under `cargo nextest`, and as a wasm test build under Node
//! (`wasm-bindgen-test`, run by `scripts/build-listing-wasm` with `-golden`),
//! where it catches what only the browser runtime does differently: the
//! `JoinSet` ordering, the inline op-log writer, and inline `spawn_blocking`.

use std::sync::Arc;

use cmdr_operations::golden::{Scenario, VOLUME_GOLDENS, VolumeWorld, render};
use serde_json::{Value, json};

use crate::commands::Demo;
use crate::events::EventSink;
use crate::tree::DemoTree;

struct Silent;

impl EventSink for Silent {
    fn emit(&self, _event: &str, _payload: Value) {}
}

/// A fresh demo, its page volume out of the fixtures' way (rooted elsewhere,
/// under another id), and the fixtures' world on its engine.
fn world() -> (Demo, VolumeWorld) {
    let tree: DemoTree = serde_json::from_value(json!({
        "volumeId": "page", "root": "/page", "entries": [],
    }))
    .expect("tree parses");
    let demo = Demo::new(tree, Arc::new(Silent)).expect("valid tree");
    let host = demo.operations();
    let world = VolumeWorld::new(host.engine().clone(), host.log_db().to_path_buf(), |id, volume| {
        host.register_volume(id, volume)
    });
    (demo, world)
}

async fn replay(scenario: Scenario) {
    let (name, fixture) = VOLUME_GOLDENS
        .iter()
        .find(|(s, _, _)| *s == scenario)
        .map(|(_, name, fixture)| (*name, *fixture))
        .expect("a fixture per scenario");
    let (_demo, world) = world();
    let rendered = render(&scenario.run(&world).await);
    assert!(
        rendered == fixture,
        "the {name} trace differs on the demo's host from the native one:\n--- native\n{fixture}\n--- demo\n{rendered}"
    );
}

#[cfg(target_family = "wasm")]
macro_rules! replay_test {
    ($name:ident, $scenario:expr) => {
        #[wasm_bindgen_test::wasm_bindgen_test]
        async fn $name() {
            replay($scenario).await;
        }
    };
}

#[cfg(not(target_family = "wasm"))]
macro_rules! replay_test {
    ($name:ident, $scenario:expr) => {
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn $name() {
            replay($scenario).await;
        }
    };
}

replay_test!(golden_copy, Scenario::Copy);
replay_test!(golden_conflict_skip, Scenario::ConflictSkip);
replay_test!(golden_undo_copy, Scenario::UndoCopy);
replay_test!(golden_move, Scenario::Move);
replay_test!(golden_move_across, Scenario::MoveAcross);
replay_test!(golden_delete, Scenario::Delete);
replay_test!(golden_rename_and_mkdir, Scenario::RenameAndMkdir);
