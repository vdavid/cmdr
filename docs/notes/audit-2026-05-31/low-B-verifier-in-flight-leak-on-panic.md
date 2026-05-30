# Verifier `in_flight` set leaks a slot on a panicking verification task

**Severity:** low
**Lens:** B — Concurrency
**Confidence:** medium

## Location
`apps/desktop/src-tauri/src/indexing/verifier.rs:73-88`

## What
`maybe_verify` inserts `dir_path` into the `in_flight` set, then spawns a task whose body must run to completion to call `state.in_flight.remove(&dir_path)`. There's no `catch_unwind` or drop-guard. If `verify_and_correct` (or `emit_dir_updated`) panics, the path is never removed, so it permanently counts against `MAX_CONCURRENT_VERIFICATIONS` (2) and the per-path dedup.

## Why it matters
Two panics (e.g. a poisoned-lock unwrap propagating, or a panic deep in the disk-diff path on a pathological entry) would permanently exhaust the verification concurrency budget, silently disabling per-navigation index self-healing for the rest of the session. No crash, no log beyond the panic — sizes just quietly stop reconciling on navigation until the app restarts.

## Evidence
```rust
// verifier.rs:73
state.in_flight.insert(dir_path.clone());
drop(state);

tauri::async_runtime::spawn(async move {
    let affected_paths = verify_and_correct(&dir_path, &writer).await;   // panic here...
    if !affected_paths.is_empty() {
        reconciler::emit_dir_updated(&app, affected_paths);
    }
    if let Ok(mut state) = VERIFIER_STATE.lock() {
        state.in_flight.remove(&dir_path);   // ...never reached on unwind
        state.recent.push((dir_path, Instant::now()));
    }
});
```

## Suggested fix
Move the `in_flight.remove` into a small RAII guard whose `Drop` runs regardless of panic (mirror the existing `WriteSettledGuard` pattern in `write_operations/`), constructed right after the insert. Then a panic in the body still frees the slot during stack unwinding.

## Notes
- Lower severity because a panic here is itself unlikely (the function is mostly `?`-free and match-based), and the index is a disposable cache a restart heals — but the failure mode is silent and session-long, which is why a guard is worth it.
- The codebase already uses exactly this guard idiom for `write-settled`; applying it here is consistent.
