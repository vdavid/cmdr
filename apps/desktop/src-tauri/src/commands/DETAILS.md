# Commands module: details

Per-file function inventory and decision rationale. `CLAUDE.md` holds the must-knows.

## File inventory

- **`mod.rs`**: re-exports. `mtp` / `network` / `sftp` / `webdav` / `volumes` gated behind
  `#[cfg(any(target_os = "macos", target_os = "linux"))]`. There's no `volumes_linux` module and no alias for one: the
  volume commands are cross-platform and `commands/volumes.rs` serves both. See `../volumes_linux/DETAILS.md` § "One
  command module".
- **`util.rs`**: `TimedOut<T>`, `DeadlineError`, `blocking_with_timeout`, `blocking_with_timeout_flag`,
  `blocking_typed_result_with_timeout`, `timeout_detached_typed`, `Deadline` (`elapsed` / `remaining` / `total` /
  `fraction`) + `timeout_detached_within`, and `BlockingBudget`. Plus `blocking_typed_result_until_stalled` with its
  `StallWatch`: no total deadline, it gives up once the watch reports the work idle for the stall limit, and detaches
  rather than drops like the deadline helpers. Its one caller is the viewer's pulling open
  (`file_viewer/DETAILS.md` § "Watching a pull").
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
  source volume id + source paths and resolves each item's real `is_directory` + size from the source volume via
  `stat_source_paths`: one `Volume::get_metadata` per top-level path, `SOURCE_STAT_CONCURRENCY` (16) in flight, strictly
  O(top-level items) and never a subtree walk. `merge_source_types_from_stats` folds the results over the FE's
  name-only placeholders so dir-vs-dir collisions classify as silent merges; back-compatible when omitted. ❌ Don't
  swap this back to `scan_for_copy_batch`: a batch of exactly one path takes a fast path straight into
  `scan_recursive` on SMB and SFTP, so a single directory source walks its whole subtree (a 119k-file folder ate the
  entire 30 s check budget, `ERR-AYVM4`). The batch scan stays right for the transfer's own scan phase, which wants
  the tree. The source paths also drive
  `drop_self_collisions`, which removes the collisions naming a source itself so a same-folder paste doesn't announce
  every item as its own conflict. It answers with the engines' own predicates, which is what keeps the dialog and the
  write agreeing about which clashes are real (canonical:
  `../file_system/write_operations/transfer/DETAILS.md` § "Self-collision (duplicating in place)"). `stat.rs`:
  `stat_paths_kinds(paths) -> TimedOut<Vec<Option<bool>>>`, a batched top-level "is this a directory?" probe for the
  drag-and-drop transfer path (`Some(true)` = dir, `Some(false)` = file, `None` = unknown / non-local / vanished). One
  `spawn_blocking` under the read timeout, never a subtree walk; per-item failures map to `None` so a virtual MTP/SMB
  path on the pasteboard can't poison the batch. The pure `stat_paths_kinds_blocking` helper is reused by
  `clipboard.rs::read_clipboard_files`. `drag.rs`: native drag, self-drag overlay (see "Drag session locality" below).
  `e2e_support.rs`: feature-gated E2E/debug commands. `listing.rs::path_exists` is session-aware: a remote volume whose session
  drops returns an immediate `false`, so it re-checks `connection_state()` and reports `timedOut: true` unless the
  session is still live, and a transient blip can't evict the user from a network folder. An id nothing registered but
  something still names answers `timedOut: true` too, never a confident `false`: a phone its device provider lists but
  nobody dialed, or a saved SFTP / WebDAV server nobody connected (`server_volumes::place_root`). `TimedOut<T>`'s TS twin lives in
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
- **`sftp.rs`**: the SFTP surface and the wire vocabulary it speaks: `connect_sftp_volume` (a tagged
  `SftpConnectResult`, never a string), `cancel_sftp_connect`, `disconnect_sftp_volume`, `approve_sftp_host_key` / `forget_sftp_host_key` /
  `list_trusted_sftp_host_keys`, the credential trio (`save` / `has` / `delete`, keyed `host:port` + username, each on a
  blocking task because the Keychain can prompt), and the known-servers trio (`get` / `update` / `forget`). ❗ There is
  deliberately no command that returns a stored secret. The flow behind the commands is
  `network::sftp_volume_wiring`; the frontend contract is `crates/cmdr-sftp/DETAILS.md` § "Connecting from the
  frontend".
  - ❗ **Reconnecting an SFTP volume, and asking what a sign-in would want, both go through `network.rs`**:
    `reconnect_volume`, `reconnect_volume_with_credentials`, and `get_volume_sign_in_state`. All three are
    backend-neutral: they delegate to a `Volume` trait method on whatever is registered, so no backend owns a copy.
  - ❗ **`connect_sftp_volume`'s result carries `rung` and ❌ nothing about a later sign-in.** The rung is a fact about
    that dial; what a sign-in would ask for is decided per dial too, so it is a query, not a payload.
  - ❗ **`connect_sftp_volume`'s `attempt_id` is the CALLER's, made before the call**, and `cancel_sftp_connect` takes
    the same one. The command doesn't answer for up to 30 s, so an id it returned would be useless for arming a cancel
    button. The table behind it: `network/DETAILS.md` § "The attempt table, and why the id is the caller's".
- **`webdav.rs`**: the WebDAV surface, shaped like `sftp.rs` minus host keys: `connect_webdav_volume` (a tagged
  connect result, never a string; `invalid_url` is the one outcome the app adds to the crate's), `cancel_webdav_connect`,
  `disconnect_webdav_volume`, the credential trio (`save` / `has` / `delete`, keyed `scheme://host:port` + username),
  the known-servers trio (`get` / `update` / `forget`), and `get_webdav_unattended_reconnect`. Same rules as SFTP: the
  `attempt_id` is the caller's, reconnect and sign-in go through `network.rs`, and no command returns a stored secret.
  The flow is `network::webdav_volume_wiring`; the contract is `crates/cmdr-webdav/DETAILS.md` § "Connecting from the
  frontend".
- **`servers.rs`**: the protocol-agnostic server family, a FACADE over the three above. The hub, the switcher, the
  sign-in sheet, and the pane banner speak about servers rather than about SFTP, WebDAV, and SMB, so this is the
  surface they call: `list_saved_servers` (the union of the two saved-server stores plus SMB hosts from
  `known_shares.rs` and `manual_servers.rs`), `connect_saved_place`, `connect_server`, `cancel_server_connect`,
  `disconnect_place`, `set_place_pinned`, `forget_server`, `forget_server_secret`, `update_saved_server`.
  - `list_saved_servers` names an SFTP or WebDAV account by its label and publishes `name_source: fallback` when
    nobody named it, so the edit sheet can open an empty name field with the label as its placeholder, and nothing on
    the frontend derives one (`network/DETAILS.md` § "An unnamed server's label, and names that only repeat the
    address").
  - ❗ **`ServerConnectOutcome` is the SUPERSET** of the two per-protocol enums, so a sign-in UI branches once instead
    of twice. `auth_method_unsupported` stays its own outcome rather than collapsing into `authentication_rejected`: a
    Digest-only server never saw the password, so "check your password" is the wrong fix to put in front of someone.
  - ❗ **`connect_saved_place` refuses a REGISTERED id** (`SavedPlaceRefusal::AlreadyConnected`). Re-dialing would
    register a second volume under the same id; a session that dropped is mended by
    `network.rs::reconnect_volume_with_credentials`, which is what enforces the read-only username rule and the
    never-seeds rule. Neither refusal is something a person did, which is why they are an `Err` rather than an outcome.
  - ❗ **An SMB host lists NO places and cannot be pinned here.** `known_shares.rs` stores no share rows, carries no
    port, and a mounted share's id comes from `statfs` (an IP where the store holds an mDNS name), so no id derivable
    from the store would match the mounted volume. SMB places keep reaching the switcher as mounted volumes.
  - ❗ **`forget_server` drops the SESSION too**, unregisters the volume, and emits `VolumeUnmounted` BEFORE
    `volumes-changed`. A forgotten server is gone: leaving its session up would keep a switcher row no store knows about
    and no second "Forget" can reach, and the pane consumer needs the redirect to land ahead of the row's removal, or it
    is left standing on a volume nothing can name. A tab on a forgotten server becomes a home tab.
  - `disconnect_place` emits `VolumeUnmounted` as well, so a pane goes home rather than failing every listing against a
    registry that stopped answering. ❌ No ordering rule there: the ROW survives a disconnect (it becomes `saved`), so
    the consumer has nothing to race. ❌ And no new "you disconnected" pane state: a `saved` row dials on activation.
  - `update_saved_server` takes a `ServerTarget`, the same shape the add sheet collects, because an edit and an add
    differ only in whether the fields arrived prefilled. It carries no PIN: `set_place_pinned` is the one writer that
    moves one, because the stores' `remember` deliberately preserves a stored pin on every replace. It answers a typed
    `SavedServerOutcome` (`network/saved_server_fields.rs`), the same one the per-protocol `update_known_*_server`
    commands answer, and a refusal writes nothing: `start_folder_outside_root` (connected or not), and for a connected
    place, whose edit applies live, `root_not_found`, `start_folder_not_found`, and `unreachable` (`network/DETAILS.md`
    § "Editing a connected place"). All three commands are `async`, because a live edit asks the server.
  - ❗ **A saved edit republishes the volume list, whatever it changed** (`update_saved_server` requests
    `volumes-changed` on `Saved`). The rows carry each place's label and landing, and neither an unconnected place's
    edit nor a start-folder-only one moves anything in the registry that would announce it. The servers hub re-reads
    `list_saved_servers` on the same broadcast, which is how it learns an edit landed.
  - ❗ **A start folder outside the remote root is a typed refusal on both writers a person types into.**
    `update_saved_server` answers `start_folder_outside_root`, and `connect_server` answers
    `ServerConnectOutcome::StartFolderOutsideRoot` BEFORE dialing, so nothing is registered or saved. Landing the pane
    somewhere else instead would hide the typo. The rule itself: `network/DETAILS.md` § "The start folder, and what a
    connect carries beside its params".
  - **This family is where a NEW backend plugs in**, and that is why it exists as a facade over correct, tested
    per-protocol enums rather than as a rewrite of them: one more `ServerTarget` arm, one more saved-server store, and
    whatever outcomes the protocol adds to the superset. The frontend then branches once, in a `switch` it already has.
    S3 is the shape this was sized against: its account is an endpoint plus an access key, its places are buckets, and
    its sign-in is the reserved `SignInShape::AccessKeys` variant (`crates/cmdr-fs/src/volume/connection.rs`, and
    `apps/desktop/src/lib/servers/DETAILS.md` § "The renderer table").
- **`network.rs`**: SMB/network shares: discovery, share listing, keychain, mounting, direct-connection upgrade,
  in-place reconnect (`reconnect_volume`: backend single-flighted via `Volume::attempt_reconnect`;
  `reconnect_volume_with_credentials`: the "Sign in" path after an auth-failure reconnect give-up, via
  `Volume::reconnect_with_credentials`), what FORM a sign-in takes (`get_volume_sign_in_state`, via
  `Volume::sign_in_prompt`, a `SignInShape` tagged on `kind`, read live when a banner renders, ❌ never carried on a
  connect result and ❌ never derived from the protocol or the sheet's mode; an unregistered id and a backend with no
  story of its own both answer `password`, the safe way to be wrong, and a share answers `username_password` because
  the share is the identity and the account is a field on it), per-volume disconnect (`disconnect_smb_volume`: macOS shells out to
  `diskutil unmount`, Linux drops the smb2 session). Borrow Finder's saved password (macOS):
  `system_has_saved_smb_password` (prompt-free probe driving the "Use saved password" offer) and
  `upgrade_to_smb_volume_using_saved_password` (consent-gated read via `secrets::system_keychain_smb` → direct smb2 →
  copies the password into Cmdr's own store so future reconnects are silent → `CredentialsNeeded` fallback if
  absent/denied). User-initiated only. Lazy-startup hooks: `ensure_network_discovery_started` (idempotent: kicks off
  mDNS + manual-server load + smb-mount upgrade on first user network action) and `set_network_enabled` (live-applies
  the `network.enabled` toggle). The "Connect directly" upgrade lives in `network::smb_connect_directly` and the
  auto-upgrade in `network::smb_upgrade`; the three `upgrade_to_smb_volume*` commands only kick mDNS and delegate, and
  `system_has_saved_smb_password` delegates to `smb_connect_directly::system_has_saved_password`. They answer a bare
  `UpgradeResult`, with no `Err` channel: a volume that's gone, isn't an SMB mount, or whose mount didn't answer in time
  is a variant (`VolumeGone` / `NotSmbMount` / `MountNotResponding`) the frontend words, and elsewhere than macOS the
  saved-password one answers `CredentialsNeeded`. Their filesystem timeout lives in `network`, around the mount read
  alone (`network/DETAILS.md` § "Connect directly answers a gone volume"), because the saved-password door goes on to
  wait for the Keychain consent dialog, which no deadline may cut off.
  `list_shares_with_credentials` carries `#[allow(clippy::too_many_arguments)]`: Tauri params must be top-level args.
- **`smb_diagnostics.rs`** (debug window only): `list_smb_volumes` (the dashboard's volume picker) and
  `get_smb_diagnostics(volume_id)` (a snapshot of one volume's `smb2::SmbClient`). The snapshot DTOs mirror
  `smb2::Diagnostics` & friends with `specta::Type` derives (so `smb2` needn't depend on specta), one `impl From` per
  type.
- **`memory_diagnostics.rs`** (macOS only): `get_memory_diagnostics(sizes_per_tag)`, one payload answering "what is
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
  to update the listing cache (both local and volume-aware paths). ❗ `check_rename_validity` and
  `check_rename_permission` stay UNMANAGED: they answer while someone is typing, so they take the snappy read-only path
  instead of `manager::run_instant`, which busy-marks the volume for a mutation that isn't happening yet.
- **`volume_id` on the write commands.** `create_directory` / `create_file` / `rename_file` only expand tilde (root),
  resolve the `volume_id`, and apply the 5 s write timeout, shipping the typed `MutationError` unchanged; the logic and the managed instant op live
  in `file_system::write_operations::{create,rename}`. For a non-root `volume_id`, `delete_files` uses the volume-aware
  delete and skips local `validate_sources` (MTP virtual paths fail `symlink_metadata`), and `rename_file` passes the id
  through and skips permission checks. The local rename notifies the listing cache via `notify_rename_in_listing`, the
  volume one via its own `notify_mutation`.
- **`operation_log.rs`**: the journal's read side (`get_recent_operation_log_entries`, `get_operation_log_detail`, both
  thin pass-throughs over `operation_log::query` on a short-lived read-only connection) plus two write entries over the
  same rollback engine. `rollback_operation` reverses ONE and returns after DISPATCH, so the history dialog's Roll back
  button hands the reversal to the operation queue; `undo_operations` reverses SEVERAL and awaits each, because Ask
  Cmdr's rename undo has to report a tally. Both refuse with the typed `RollbackRefusal`, never a sentence.
- **`restricted_paths.rs`**: `get_restricted_paths`: read-only snapshot for the frontend store bootstrap. See
  `crate::restricted_paths` for the state machine and the `restricted-paths-changed` event payload.
- **`file_viewer.rs`**: session lifecycle, regex/literal search with mode flags, word wrap, menu state, encoding pickers
  (`viewer_set_encoding` / `viewer_get_encoding_options`), tail mode (`viewer_set_tail_mode`), `viewer_reload`.
- **`menu.rs`**: native menus and menu-bar state: the context menus (file / breadcrumb / volume row / parent row /
  tab / network host), the view-mode + hidden-files + pin-tab + reopen-tab sync commands, and `activate_window_menu`
  (per-window focus-gain: swaps the macOS app menu bar between main/viewer, then enables/disables file-scoped items via
  the private `set_menu_context` helper; see `menu/DETAILS.md`).
  - ❗ **`show_file_context_menu` holds a `ServicesLoan` across `popup()`** (macOS): AppKit's one Services menu is
    borrowed for the life of the menu and pointed at the right-clicked rows. `popup()` runs the whole tracking loop, so
    the binding is `let _services_loan = …` and ❌ never `let _ = …`; see `menu/DETAILS.md` § Services in the
    right-click menu.
  - ❗ **`build_file_context_info` enumerates the share services on THIS thread** (macOS), taking its own
    `MainThreadMarker` rather than hopping: `file_system::share` pairs the offer with the click by index through a
    thread-local, and `on_menu_event` reads it back on the main thread. A sync `#[tauri::command]` runs there, which is
    also what lets `popup()` work; with no marker the `Share` submenu is simply absent. `file_system/DETAILS.md` § "The
    Share submenu".
  - ❗ **`show_volume_row_context_menu` takes an optional `ServerRowMenu`**, and a server row gets Open / Edit server… /
    Disconnect / Pin or Unpin / Forget saved password / Forget server instead of Eject: "Eject" promises safe-to-unplug
    and a server has nothing to unplug. The CALLER says which items apply (this command is synchronous, and a
    secret-store read on the popup path would block the IPC handler thread); `busy_volume_ids()` is filled in here and
    disables the three destructive items exactly like Eject. ❌ Open, Edit…, and the pin pair are never `busy`-disabled:
    none of them touches a session. ❗ There is deliberately no "is a secret stored" field — see
    `navigation/server-row-actions.ts` for why the Keychain is not read on a right-click.
  - ❗ **The picked action crosses as the typed `VolumeContextActionKind`**, ❌ never a free string. `menu_handlers.rs`
    maps menu id → action through one table, so what it recognizes and what it emits can't drift.
- **`quick_look.rs`**: `quick_look_open` / `quick_look_set_path` / `quick_look_close` (native `QLPreviewPanel`
  singleton on macOS, no-op stubs elsewhere; 2 s main-thread-hop timeout). See `crate::quick_look`.
- **`window_ordering.rs`**: `show_main_window` / `order_window_to_back`. `show_main_window` is the ONE path that makes
  Cmdr visible (the window is created `"visible": false`; the frontend calls it from `onMount`), and it takes a
  `ShowReason`: a `launch` show follows `show()` with `set_focus()`, a `repaint-repair` re-show doesn't. The activation
  is load-bearing, because `show()` is only `makeKeyAndOrderFront:`, which makes the window key within Cmdr without
  making Cmdr the active app: the update toast's relaunch spawns the binary directly
  (`tauri::process::restart_macos_app`), bypassing LaunchServices, so the new process is granted no activation and its
  window opens behind whatever is frontmost (verified on macOS 15.5 with Tauri 2.11.5, from a user-reported background
  start after "Restart now", 2026-08-27). `order_window_to_back` is E2E-only z-ordering (order to back without focus),
  a no-op off macOS / outside E2E, and the E2E branch of `show_main_window` orders back instead of showing.
- **`file_actions.rs`**: direct file actions from the palette / menus: `show_in_finder`, `get_info`, `open_in_editor`,
  `copy_to_clipboard`, and `cloud_make_available_offline` / `cloud_remove_download` (iCloud Drive download/eviction via
  `FileManager` ubiquity APIs; see `file_system/cloud_actions.rs`). `google_drive_links(path)` returns a Drive item's
  `viewUrl` plus its `geminiUrl` (absent for folders) or `None`, backing the three Drive menu items from ONE
  resolution; that resolution lives in `file_system/google_drive/`, and a timeout yields `None` rather than an error
  because a missing link only means the caller shows nothing. Plus the "open terminal
  here" pair,
  `list_terminal_apps(app_choice)`, `open_terminal_here(path, volume_id, app_choice)`, and the sync
  `terminal_app_display_name(app_choice)`, all pass-throughs to `../file_system/terminal.rs`, which owns the table, the
  recipes, and the volume gate. The chosen app arrives as an argument because the frontend owns the settings store.
  The display-name lookup is I/O-free on purpose: the toast that needs it fires exactly when the app it names has been
  uninstalled, so the table is all that's left to read a name from. `open_path`, `open_in_editor`, and `open_terminal_here` all
  swap to a recording variant under `playwright-e2e`, funneling into `crate::open_mock` so a suite run leaves no orphan
  windows.
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
  it when that holder ends (`indexing/lifecycle/DETAILS.md` § The one walk a volume remembers), a promise the UI has to
  voice, since nothing else marks the wait, and they stay two variants because the user's next question differs.
- **`media_index/`**: the media-index IPC surface, one module per family: `search.rs` (OCR, tag, semantic,
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
- **`error_reporter.rs`**: Flow A's `prepare_error_report_preview(userNote?, email?)` and
  `send_error_report(userNote?, email?, id?)`, two-step so the preview dialog is deterministic without shipping the
  full bundle through IPC twice. Hand the preview's `id` back to the send or the report lands under a different one
  than the dialog showed. Plus `get_auto_sent_report_preview()` (what Flow B already sent, with `can_amend`; `null`
  when nothing was auto-sent this run) and `amend_error_report(userNote?, email?)`, which adds a note to that one
  report and so takes no id (it resolves the target from the stash, then supplies
  `error_report_amend_url(id)` the way the send path supplies its own URL). `flow_a_request` is the single place note validation, id reuse, and wrapping an address in
  `AttachedEmail` happen. Network skipped in dev/CI. The two preview commands are dispatch-only (a `BundleManifest`
  holds a `serde_json::Value`, which specta can't describe), so the frontend reaches them by raw invoke.
- **`analytics.rs`**: `track_event(name, props_json)`, a thin pass-through to `posthog::capture` for the open set of
  frontend feature events. No capability entry; the PII-free prop contract lives in `analytics/CLAUDE.md`.
- **`usage.rs`**: `get_launch_day_count()`, the read seam over the on-device launch-day ledger, so the frontend can gate
  a hint on "at least N days of use". Read-only (Rust appends at startup) and 2 s-deadlined; a missing, unreadable, or
  slow ledger answers 0 so a hint stays silent. Not telemetry and deliberately not a setting: `../usage/CLAUDE.md`.
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
- **`e2e.rs`**: E2E/test-support hooks. The four READERS are always compiled in (reading an unset env var is a no-op in
  production): `get_e2e_start_path`, `is_e2e_mode`, `ask_cmdr_fake_active`, `is_force_onboarding`. Everything below them
  is `#[cfg(feature = "playwright-e2e")]`, so no production binary carries a command that CHANGES behavior:
  `set_test_throttle`,
  `set_test_rollback_throttle` (the same idea for the operation-log rollback engine's item loop, on its own knob so
  pacing a reversal doesn't pace the copy that staged it), `set_test_scan_preview_delay`, `flush_file_watcher`, `force_agent_wake` (stages one folder's activity on the wake
  loop's real channel and makes it act now, on that folder alone; it skips the timer and the proactive toggle, never a
  gate, and its `quiet` flag picks which script the wake's fake assistant plays), `stage_agent_rollup` (the same
  staging without the wake, so a spec can prove the force reports on its own folder). Both are in
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
commands accept only opaque proposal and row ids.

Its four pre-turn gates (no agent store, consent not accepted, no resolvable provider, a local context window under the
one-turn floor) are also the ONLY account of the Ask Cmdr funnel's top, because none of them reach `run_turn`. That's
why `AskCmdrSendRefusal` has two constructors and no struct literal: `of` and `detailed` both report the anonymous
`ask_cmdr_turn` refusal, and a literal would silently lose the gate. `agent/chat/DETAILS.md` § The turn event. `apply_bulk_rename` consumes an exact accepted preflight then delegates
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
See `crates/cmdr-mtp/src/connection/DETAILS.md` § "No dropping timeouts".

`util::timeout_detached_typed` is the shape to use: it spawns the future and races the deadline against the resulting
JOIN HANDLE. On expiry the handle is dropped, which DETACHES the task rather than cancelling it, so the caller returns
its own `TimedOut` variant on schedule and the transaction finishes safely behind it. The cost is that the work isn't
actually stopped, which is the right trade for a device op (the alternative is a bricked device) and harmless for a
local one (the deadline only ever fires on a hung mount, where dropping the future wouldn't unblock the syscall
either).

The `blocking_*` helpers already have this property for free: they wrap `spawn_blocking`, so their timeout races a join
handle too, and the blocking closure is never interrupted.

### An optional leg gets a sub-budget, never the whole deadline

When a command has one leg that MUST answer and another whose failure it treats as non-fatal, the optional leg runs on
`deadline.fraction(divisor)`: a budget worth `1/divisor` of the deadline's original total, and never outliving what's
left of the parent. `scan_volume_for_conflicts` does this with `SOURCE_STAT_BUDGET_DIVISOR` (3), so the source stats
get 10 s of the check's 30 s and the mandatory destination scan keeps the rest.

Why it matters: sharing one `Deadline` lets a wedged optional leg spend everything, after which the leg that actually
answers the question fails instantly on an empty budget. The user then reads a timeout blamed on a device the command
never got around to asking (here: "couldn't read the destination" for a hung SOURCE share), which sends them debugging
the wrong end.
