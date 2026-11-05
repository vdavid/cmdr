//! The native half of `rt::task::JoinSet`'s blocking surface (tokio's own): what
//! an engine leans on, pinned so the browser half (`rt/wasm.rs`, checked in
//! `cmdr-listing-wasm`'s `rt_self_test.rs`) has the same contract to match.

use super::Runtime;
use super::task::JoinSet;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn blocking_work_joins_through_join_next() {
    let runtime = Runtime::current();
    let mut set = JoinSet::new();
    set.spawn_blocking_on(|| 2, &runtime);
    set.spawn_blocking_on(|| 3, &runtime);
    assert_eq!(set.len(), 2);
    let mut joined = Vec::new();
    while let Some(value) = set.join_next().await {
        joined.push(value.expect("blocking work joins"));
    }
    joined.sort_unstable();
    assert_eq!(joined, [2, 3]);
    assert!(set.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn blocking_work_joins_under_the_id_it_spawned_with() {
    let runtime = Runtime::current();
    let mut set = JoinSet::new();
    let handle = set.spawn_blocking_on(|| "done", &runtime);
    let (id, value) = set.join_next_with_id().await.expect("one task").expect("it finished");
    assert_eq!((id, value), (handle.id(), "done"));
    assert!(set.join_next().await.is_none());
}
