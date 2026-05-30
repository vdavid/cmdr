# Live write-path lock unwraps bypass the project's `IgnorePoison` convention

**Severity:** low
**Lens:** C — Error handling discipline
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/file_system/write_operations/helpers.rs:625`, `state.rs:417,435,456`, `transfer/volume_conflict.rs:144` (plus `icons.rs:28,33,40,49`, `mcp/dialog_state.rs:40-56`, and several `indexing/` sites).

## What
These are `state.conflict_resolution_tx.lock().unwrap()` (and `ICON_CACHE.read()/write().unwrap()`, etc.) on `Mutex`/`RwLock`s that are simple value stores. The `write_operations` module's own CLAUDE.md lists `crate::ignore_poison: IgnorePoison extension … to not panic on poisoned locks` as a dependency, yet these live sites use raw `.lock().unwrap()`.

## Why it matters
Mostly consistency / defense-in-depth, not a live crash. `Mutex::lock()` only `Err`s on poison (a thread panicked *while holding* the guard); here every critical section is a trivial `*guard = Some(tx)` / `.take()` / map insert-remove with no fallible code under the lock, so poisoning is essentially impossible today and the `.unwrap()` won't fire. But the project deliberately built `IgnorePoison` so an unrelated panic elsewhere can't cascade into a second panic at the next lock site, and these sit on the active copy/move conflict-resolution path. The convention exists precisely so a future edit that *does* run fallible code under one of these guards doesn't silently become a crash vector.

## Evidence
```rust
// helpers.rs:625 (conflict-resolution path, live during every Stop-mode transfer)
*state.conflict_resolution_tx.lock().unwrap() = Some(tx);
// state.rs:456
let tx = state.conflict_resolution_tx.lock().unwrap().take();
```

## Suggested fix
Swap to `.lock_ignore_poison()` / `.read_ignore_poison()` / `.write_ignore_poison()` at these live sites for consistency with the module contract. No behavior change today. (The `CollectorEventSink` `.lock().unwrap()`s in test code are `#[cfg(test)]` — leave them.)

## Notes
Cmdr's panic discipline is otherwise strong: no `panic!`/`unwrap` crash-class bug in live code, all 7 `panic!` and the risky-looking `unwrap`s are in test files or invariant-guaranteed, and the banned `eprintln!`/`println!`/`dbg!` and error-string-matching rules are respected with correct opt-outs.
