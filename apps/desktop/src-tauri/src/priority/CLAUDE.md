# Priority (`src/priority/`)

Who gets the volume, and which of its folders come first. The per-volume signals background work yields to (ONE
transport-generic order: **interactive > transfers > indexing**), plus a ranked walk order a piecewise volume walk could
take. This module owns the SIGNALS and pure decisions; consumers compose them at their own loop boundaries.

## Module map

- `foreground.rs`: is the user waiting on this volume? Per-volume in-flight LEASES (held by the spawned listing
  task) plus last-activity timestamps, app-wide + per volume. Written by the hot listing IPC.
- `transfers.rs`: per-volume gauge of user-initiated write ops (copy/move/delete/trash/drag-out).
- `roots.rs`: which folders matter to this user, ranked, for a walk that takes a volume in pieces. Last session's tabs,
  favorites, this month's working folders, the standard home folders, cloud roots, then `$HOME`. Asked by the index's
  phase machine at each phase boundary, so edits land without a restart.
- `roots/recency.rs`: the Spotlight-recency signal (when to ask, which answers are worth walking early). The query
  itself is `apps/desktop/src-tauri/src/spotlight.rs`, coupled to nothing.
- `host_policy.rs`: `AppHostPolicy` (the index subsystems' `HostPolicy`) and `AppUserActivity` (a storage backend's
  narrower per-volume question), answering from the signals above. Installed once in `setup()`.

## Must-knows

- **Feed the transfer gauge ONLY from `write_operations::state::register_operation_status` /
  `unregister_operation_status`** (the one lifecycle choke point, shared with the eject busy set). A second feed site
  desyncs the count.
- **A missing foreground entry means "never browsed" = idle.** ❌ Don't collapse it to a `0` timestamp — `0` is a real
  clock point, and every background user would stall for the app's first threshold window.
- **The foreground signal is a LEASE plus a timestamp, composed only by
  `cmdr_fs::volume::host::activity::volume_busy_for_user`.** ❌ Reading `idle_for_volume` alone re-opens the bug the
  lease closed. The lease is released by DROP alone (❌ no manual release, ❌ never bind it to `_`) and restamps on the
  way out, which is what starts the debounce.
- **Consumers pick their own scope on purpose** (listed in `foreground.rs` + `DETAILS.md`): enrichment reads APP-WIDE
  foreground + per-volume transfers; scan pacing and transfer-yield read PER-VOLUME. ❌ Don't "unify" them.
- **The index reads these through `AppHostPolicy`, never directly**: it lives in a Tauri-free crate that can't reach
  `crate::priority`. A new index consumer asks `indexing::host::policy` and the adapter answers; ❌ don't hand it a
  `crate::priority` import back. SMB transfer-yield and the write-op feed are app code and still call in directly.
- **Indexing yields must keep forward progress structural**: throttle-to-one or pause-with-resume, ❌ never a gate that
  can stop work (see `scan_pace.rs`'s never-zero budget).
- **The recency sample is armed ONCE per process, off-thread, and answers late.** ❌ Never make an ask wait for it, ❌
  never arm it while the FDA gate is pending.
- **Priority roots are a walk ORDER, never a scope.** Dropping one changes what gets indexed first, never what gets
  indexed — that's what makes ranking on guesses safe. ❌ Don't grow a setting, a promise, or a skip out of them.
- **❌ Never stat a path here while the FDA gate is pending and `tcc_paths::is_potentially_tcc_restricted` covers it.**
  Even `Path::exists()` stacks a system popup on our onboarding modal. Assume it's there, as `volumes::get_favorites`
  does; ❌ no second rule.
- **❌ Nothing in `roots.rs` may touch another volume**, not even a `stat`: a wedged share answers in minutes. Two
  guards run before any filesystem call: the `/Volumes`-style path prefixes and the in-memory `mount_id_for_path`
  lookup. ❌ Never `statfs` (`path_is_on_network_mount`) to decide it.

Design, the full consumer wiring, and decisions: `DETAILS.md`.
