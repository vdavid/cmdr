# Panics in non-test backend code: mostly justified, a few worth tightening

**Severity:** low
**Lens:** C — Error handling
**Confidence:** high

## Location

Across `apps/desktop/src-tauri/src/`. After excluding `_test.rs`, `tests.rs`, `bench.rs`, `test_fixtures.rs`, `stress_tests*`, `virtual_device.rs`, ~130 candidate sites remain. Almost all are justified. Per-cluster triage below.

## What

Ran an awk-based scan to enumerate `.unwrap()`, `.expect(...)`, `panic!(...)`, `unreachable!(...)`, `todo!(...)` outside `#[cfg(test)] mod tests { ... }` blocks. Triaged by cluster (top files: `search/ai/mappings/time_mapping.rs`, `file_system/volume/backends/local_posix.rs`, `file_system/git/virtual_listing.rs`, `mcp/executor/*`, `network/manual_servers.rs`, plus singletons). Patterns reviewed but not flagged here: `Mutex::lock().unwrap()` (per audit instructions; poisoning means abort and we accept that), `spawn_blocking(...).await.unwrap()` on the `JoinHandle` (only fires if the closure panicked, which is itself a Rust-panic propagation question, not new risk).

## Why it matters

A file manager that panics mid-operation can corrupt the user's data or leave a write op half-finished. We're shipping to paying users at launch, so even a benign panic shows up as a crash report.

## Evidence

Clusters reviewed (representative, not exhaustive):

| Cluster | Verdict | Notes |
|---|---|---|
| `search/ai/mappings/time_mapping.rs:45-239` (~17 sites) | Safe | All `with_hms(0, 0, 0).expect("valid")` on `time::Date` are infallible (0:0:0 is always a valid time); `from_calendar_date(year, month, 28)` is bounded (every month has a 28th); `unreachable!()` at line 115 covers an exhaustive quarter-month match. |
| `file_system/volume/backends/local_posix.rs:126-698` (~16 sites) | Safe | All `spawn_blocking(...).await.unwrap()` on the `JoinHandle`; only fires if the closure itself panics, which already aborts the task and bubbles to the safety net. Routine `tokio` idiom. |
| `file_system/git/virtual_listing.rs:119-158` (6 sites) | Safe | Every `display_size.as_ref().unwrap()` is on a `Some(...)` value assigned one line above. |
| `mcp/executor/*.rs` (`async_tools`, `search`, `nav`, `app`) | Safe | Each `_ => unreachable!()` follows an exhaustive `match` on an enum variant or `if let` discriminant that constrains the value. |
| `commands/network.rs:432` | Safe | `None => unreachable!()` matches a `Some` value the prior arm just early-returned for. |
| `network/manual_servers.rs:135, 176` | Safe | `colon_count == 1` branch precondition holds for `split_once(':').expect("colon exists")`; `parse_smb_url` is only called when `extract_protocol` already confirmed `://` exists. |
| `file_system/sync_status.rs:144, 150` | Acceptable | `thread::Builder::spawn` and `JoinHandle::join`: thread spawn failure means the process is out of resources; thread panic should never happen in the closure (pure `get_sync_status`). |
| `file_system/open_with.rs:104, 166, 172` | Safe | `EXT_CACHE.lock().expect("...")` is mutex-poisoning panic; acceptable. |
| `file_system/open_with.rs:309-350`, `quick_look/controller.rs:121, 148, 177`, `menu/macos.rs:496, 568`, `lib.rs:178` | Safe | `MainThreadMarker::new().expect(...)` and similar — invariant is documented at call site, every caller already runs on main thread via `run_on_main_thread`. |
| `restricted_paths/mod.rs:76-119` | Safe | `RwLock::{read,write}().expect("poisoned")` — poisoning case. |
| `file_system/git/watcher.rs:62, 114` | Safe | Same — mutex poisoning. |
| `network/mdns_discovery.rs:101` | Safe | `thread::Builder::spawn(...).expect("Failed to spawn mDNS event thread")` — spawn failure means OOM. |
| `file_system/write_operations/helpers.rs:554` | Safe | `unreachable!("conditional conflict resolutions must be reduced before apply_resolution")` — documented invariant, no caller can violate without code change. |
| `redact/mod.rs:190` | Safe | `Regex::new(...).expect("valid redactor regex")` on a static const; would fail at startup, immediately. |
| `mcp/server.rs:591` | Mostly safe | `serde_json::to_value(caps).unwrap()` on an internal `McpCapabilities` struct. Worth a `.unwrap_or(json!({}))` since the JSON is going out the IPC, but the type is small and statically known. |
| `file_system/write_operations/scan_preview.rs:407`, `transfer/copy.rs:294`, `transfer/volume_copy.rs:1013`, `transfer/volume_move.rs:376, 786`, `transfer/eta.rs:157` | Safe | All `.expect("...")` messages encode a documented invariant kept by the surrounding driver loop (e.g. "`async driver always supplies dest_path`"). Each panic would be a bug, but the closest caller is one function away. |
| `commands/file_system/write_ops.rs:193-246`, `commands/file_system/volume_copy.rs:40, 74`, `state.rs:591`, `rename.rs:368, 394`, `ui.rs:99` | Safe | `unwrap_or_default()` on `Option<WriteOperationConfig>` etc — defaults are sane (empty config, empty string for filename). |
| `indexing/writer.rs:1014, 1066` | Worth a look | `inode.unwrap()` is guarded by `inode.is_some()` checks higher in the function but the chain is non-trivial. Not user-facing (background indexing), so a panic crash-reports + restarts indexing on next launch. Low risk. |

## Suggested fix

No code changes required for launch. Two soft cleanups for a future polish pass:

- `mcp/server.rs:591`: swap `serde_json::to_value(caps).unwrap()` for `.unwrap_or_else(|_| json!({}))` so a hypothetical capability struct change can't crash the MCP server.
- `indexing/writer.rs:1014, 1066`: extract the `inode.unwrap()` calls into a single `let Some(inode) = inode else { ... };` early guard so the invariant is local and visible.

## Notes

The codebase is unusually disciplined here: nearly every `.expect("...")` carries a message stating the invariant, and the riskiest panic surfaces (Tauri command boundaries, hot copy paths, watcher callbacks) all use proper `Result` returns or `log::error!`-and-continue. The mutex-poisoning `.expect()` pattern is consistent and the audit instructions accepted it explicitly.

Frontend-side (`apps/desktop/src/`) silent error swallowing checked separately — see notes in summary.
