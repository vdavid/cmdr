# Commands module: details

Per-file function inventory and decision rationale. `CLAUDE.md` holds the must-knows.

## File inventory

- **`mod.rs`**: re-exports. `mtp` / `network` / `volumes` gated behind
  `#[cfg(any(target_os = "macos", target_os = "linux"))]`. There's no `volumes_linux` module: the volume commands are
  cross-platform, and `commands::volumes_linux` is a `#[cfg(target_os = "linux")] pub use volumes as volumes_linux;`
  kept only until `ipc.rs` stops registering the Linux set under that path. See
  `../volumes_linux/DETAILS.md` § "One command module".
- **`util.rs`**: `TimedOut<T>`, `DeadlineError`, `blocking_with_timeout`, `blocking_with_timeout_flag`,
  `blocking_typed_result_with_timeout`, `timeout_detached_typed`, `Deadline` + `timeout_detached_within`, and
  `BlockingBudget`.
- **`file_system/`**: directory module split by operation type. `mod.rs` has `expand_tilde()`, re-exports, tests.
  `listing.rs`: streaming + virtual-scroll listing, path queries, `find_first_fuzzy_match` (type-to-jump),
  benchmarking, `get_brief_column_text_widths` (per-column widest-filename text widths for Brief mode). `refresh_listing`
  takes a `force` flag. Unforced (the post-write top-ups: transfer, rename, mkdir) it short-circuits on fully-covered
  listings (`Volume::listing_watch_coverage(path) == WatchCoverage::EveryWriter`), because the cache is kept fresh by
  `notify_mutation` and a redundant full re-read after every transfer (the FE's `refreshPanesAfterTransfer`) wedges slow
  volumes (MTP 17 s + USB session collision). Forced (⌘R and the MCP `refresh` tool) it always re-reads: `EveryWriter`
  is a claim about the volume's own writes, so on SMB a cached answer to "re-read this" would be a lie. Logs at debug
  `target: "refresh_listing"` on short-circuit.
  `write_ops.rs`: create, copy, move, delete, trash, scan preview, conflict resolution, synthetic diff helpers.
  `volume_copy.rs`: cross-volume copy/move/compress/scan, `SourceItemInput`. The three transfer commands are
  pass-throughs that build the `TauriEventSink` and hand everything to `write_operations::start_volume_{copy, move,
  compress}`, which own the volume + destination-path resolution and the archive forks so a backend caller reaches the
  same routing (`../file_system/write_operations/DETAILS.md` § "Routing a transfer"). `scan_volume_for_conflicts` optionally takes a
  source volume id + source paths and resolves each item's real `is_directory` + size from the source volume via ONE
  batched `scan_for_copy_batch` (O(top-level items), never a subtree walk), overriding the FE's name-only placeholders
  so dir-vs-dir collisions classify as silent merges; back-compatible when omitted. The source paths also drive
  `drop_self_collisions`, which removes the collisions naming a source itself so a same-folder paste doesn't announce
  every item as its own conflict. It answers with the engines' own predicates, which is what keeps the dialog and the
  write agreeing about which clashes are real (canonical:
  `../file_system/write_operations/transfer/DETAILS.md` § "Self-collision (duplicating in place)"). `stat.rs`:
  `stat_paths_kinds(paths) -> TimedOut<Vec<Option<bool>>>`, a batched top-level "is this a directory?" probe for the
  drag-and-drop transfer path (`Some(true)` = dir, `Some(false)` = file, `None` = unknown / non-local / vanished). One
  `spawn_blocking` under the read timeout, never a subtree walk; per-item failures map to `None` so a virtual MTP/SMB
  path on the pasteboard can't poison the batch. The pure `stat_paths_kinds_blocking` helper is reused by
  `clipboard.rs::read_clipboard_files`. `drag.rs`: native drag, self-drag overlay (see "Drag session locality" below).
  `e2e_support.rs`: feature-gated E2E/debug commands. `listing.rs::path_exists` is SMB-aware: a disconnected SMB volume
  returns an immediate `false`, so it re-checks `smb_connection_state()` and reports `timedOut: true` instead, and a
  transient blip can't evict the user from a network folder. `TimedOut<T>`'s TS twin lives in
  `$lib/tauri-commands/ipc-types.ts`; every typed error enum's twin is generated into `$lib/ipc/bindings.ts`.
- **`volumes.rs`** (macOS): `list_volumes`, `get_default_volume_id`, `get_volume_space`, `resolve_path_volume`
  (statfs-based, no volume enumeration), `resolve_location`. The latter two share one `resolve_path_to_volume` body
  (protocol dispatch for `mtp://` / `smb://` plus the local `statfs` branch), so a virtual path resolves the same way
  for both; `resolve_path_volume` returns the `VolumeInfo`, `resolve_location` maps it to a `Location` (`volume_id` +
  the input path). `resolve_location` is the canonical path→volume resolver for navigation edges: the `Location` type
  lives in `crate::location` (shared across all three platform backends) and is the specta-export vehicle that lands
  `Location` + `ResolveLocationResult` in `bindings.ts`. The frontend wraps it as `resolveLocation`
  (`$lib/tauri-commands/storage.ts`, with the outer FE timeout layer) and
  `lib/file-explorer/navigation/resolve-location.ts` maps it to a typed `{ ok }` outcome. Calling
  `resolve_path_volume_fast` alone would return `None` for `smb://` / `mtp://` paths, so don't bypass the shared body.
- **`volumes_linux.rs`** (Linux): same interface as `volumes.rs` (including `resolve_location`), delegates to the
  `volumes_linux` module.
- **`mtp.rs`**: full MTP command surface (connect, disconnect, list, download, upload, delete, rename, move, scan).
- **`sftp.rs`**: the SFTP surface and the wire vocabulary it speaks — `connect_sftp_volume` (a tagged
  `SftpConnectResult`, never a string), `cancel_sftp_connect`, `disconnect_sftp_volume`, `approve_sftp_host_key` / `forget_sftp_host_key` /
  `list_trusted_sftp_host_keys`, the credential trio (`save` / `has` / `delete`, keyed `host:port` + username, each on a
  blocking task because the Keychain can prompt), and the known-servers trio (`get` / `update` / `forget`). ❗ There is
  deliberately no command that returns a stored secret. The flow behind the commands is
  `network::sftp_volume_wiring`; the frontend contract is `crates/cmdr-sftp/DETAILS.md` § "Connecting from the
  frontend".
  - ❗ **Reconnecting an SFTP volume, and asking what a sign-in would want, both go through `network.rs`**: the two
    `reconnect_smb_*` commands and `get_volume_sign_in_state`. All three are backend-neutral (they delegate to a
    `Volume` trait method on whatever is registered); renaming the two `smb`-prefixed ones is a cross-backend follow-up
    rather than something SFTP does on its own.
  - ❗ **`connect_sftp_volume`'s result carries `rung` and ❌ nothing about a later sign-in.** The rung is a fact about
    that dial; what a sign-in would ask for is decided per dial too, so it is a query, not a payload.
  - ❗ **`connect_sftp_volume`'s `attempt_id` is the CALLER's, made before the call**, and `cancel_sftp_connect` takes
    the same one. The command doesn't answer for up to 30 s, so an id it returned would be useless for arming a cancel
    button. The table behind it: `network/DETAILS.md` § "The attempt table, and why the id is the caller's".
- **`network.rs`**: SMB/network shares: discovery, share listing, keychain, mounting, direct-connection upgrade,
  in-place reconnect (`reconnect_smb_volume`: backend single-flighted via `Volume::attempt_reconnect`;
  `reconnect_smb_volume_with_credentials`: the "Sign in" path after an auth-failure reconnect give-up, via
  `Volume::reconnect_with_credentials`), what a sign-in would ask for (`get_volume_sign_in_state`, via
  `Volume::sign_in_prompt` — read live when a banner renders, ❌ never carried on a connect result; an unregistered id
  and a backend with no story of its own both answer `password`, the safe way to be wrong), per-volume disconnect (`disconnect_smb_volume`: macOS shells out to
  `diskutil unmount`, Linux drops the smb2 session). Borrow Finder's saved password (macOS):
  `system_has_saved_smb_password` (prompt-free probe driving the "Use saved password" offer) and
  `upgrade_to_smb_volume_using_saved_password` (consent-gated read via `secrets::system_keychain_smb` → direct smb2 →
  copies the password into Cmdr's own store so future reconnects are silent → `CredentialsNeeded` fallback if
  absent/denied). User-initiated only. Lazy-startup hooks: `ensure_network_discovery_started` (idempotent: kicks off
  mDNS + manual-server load + smb-mount upgrade on first user network action) and `set_network_enabled` (live-applies
  the `network.enabled` toggle). Upgrade business logic lives in `network::smb_upgrade`; commands here are thin wrappers.
  `list_shares_with_credentials` carries `#[allow(clippy::too_many_arguments)]`: Tauri params must be top-level args.
- **`smb_diagnostics.rs`** (debug window only): `list_smb_volumes` (the dashboard's volume picker) and
  `get_smb_diagnostics(volume_id)` (a snapshot of one volume's `smb2::SmbClient`). The snapshot DTOs mirror
  `smb2::Diagnostics` & friends with `specta::Type` derives (so `smb2` needn't depend on specta), one `impl From` per
  type.
- **`memory_diagnostics.rs`** (macOS only): `get_memory_diagnostics(sizes_per_tag)` — one payload answering "what is
  Cmdr holding right now, and what shape is it in?". Folds `cmdr_fs::process_memory`'s four readers together: the
  footprint, mimalloc's own accounting, the registered malloc zones, and the kernel's VM map by tag with a per-tag
  region-size histogram. That last field is why it exists: a repeated exact region size is a fingerprint of whatever
  asked for those bytes, and it is what produced the first real candidate for a 643 MB block three investigations had
  left anonymous (`../../../../../docs/notes/idle-malloc-large-clip-towers-2026-08-21.md`). `sqlitePageCache` adds the
  fifth accountant, `cmdr_fs::sqlite_util::query_page_cache_usage` plus `live_read_connections`: SQLite's page slab is a
  leaked Rust allocation, so it's a fixed 64 MiB inside the mimalloc total that no other field names, and the whole
  point of one payload is that nobody has to know to go ask SQLite separately. Deliberately NOT
  `debug_assertions`-gated:
  the readings that matter come from a shipped build under a real workload, which is the one condition a debug-only
  command can't reach. Carries no paths or names, only counts. Runs off the IPC thread (one syscall per map entry) with
  a 5 s backstop.
- **`eject.rs`**: `eject_volume(volume_id)` + `get_busy_volume_ids()`, thin delegates. The teardown logic (kind
  dispatch, the pure unit-tested `decide_eject_action`, the busy-volume guard, and the `diskutil`/`umount`/MTP
  shell-out) lives in `file_system::volume::eject`. `EjectError` IS the wire type, so the command returns it unchanged
  and the frontend words each variant from `errors.eject.*`. `get_busy_volume_ids()` bootstraps the picker's busy set (see
  `write_operations/DETAILS.md` § "Busy-volumes set").
- **`favorites.rs`**: `add_favorite`, `remove_favorite`, `rename_favorite`, `reorder_favorites`. Thin pass-throughs over
  `crate::favorites::store`; each persists `favorites.json` (5s write timeout) then re-emits `volumes-changed`. No
  `list_favorites` (listing rides `list_volumes` / `volumes-changed`). See `favorites/CLAUDE.md`.
- **`font_metrics.rs`**: `store_font_metrics`, `has_font_metrics`.
- **`logging.rs`**: `batch_fe_logs` (forwards batched frontend log entries into the fern logger) and `set_log_level`.
- **`icons.rs`**: `get_icons`, `get_custom_folder_icon_ids` (visible-range custom-folder detection),
  `refresh_directory_icons`, cache clear.
- **`rename.rs`**: `move_to_trash` (delegates to `write_operations::trash::move_to_trash_sync`),
  `check_rename_permission`, `check_rename_validity`, `rename_file`. `rename_file` calls `notify_mutation` after success
  to update the listing cache (both local and volume-aware paths).
- **`volume_id` on the write commands.** `create_directory` / `create_file` / `rename_file` only expand tilde (root),
  resolve the `volume_id`, and apply the 5 s write timeout, shipping the typed `MutationError` unchanged; the logic and the managed instant op live
  in `file_system::write_operations::{create,rename}`. For a non-root `volume_id`, `delete_files` uses the volume-aware
  delete and skips local `validate_sources` (MTP virtual paths fail `symlink_metadata`), and `rename_file` passes the id
  through and skips permission checks. The local rename notifies the listing cache via `notify_rename_in_listing`, the
  volume one via its own `notify_mutation`.
- **`restricted_paths.rs`**: `get_restricted_paths`: read-only snapshot for the frontend store bootstrap. See
  `crate::restricted_paths` for the state machine and the `restricted-paths-changed` event payload.
- **`file_viewer.rs`**: session lifecycle, regex/literal search with mode flags, word wrap, menu state, encoding pickers
  (`viewer_set_encoding` / `viewer_get_encoding_options`), tail mode (`viewer_set_tail_mode`), `viewer_reload`.
- **`menu.rs`**: native menus and menu-bar state — the context menus (file / breadcrumb / volume row / parent row /
  tab / network host), the view-mode + hidden-files + pin-tab + reopen-tab sync commands, and `activate_window_menu`
  (per-window focus-gain: swaps the macOS app menu bar between main/viewer, then enables/disables file-scoped items via
  the private `set_menu_context` helper; see `menu/DETAILS.md`).
- **`quick_look.rs`**: `quick_look_open` / `quick_look_set_path` / `quick_look_close` (native `QLPreviewPanel`
  singleton on macOS, no-op stubs elsewhere; 2 s main-thread-hop timeout). See `crate::quick_look`.
- **`window_ordering.rs`**: `show_main_window` / `order_window_to_back`, E2E-only window z-ordering (order to back
  without focus). No-op off macOS / outside E2E.
- **`file_actions.rs`**: direct file actions from the palette / menus — `show_in_finder`, `get_info`, `open_in_editor`,
  `copy_to_clipboard`, and `cloud_make_available_offline` / `cloud_remove_download` (iCloud Drive download/eviction via
  `FileManager` ubiquity APIs; see `file_system/cloud_actions.rs`).
- **`child_window_state.rs`**: `get_child_window_rect` / `set_child_window_rect(label, rect)` cache per-label
  child-window geometry via `State<ChildWindowRectStore>`. In-memory and session-only, never on disk; used by Settings
  and Debug. Viewers don't use it (they cascade, see `lib/window-positioning.ts`). Only the main window persists across
  launches, via `window_state/`.
- **`settings.rs`**: port availability check, watcher debounce, menu accelerator updates, live-apply setters for
  `network.directSmbConnection`, `advanced.showSafeSaveFiles`, `advanced.showStagingTempFiles`,
  `network.smbConcurrency`, and the restricted-window
  pair `get_restricted_window_settings` / `persist_restricted_window_setting` (the viewer's typed settings surface; see
  `capabilities/CLAUDE.md` § viewer).
- **`mcp.rs`**: `set_mcp_enabled`, `set_mcp_port` (live start/stop/port-change without app restart), `get_mcp_token`
  (returns the per-instance bearer token for in-process / E2E callers; see `mcp/DETAILS.md` § Authentication).
- **`licensing.rs`**: status query, activation, expiry, reminder, key validation.
- **`whats_new.rs`**: `get_whats_new(since_version, max)` (release entries for the What's New dialog) and
  `whats_new_dev_override` (dev-only).
- **`indexing.rs`**: `start_drive_index`, `stop_drive_index`, `get_index_status`, `get_dir_stats`,
  `get_dir_stats_batch`, `clear_drive_index`, `set_indexing_enabled`, `get_index_debug_status` (dev-only). Uses
  `State<IndexManagerState>`. Two of these carry the MASTER drive-indexing switch (the model lives in
  `indexing/lifecycle/DETAILS.md` § The two indexing switches): `set_indexing_enabled` moves the gate first, then stops
  every volume or resumes only the drives whose per-drive intent says yes, and `enable_drive_index` refuses once,
  transport-neutrally, with `EnableIndexingOutcome::IndexingDisabled` so the FE has one shape to match. The other
  non-`Started` arms the FE must answer are the two deferrals: something else holds the drive (`DeferredUntilSearchEnds`
  a search walking it, `DeferredUntilScanEnds` a full walk already running), so the index remembers the request and runs
  it when that holder ends (`indexing/lifecycle/DETAILS.md` § The one walk a volume remembers) — a promise the UI has to
  voice, since nothing else marks the wait, and they stay two variants because the user's next question differs.
- **`media_index/`**: the media-index IPC surface, one module per family — `search.rs` (OCR, tag, semantic,
  find-similar, dedup), `state.rs` (per-volume state + covered-count preview), `reclaim.rs` (preview + prune),
  `file_status.rs` (per-file overlay + per-folder badge), `clip_model.rs` (install state, download, delete),
  `thumbnail.rs` (grid tokens), and `policy.rs` (the coverage-CHANGING setters). `mod.rs` keeps the hit-limit clamp and
  the ONE enabled-volume rule, and glob-re-exports the rest so `generate_handler!` can resolve each command's hidden
  `__cmd__*` macros through the same path. The subsystem itself is reached through `media_index::read` / `gate` /
  `network::config`. Behavior rationale: `media_index/DETAILS.md` § "The IPC surface".
- **`importance.rs`**: `record_visit(Location)`, the fire-and-forget navigation-visit feeder. Gated on the volume's
  typed kind, failure-silent by contract. Rationale: `importance/DETAILS.md` § "The visit signal".
- **`clipboard.rs`**: `copy_files_to_clipboard`, `cut_files_to_clipboard`, `copy_paths_to_clipboard` /
  `cut_paths_to_clipboard` (paths-by-value siblings for the search-results pane, which has no backend listing),
  `read_clipboard_files`, `clear_clipboard_cut_state`. macOS uses NSPasteboard via `clipboard::pasteboard`; non-macOS
  stubs return errors. `read_clipboard_files` returns `ClipboardReadResult { paths, is_cut, is_directory }` where
  `is_directory` is an index-aligned `Vec<Option<bool>>` from a batched off-main-thread `stat_paths_kinds_blocking`, so
  the paste toast can split files vs. folders without walking trees.
- **`crash_reporter.rs`**: `check_pending_crash_report`, `dismiss_crash_report`, `send_crash_report`. Send skipped in
  dev/CI.
- **`beta_signup.rs`**: `beta_signup(email)` POSTs ONLY the email (never an install id) to `POST /beta-signup`. Returns a
  typed `BetaSignupResult` (`subscribed`/`invalidEmail`/`softFailure`). Network, not filesystem, so no
  `blocking_with_timeout` (the `reqwest` client carries its own 10 s timeout).
- **`error_reporter.rs`** (Flow A): `prepare_error_report_preview`, `send_error_report`. Two-step so the preview dialog
  is deterministic without shipping the full bundle through IPC twice. Upload skipped in dev/CI.
- **`analytics.rs`**: `track_event(name, props_json)`, a thin pass-through to `posthog::capture` for the open set of
  frontend feature events. No capability entry; the PII-free prop contract lives in `analytics/CLAUDE.md`.
- **`feedback.rs`**: `send_feedback(feedback_text, email?)` POSTs to `/feedback` via `crate::feedback`, returning a
  typed `SendFeedbackResult` (`Invalid` on a bad email, etc.). Network, not filesystem, so no `blocking_with_timeout`
  (the `reqwest` client carries its own 10 s timeout).
- **`search.rs`**: thin IPC wrappers over the `search` module. `resolve_ai_backend` for AI provider config. Post-filters
  directory sizes after `fill_directory_sizes`.
- **`selection.rs`**: Selection-dialog backend (parallel to `search.rs`), thin wrappers over `crate::selection`:
  `translate_selection_query` (AI translation via `crate::ai` + `crate::selection::ai`) plus the recent-selections
  history (`get_recent_selections`, `add_recent_selection`, `remove_recent_selection`, `clear_recent_selections`,
  `apply_recent_selections_max_count`).
- **`go_to_path.rs`**: the "Go to path" quick-nav surface: `resolve_go_to_path(input, base_dir)` plus recent-paths
  history (`get_recent_paths`, `add_recent_path`, `remove_recent_path`, `clear_recent_paths`).
- **`sync_status.rs`**: `get_sync_status`: macOS delegates to `file_system::sync_status`; non-macOS returns an empty map
  via `#[cfg]` on the function itself (not the module).
- **`e2e.rs`**: E2E/test-support hooks, always compiled in (reading an unset env var is a no-op in production):
  `get_e2e_start_path`, `is_e2e_mode`, `ask_cmdr_fake_active`, `is_force_onboarding`, `set_test_throttle`,
  `set_test_scan_preview_delay`, `flush_file_watcher`, `force_agent_wake` (stages one folder's activity on the wake
  loop's real channel and makes it act now, on that folder alone; it skips the timer and the proactive toggle, never a
  gate, and its `quiet` flag picks which script the wake's fake assistant plays), `stage_agent_rollup` (the same
  staging without the wake, so a spec can prove the force reports on its own folder) — both in
  `agent/wake/DETAILS.md` § Forcing a wake.

## Decisions

`agent/` is the one command domain split into a directory, because it carries five unrelated command families plus the
wire DTOs they share: `views.rs` (the stream event enum, the specta display projections, and the pure mappings),
`chat.rs` (send + cancel), `attachments.rs`, `bulk_rename.rs`, `conversations.rs`, `consent.rs`, `cost.rs`,
`wake.rs` (the live-apply push for the proactive loop's three settings). `mod.rs`
glob-re-exports each submodule, which is load-bearing: `#[tauri::command]` generates companion items next to the
function, and the `ipc.rs` manifest registers by the `crate::commands::agent::<name>` path, so a NAMED re-export
would leave those hidden items behind and fail to compile. `mod.rs` also owns the two shared `main.db` connection
helpers.

`chat.rs` adapts Ask Cmdr's channel-only stream events. Its `ProposalReady` snapshot is display-only; rename review
commands accept only opaque proposal and row ids. `apply_bulk_rename` consumes an exact accepted preflight then delegates
the batch to `write_operations::start_bulk_rename`; it never accepts frontend paths or model approval.

**One commands file per domain, no business logic in commands.** Tauri command functions are the IPC boundary
(deserialization, state extraction, error mapping). Mixing business logic here makes it untestable (Tauri commands need
a running app to invoke); thin pass-throughs keep the real logic in independently unit-testable subsystem modules.

**Platform gating at the module level in `mod.rs`, not inside functions.** Entire command surfaces (MTP, network,
volumes) are platform-specific. Module-level gating makes the compiler exclude unused code entirely rather than compile
stub functions, and prevents calling an unsupported command (the Tauri command isn't registered at all).

**`blocking_with_timeout` for ALL filesystem-touching commands, not just read-only ones.** `spawn_blocking` alone
doesn't protect against hung NFS/SMB mounts where even `path.exists()` can block indefinitely. The timeout wrapper
returns a fallback (or error) instead of freezing the IPC thread or exhausting the blocking pool. Commands that already
use `spawn_blocking` wrap it with `tokio::time::timeout` instead.

**Timeout-aware return types.** A plain fallback is indistinguishable from a real empty/none result ("no volumes
mounted" vs "timed out before listing volumes"). `TimedOut<T>` (`{ data, timedOut }`) carries the distinction for
non-`Result` returns; the bare `blocking_with_timeout` stays for the rare read where it genuinely doesn't matter. A
`Result` return carries it as a VARIANT of the command family's own error enum (`MutationError::TimedOut`,
`EjectError::TimedOut`, `DeadlineError::TimedOut`), which is why `timeout_detached_typed` takes an `on_timeout` that
mints the caller's type.

**Every command's `Err` is a typed enum, and there is deliberately no shared one.** A generic
`IpcError { message, timed_out }` with a `from_err` constructor used to sit in `util.rs`. Being ergonomic, it spread to
39 call sites and stringified whatever typed error reached it, so `EjectError::Busy` (a proper enum with fields and doc
comments) arrived on the frontend as an English sentence that a translated toast then interpolated verbatim. The rule
that replaced it: reuse the vocabulary the command belongs to (`MutationError` for a mutation, `ViewerError` for the
viewer, `VolumeError` nested inside either), or add a small enum beside the family. `DeadlineError` is the ONE shared
type, and only for commands whose wrapped work genuinely cannot refuse (the favorites writes, `resolve_go_to_path`),
where "the deadline passed" and "the task panicked" exhaust the failure modes. The frontend renders every variant from
the message catalog: `docs/guides/error-handling.md`.

**JSON for all Tauri IPC, not binary (MessagePack/Protobuf).** Benchmarked with real directory listings: MessagePack is
34-58% SLOWER than JSON despite being 17-19% smaller. Tauri serializes `Vec<u8>` as a JSON array of numbers, so binary
data gets wrapped in JSON anyway, negating size benefits and adding decode overhead. See
[benchmark data](../../../../../docs/notes/json-ipc-benchmarks.md).

**The index subsystems' commands DO live here, unlike `ai` and `space_poller`.** `indexing/`, `media_index/`, and
`importance/` are being extracted into a Tauri-free crate, so a `#[tauri::command]` inside them is a back-edge by
construction. That reverses the co-location argument below for those three only: the commands sit here, and each one
stays thin over the subsystem's own read/gate/config entry points.

**No `ai` or `space_poller` module under `commands/`.** Both subsystems have their own complex lifecycle (model
loading / suggestion pipelines / secret-store keys; poller init/start/watch). A thin wrapper would be pure boilerplate
forwarding, so they register directly from their own modules, keeping the command surface co-located with the
frequently-changing implementation. Space-poller commands: `watch_volume_space`, `unwatch_volume_space`,
`set_disk_space_threshold`.

## Drag session locality

`start_selection_drag` and `start_drag_paths` both run on the main thread (`run_drag_on_main_thread`) and pick a
`DragSessionLocality` through `locality_for_volume`, keyed on `Volume::paths_are_os_visible()`. The question a drop
target asks is whether a `file://` URL built from the path opens in ANOTHER app, not whether Cmdr can read it through
`std::fs`:

- **Local** (local disks, OS-mounted shares, direct SMB while its mount is alive): each item gets a file URL plus the
  legacy filenames representation, matching Finder. No path text, which once broke browser uploads.
- **Virtual** (MTP, search-results, archive-inner paths, a direct SMB share whose mount vanished): each item gets an
  `NSFilePromiseProvider`, which only Finder can read. Archive-inner paths force Virtual even though the source volume
  is the local parent drive; the `.zip` itself stays Local.
- An unknown or absent `volume_id` resolves to Local, the back-compatible default.

❌ Don't key this on `supports_local_fs_access()`: direct SMB answers `false` there while handing out perfectly openable
`/Volumes/…` paths, and promise-only drags are rejected by every target except Finder. The mistake looks like "drag to
Mail or a browser silently does nothing" while a Finder drop keeps working.

## IPC deadlines detach, never drop

An IPC deadline is a promise about the REPLY, not permission to abandon half-written work. `tokio::time::timeout(d,
fut)` breaks that: when the deadline fires it drops `fut` wherever it happens to be.

For anything that can reach a device backend (any command taking a `volume_id`: `rename_file`,
`check_rename_validity`, `scan_for_volume_copy`, `scan_volume_for_conflicts`), dropping mid-flight means dropping a PTP
transaction mid-data-phase on MTP, which leaves the phone expecting bytes nobody will send and wedges it until replug.
See `mtp/connection/DETAILS.md` § "No dropping timeouts".

`util::timeout_detached_typed` is the shape to use: it spawns the future and races the deadline against the resulting
JOIN HANDLE. On expiry the handle is dropped, which DETACHES the task rather than cancelling it, so the caller returns
its own `TimedOut` variant on schedule and the transaction finishes safely behind it. The cost is that the work isn't
actually stopped, which is the right trade for a device op (the alternative is a bricked device) and harmless for a
local one (the deadline only ever fires on a hung mount, where dropping the future wouldn't unblock the syscall
either).

The `blocking_*` helpers already have this property for free: they wrap `spawn_blocking`, so their timeout races a join
handle too, and the blocking closure is never interrupted.
