# Priority (`src/priority/`)

Who gets the volume, and which of its folders come first. The per-volume signals background work yields to (ONE
transport-generic order: **interactive > transfers > indexing**, drive indexing AND image enrichment), plus a ranked
walk order a piecewise volume walk could take. This module owns the SIGNALS and pure decisions; consumers compose them
at their own loop boundaries.

## Module map

- `foreground.rs`: last-interactive-activity timestamps, app-wide + per volume. Written by the hot listing IPC.
- `transfers.rs`: per-volume gauge of user-initiated write ops (copy/move/delete/trash/drag-out).
- `roots.rs`: which folders matter to this user, ranked, for a walk that takes a volume in pieces. Last session's tabs,
  favorites, the standard home folders, cloud roots, then `$HOME`. Read by the index's phase machine at each phase
  boundary, so an edited favorites list lands without a restart.
- `host_policy.rs`: `AppHostPolicy` (the index subsystems' `HostPolicy`) and `AppUserActivity` (a storage backend's
  narrower per-volume question), answering from the signals above. Installed once in `setup()`.

## Must-knows

- **Feed the transfer gauge ONLY from `write_operations::state::register_operation_status` /
  `unregister_operation_status`** (the one lifecycle choke point, shared with the eject busy set). A second feed site
  desyncs the count; a missed unregister is already covered by the manager's panic-safe cleanup.
- **A missing foreground entry means "never browsed" = idle.** ❌ Don't collapse it to a `0` timestamp — `0` is a real
  clock point, and every background user would stall for the app's first threshold window.
- **Consumers pick their own scope on purpose** (documented per consumer in `foreground.rs` + `DETAILS.md`): enrichment
  reads APP-WIDE foreground + per-volume transfers; scan pacing and transfer-yield read PER-VOLUME. Don't "unify" the
  scopes.
- **The index reads these through `AppHostPolicy`, never directly.** It's being extracted into a Tauri-free crate, so
  `crate::priority` isn't reachable from it. A new index consumer asks `indexing::host::policy` and the adapter answers;
  ❌ don't hand it a `crate::priority` import back. SMB transfer-yield and the write-op feed still call in directly —
  they're app code.
- **Indexing yields must keep forward progress structural**: throttle-to-one or pause-with-resume, ❌ never a gate that
  can stop work with no wake-up path (see `indexing/network_scanner/scan_pace.rs`'s never-zero budget).
- **Priority roots are a walk ORDER, never a scope.** Dropping one changes what gets indexed first and never what gets
  indexed, which is what makes ranking on guesses safe. ❌ Don't grow a setting, a promise, or a skip out of them.
- **❌ Never stat a path here while the FDA gate is pending and `tcc_paths::is_potentially_tcc_restricted` says a gate
  covers it.** Even `Path::exists()` stacks a system popup on our onboarding modal. Assume it's there, exactly as
  `volumes::get_favorites` does; ❌ don't hand-roll a second rule.
- **❌ Nothing in `roots.rs` may touch another volume**, not even a `stat`: a wedged share answers in minutes and the
  index asks this on its own thread. Two guards, both before any filesystem call: the `/Volumes`-style path prefixes
  (which catch a mount Cmdr never registered) and the in-memory `mount_id_for_path` lookup. ❌ Never `statfs`
  (`path_is_on_network_mount`, a space query) to decide it.

Design, the full consumer wiring, and decisions: `DETAILS.md`.
