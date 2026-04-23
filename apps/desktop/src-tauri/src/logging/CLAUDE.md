# Logging support module

Lives under `src-tauri/src/logging/`. `tauri-plugin-log` owns the actual log pipeline
(terminal target + rotating file target). This module fills the gaps the plugin doesn't
expose.

## File map

| File       | Purpose                                                                 |
| ---------- | ----------------------------------------------------------------------- |
| `mod.rs`   | `OnceLock<PathBuf>` for the resolved log dir, `AtomicUsize` for keep-count, `eager_prune`, `list_recent_log_files`, `current_total_log_bytes` |
| `tests.rs` | Unit tests: pruner keep-N, zero-keep, missing dir, sorted listing, byte totals |

## What lives here

| Function                          | Role                                                                                   |
| --------------------------------- | -------------------------------------------------------------------------------------- |
| `set_log_dir(path)` / `log_dir()` | Cache the resolved dir once at plugin-build time; Phase 4 bundle builder reads it back |
| `set_keep_count(n)` / `keep_count()` | Live view of the `KeepSome(N)` value the plugin was built with                     |
| `list_recent_log_files(dir)`      | `*.log*` files newest-first by mtime                                                   |
| `current_total_log_bytes(dir)`    | Sum sizes of `*.log*` files (diagnostic)                                               |
| `eager_prune(dir, keep_n)`        | One-shot: delete everything beyond `keep_n` newest                                     |

## Why a one-shot pruner and not a recurring task

`tauri-plugin-log` 2.8.0 exposes `RotationStrategy::KeepSome(usize)` natively. The plugin
rotates and prunes files itself on each rotation. We don't need a 10-min recurring pruner.

The only gap is the "user just lowered the cap at runtime" case: the plugin won't delete
existing archived files until the next rotation, which may not happen for a while. The
`set_max_log_storage_mb` Tauri command calls `eager_prune` so the user sees files disappear
immediately. Purely cosmetic — correctness is already guaranteed by the plugin's rotation.

## Plugin is one-shot

`tauri_plugin_log::Builder` builds the plugin exactly once. There's no runtime reconfigure
API — no "switch rotation strategy", no "add Folder target", no "change max file size".
Changes to the cap that need a different rotation strategy or a target list change (0 ↔
non-zero transitions, raising the cap above the previously baked-in value) take effect on
the next app launch.

This is why `set_keep_count` exists: the in-RAM count drives `eager_prune` calls without
needing to restart the plugin. The next startup reads the setting again and bakes the new
value into the plugin.

## Per-target level filtering (missing feature)

`tauri-plugin-log` does NOT support different log levels per target. Setting the file
target to Debug also forces the terminal target to Debug for modules that don't have an
explicit `RUST_LOG` filter. This is the documented tradeoff in the error-report design —
worth it to get full debug context in error report bundles.

When `advanced.maxLogStorageMb = 0`, the file target is dropped entirely, and terminal
level falls back to the old Info default. The verbose toggle continues to flip terminal
gating via `log::set_max_level` in that case.

## Gotcha/Why

- `list_recent_log_files` returns `Vec<PathBuf>` in "newest-first by mtime" order. If you
  use it to locate the currently-active file, trust mtime, not the filename — rotated
  files have a timestamp suffix but there's no guarantee the suffix is current.
- `eager_prune(dir, 0)` wipes everything including the live file; the plugin re-creates it
  on the next write. This is the correct behavior for the "user just disabled logging at
  runtime" path — we stop capturing immediately rather than waiting for the next restart.
- The log dir resolution in `lib.rs` and in `early_load_max_log_storage_mb` must stay in
  sync. `lib.rs` resolves at plugin-build time with env-var precedence; the early-load
  helper in `settings::loader` mirrors the CMDR_DATA_DIR fallback but uses `dirs::data_dir`
  + bundle id rather than Tauri's `app_data_dir`. If the bundle id changes, both places
  need updating.
