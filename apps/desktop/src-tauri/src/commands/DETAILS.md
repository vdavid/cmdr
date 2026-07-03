# Commands module: details

Per-file function inventory and decision rationale. `CLAUDE.md` holds the must-knows.

## File inventory

- **`mod.rs`**: re-exports. `mtp` / `network` gated behind `#[cfg(any(target_os = "macos", target_os = "linux"))]`;
  `volumes` behind `#[cfg(target_os = "macos")]`; `volumes_linux` behind `#[cfg(target_os = "linux")]`.
- **`util.rs`**: `TimedOut<T>`, `IpcError`, `blocking_with_timeout`, `blocking_with_timeout_flag`,
  `blocking_result_with_timeout`.
- **`file_system/`**: directory module split by operation type. `mod.rs` has `expand_tilde()`, re-exports, tests.
  `listing.rs`: streaming + virtual-scroll listing, path queries, `find_first_fuzzy_match` (type-to-jump),
  benchmarking, `get_brief_column_text_widths` (per-column widest-filename text widths for Brief mode). `refresh_listing`
  short-circuits on watcher-backed listings (`Volume::listing_is_watched(path) == true`): the cache is kept fresh by
  `notify_mutation`, so a redundant full re-read after every transfer (the FE's `refreshPanesAfterTransfer`) used to
  wedge slow volumes (MTP 17 s + USB session collision). Logs at debug `target: "refresh_listing"` on short-circuit.
  `write_ops.rs`: create, copy, move, delete, trash, scan preview, conflict resolution, synthetic diff helpers.
  `volume_copy.rs`: cross-volume copy/move/scan, `SourceItemInput`. `scan_volume_for_conflicts` optionally takes a
  source volume id + source paths and resolves each item's real `is_directory` + size from the source volume via ONE
  batched `scan_for_copy_batch` (O(top-level items), never a subtree walk), overriding the FE's name-only placeholders
  so dir-vs-dir collisions classify as silent merges; back-compatible when omitted. `stat.rs`:
  `stat_paths_kinds(paths) -> TimedOut<Vec<Option<bool>>>`, a batched top-level "is this a directory?" probe for the
  drag-and-drop transfer path (`Some(true)` = dir, `Some(false)` = file, `None` = unknown / non-local / vanished). One
  `spawn_blocking` under the read timeout, never a subtree walk; per-item failures map to `None` so a virtual MTP/SMB
  path on the pasteboard can't poison the batch. The pure `stat_paths_kinds_blocking` helper is reused by
  `clipboard.rs::read_clipboard_files`. `drag.rs`: native drag, self-drag overlay. `e2e_support.rs`: feature-gated
  E2E/debug commands.
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
- **`network.rs`**: SMB/network shares: discovery, share listing, keychain, mounting, direct-connection upgrade,
  in-place reconnect (`reconnect_smb_volume`: backend single-flighted via `Volume::attempt_reconnect`;
  `reconnect_smb_volume_with_credentials`: the "Sign in" path after an auth-failure reconnect give-up, via
  `Volume::reconnect_with_credentials`), per-volume disconnect (`disconnect_smb_volume`: macOS shells out to
  `diskutil unmount`, Linux drops the smb2 session). Borrow Finder's saved password (macOS):
  `system_has_saved_smb_password` (prompt-free probe driving the "Use saved password" offer) and
  `upgrade_to_smb_volume_using_saved_password` (consent-gated read via `secrets::system_keychain_smb` → direct smb2 →
  copies the password into Cmdr's own store so future reconnects are silent → `CredentialsNeeded` fallback if
  absent/denied). User-initiated only. Lazy-startup hooks: `ensure_network_discovery_started` (idempotent: kicks off
  mDNS + manual-server load + smb-mount upgrade on first user network action) and `set_network_enabled` (live-applies
  the `network.enabled` toggle). Upgrade business logic lives in `network::smb_upgrade`; commands here are thin wrappers.
- **`smb_diagnostics.rs`** (debug window only): `list_smb_volumes` (the dashboard's volume picker) and
  `get_smb_diagnostics(volume_id)` (a snapshot of one volume's `smb2::SmbClient`). The snapshot DTOs mirror
  `smb2::Diagnostics` & friends with `specta::Type` derives (so `smb2` needn't depend on specta), one `impl From` per
  type.
- **`eject.rs`**: `eject_volume(volume_id)` + `get_busy_volume_ids()`, thin delegates. The teardown logic (kind
  dispatch, the pure unit-tested `decide_eject_action`, the busy-volume guard, and the `diskutil`/`umount`/MTP
  shell-out) lives in `file_system::volume::eject`; the command only maps the typed `EjectError` to `IpcError`
  (preserving the timeout flag). `get_busy_volume_ids()` bootstraps the picker's busy set (see
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
- **`child_window_state.rs`**: `get_child_window_rect` / `set_child_window_rect(label, rect)` persist per-label
  child-window (viewer, settings) geometry via `State<ChildWindowRectStore>`.
- **`settings.rs`**: port availability check, watcher debounce, menu accelerator updates, live-apply setters for
  `network.directSmbConnection`, `advanced.filterSafeSaveArtifacts`, `network.smbConcurrency`, and the restricted-window
  pair `get_restricted_window_settings` / `persist_restricted_window_setting` (the viewer's typed settings surface; see
  `capabilities/CLAUDE.md` § viewer).
- **`mcp.rs`**: `set_mcp_enabled`, `set_mcp_port` (live start/stop/port-change without app restart), `get_mcp_token`
  (returns the per-instance bearer token for in-process / E2E callers; see `mcp/DETAILS.md` § Authentication).
- **`licensing.rs`**: status query, activation, expiry, reminder, key validation.
- **`whats_new.rs`**: `get_whats_new(since_version, max)` (release entries for the What's New dialog) and
  `whats_new_dev_override` (dev-only).
- **`indexing.rs`**: `start_drive_index`, `stop_drive_index`, `get_index_status`, `get_dir_stats`,
  `get_dir_stats_batch`, `clear_drive_index`, `set_indexing_enabled`, `get_index_debug_status` (dev-only). Uses
  `State<IndexManagerState>`.
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
  `get_e2e_start_path`, `is_e2e_mode`, `is_force_onboarding`, `set_test_throttle`, `flush_file_watcher`.

## Decisions

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

**Timeout-aware return types (`TimedOut<T>` and `IpcError`).** A plain fallback is indistinguishable from a real
empty/none result ("no volumes mounted" vs "timed out before listing volumes"). `TimedOut<T>`
(`{ data, timedOut }`) for non-`Result` returns; `IpcError` (`{ message, timedOut }`) for `Result` returns. The bare
`blocking_with_timeout` stays for the rare read where the distinction genuinely doesn't matter.

**JSON for all Tauri IPC, not binary (MessagePack/Protobuf).** Benchmarked with real directory listings: MessagePack is
34-58% SLOWER than JSON despite being 17-19% smaller. Tauri serializes `Vec<u8>` as a JSON array of numbers, so binary
data gets wrapped in JSON anyway, negating size benefits and adding decode overhead. See
[benchmark data](../../../../../docs/notes/json-ipc-benchmarks.md).

**No `commands/ai.rs` and no `commands/space_poller.rs`.** Both subsystems have their own complex lifecycle (model
loading / suggestion pipelines / secret-store keys; poller init/start/watch). A thin wrapper would be pure boilerplate
forwarding, so they register directly from their own modules, keeping the command surface co-located with the
frequently-changing implementation. Space-poller commands: `watch_volume_space`, `unwatch_volume_space`,
`set_disk_space_threshold`.
