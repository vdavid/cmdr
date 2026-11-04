//! A self-test of `cmdr_fs::rt`'s browser half, exported to the smoke test
//! (`smoke.mjs`): the seam's natively-untestable behavior, checked on the real
//! `.wasm` where a trap or a starved event loop actually shows.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use cmdr_fs::rt::{self, Runtime, task::JoinSet, time};
use wasm_bindgen::prelude::*;

/// Runs every check and resolves with a one-line summary, or rejects naming the
/// first one that failed. `now_ms` is the page's `Date.now()`, for the wall clock.
#[wasm_bindgen(js_name = rtSelfTest)]
pub fn rt_self_test(now_ms: f64) -> js_sys::Promise {
    wasm_bindgen_futures::future_to_promise(async move {
        run(now_ms)
            .await
            .map(|summary| JsValue::from_str(&summary))
            .map_err(|e| JsValue::from_str(&e))
    })
}

async fn run(now_ms: f64) -> Result<String, String> {
    let runtime = Runtime::default();

    // A set joins its tasks in the order they finish, each with the id it spawned under.
    let mut set = JoinSet::new();
    let slow = set.spawn_on(sleep_then(30, "slow"), &runtime).id();
    let fast = set.spawn_on(sleep_then(5, "fast"), &runtime).id();
    let mid = set.spawn_on(sleep_then(15, "mid"), &runtime).id();
    let mut order = Vec::new();
    while let Some(joined) = set.join_next_with_id().await {
        let (id, label) = joined.map_err(|e| format!("a set's task failed: {e}"))?;
        order.push((id, label));
    }
    let expected = [(fast, "fast"), (mid, "mid"), (slow, "slow")];
    if order != expected {
        return Err(format!("a set joined {order:?}, want {expected:?}"));
    }

    // `abort_all` stops a task that would never end; it joins as cancelled.
    let mut set = JoinSet::new();
    let forever = set.spawn_on(std::future::pending::<()>(), &runtime).id();
    set.abort_all();
    match set.join_next_with_id().await {
        Some(Err(e)) if e.is_cancelled() && e.id() == forever => {}
        other => return Err(format!("an aborted task joined as {:?}", other.map(|r| r.map(|_| ())))),
    }

    // Dropping a set aborts what's still in it, as tokio's does.
    let ran = Rc::new(Cell::new(false));
    {
        let mut set = JoinSet::new();
        let flag = Rc::clone(&ran);
        set.spawn_on(
            async move {
                time::sleep(Duration::from_millis(5)).await;
                flag.set(true);
            },
            &runtime,
        );
    }
    time::sleep(Duration::from_millis(20)).await;
    if ran.get() {
        return Err("a dropped set's task ran on".into());
    }

    // `yield_now` lets a timer fire: a microtask yield would spin past it forever.
    let fired = Rc::new(Cell::new(false));
    let flag = Rc::clone(&fired);
    drop(runtime.spawn(async move {
        time::sleep(Duration::ZERO).await;
        flag.set(true);
    }));
    let mut yields = 0;
    while !fired.get() {
        if yields == 100 {
            return Err("100 yields and a zero-delay timer never fired".into());
        }
        rt::task::yield_now().await;
        yields += 1;
    }

    // `timeout_at` answers `Elapsed` once its deadline passes.
    let deadline = time::now() + Duration::from_millis(5);
    if time::timeout_at(deadline, std::future::pending::<()>()).await.is_ok() {
        return Err("a timeout_at on a pending future answered".into());
    }

    // `interval_delayed`: the first tick is immediate, the second a period later.
    let mut ticks = time::interval_delayed(Duration::from_millis(10));
    let first = ticks.tick().await;
    let second = ticks.tick().await;
    if second.saturating_duration_since(first) < Duration::from_millis(9) {
        return Err("an interval ticked twice within its period".into());
    }

    // The wall clock reads the page's, re-based onto `std`'s epoch.
    let wall_ms = rt::wall_now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "the wall clock is before 1970".to_string())?
        .as_millis() as f64;
    if (wall_ms - now_ms).abs() > 1_000.0 {
        return Err(format!("wall_now is {wall_ms} ms, the page says {now_ms}"));
    }

    Ok(format!("rt OK ({yields} yield(s) to fire a timer)"))
}

async fn sleep_then(ms: u64, label: &'static str) -> &'static str {
    time::sleep(Duration::from_millis(ms)).await;
    label
}
