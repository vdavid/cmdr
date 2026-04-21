# Changelog

All notable changes to Cmdr will be documented in this file.

The format is based on [keep a changelog](https://keepachangelog.com/en/1.1.0/), and we use
[Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Breaking (internal API):** The `Volume` trait no longer exposes `export_to_local` and `import_from_local`. Every
  cross-volume copy now flows through `open_read_stream` + `write_from_stream` (or the APFS clonefile fast path when
  both sides are `LocalPosixVolume` on the same volume). Adding a new volume backend takes two streaming methods
  instead of four, and concurrency (coming in Phase 4.2) plugs into one dispatch point. Only in-tree consumer was
  `volume_copy.rs`; no external crates depended on the removed methods. See
  `docs/notes/phase4-volume-copy-unification.md`.

## [0.12.0] - 2026-04-18

### Added

- Friendly error pane for listing failures — plain-language titles, provider-aware suggestions (Dropbox, Google Drive,
  OneDrive, iCloud, MacDroid, macFUSE/SSHFS, VeraCrypt, and 12 more), collapsible technical details, and retry with
  attempt history for transient errors. Two-layer mapping: 37 macOS errno codes → `FriendlyError` (Transient /
  NeedsAction / Serious categories), then path- and `statfs`-based enrichment for 19 cloud/mount providers
  ([eec50ff](https://github.com/vdavid/cmdr/commit/eec50ff), [cc7bb3](https://github.com/vdavid/cmdr/commit/cc7bb3))
- Live disk space updates in the status bar — backend polls `statvfs`/NSURL per watched volume, emits
  `volume-space-changed` when deltas exceed a configurable threshold (`advanced.diskSpaceChangeThreshold`). Deduplicates
  per volume across panes, 3s timeout per fetch so hung mounts don't stall
  ([d67dd3](https://github.com/vdavid/cmdr/commit/d67dd3))
- Breadcrumb "Copy path" context menu — right-click the breadcrumb header to copy the current directory's full path,
  `file.copyCurrentDirectoryPath` configurable in Settings ([eb4d3c](https://github.com/vdavid/cmdr/commit/eb4d3c))
- SMB streaming read + write — `SmbVolume` now implements `open_read_stream` and `write_from_stream`, so MTP↔SMB and
  SMB↔SMB copies flow through memory without temp files, and large local↔NAS copies stay bounded to ~1 MiB peak RAM
  instead of buffering the whole file ([ac71bd](https://github.com/vdavid/cmdr/commit/ac71bd),
  [a82709](https://github.com/vdavid/cmdr/commit/a82709), [35120d](https://github.com/vdavid/cmdr/commit/35120d),
  [04359](https://github.com/vdavid/cmdr/commit/04359))
- SMB mount disambiguation — two shares with the same name from different servers now get unique mount points (`public`,
  `public-1`, …) via `kNetFSForceNewSessionKey`. Volume switcher shows `{share} on {server}` so it's clear which is
  which ([76671b](https://github.com/vdavid/cmdr/commit/76671b))
- SMB login form on direct-connection upgrade — instead of an opaque error toast, shows `NetworkLoginForm` inline when
  credentials are missing or wrong; structured `UpgradeResult` distinguishes auth needs from network errors
  ([b315b4](https://github.com/vdavid/cmdr/commit/b315b4))
- Instant dialog open for large selections — batch `get_paths_at_indices` / `get_files_at_indices` Tauri commands
  replace per-file IPC loops. Copy/Move dialogs for 50k files open in ~1 ms instead of ~10 s
  ([48ea60](https://github.com/vdavid/cmdr/commit/48ea60))
- MTP Samsung support — late-arriving storages announced via `StoreAdded`/`StoreRemoved` are registered/unregistered on
  the fly; phones that report 0 storages at connect time now appear in the volume selector
  ([14b3ac](https://github.com/vdavid/cmdr/commit/14b3ac))
- MTP batch scan for copy — `scan_for_copy_batch` groups paths by parent directory and lists each parent once. A copy of
  14,825 files now uses one USB `GetObjectHandles` per parent instead of one per file
  ([70978c](https://github.com/vdavid/cmdr/commit/70978c))
- Rename: skip extension confirm on case-only changes — `photo.JPG` → `photo.jpg` no longer triggers the extension
  dialog or the red-border guard ([1401017](https://github.com/vdavid/cmdr/commit/1401017))
- Filename + extension split in Full view — `photo.jpg` renders as `photo` + `jpg` in separate columns, inline rename
  editor spans both cells for more room ([275d091](https://github.com/vdavid/cmdr/commit/275d091))
- Volume selector polish — spacebar area is clickable, dropdown renders over the function key bar via `position: fixed`,
  resizes on window resize, submenus render outside the scrollable dropdown to avoid clipping
  ([700eac](https://github.com/vdavid/cmdr/commit/700eac))
- File operation dialog polish — thousands-separator formatting for all file/dir counts, pixel-accurate mid-text
  truncation via `@chenglou/pretext` (preserves extensions and path separators), fixed 500 px width eliminates jitter
  during progress ([d67dd3](https://github.com/vdavid/cmdr/commit/d67dd3))
- Debug window error-pane preview — all 47 error states with provider dropdown and L/R pane trigger buttons, wired via
  `debug-inject-error` events ([cc7bb3](https://github.com/vdavid/cmdr/commit/cc7bb3))

### Fixed

- Don't log user cancels as ERROR — cancellation now propagates as `VolumeError::Cancelled` at three SMB sites instead
  of being erased into `IoError("Operation cancelled")`; `copy_between_volumes` / `move_between_volumes` skip the
  duplicate `write-error` emit on cancels ([6f79392](https://github.com/vdavid/cmdr/commit/6f79392))
- Copy/move crash from reactivity race — `TransferDialog` now guards `$derived.by` against null `sourcePaths`, tracks a
  `destroyed` flag for stale async IPC results, and defers prop teardown via `queueMicrotask` so `onDestroy` fires first
  ([0cdd7d](https://github.com/vdavid/cmdr/commit/0cdd7d))
- Stuck "Scanning 0 files" transfer dialog — `TransferDialog` now awaits `startScanPreview` before calling `onConfirm`
  so `previewId` is always set; `TransferProgressDialog` no longer tries to adopt `previewId` from racing events
  ([dd06d68](https://github.com/vdavid/cmdr/commit/dd06d68))
- Double-dispatched MCP autoConfirm copies — `waitForScanThenStart` raced between the scan-complete listener and the
  status check. Both paths now converge on an idempotent `kickOff()` guarded by a `started` flag
  ([4af22ab](https://github.com/vdavid/cmdr/commit/4af22ab))
- File watcher panicked on 500+ external changes — fallback-to-full-reread path spawned via `tokio::spawn` from the
  notify-rs debouncer thread (no Tokio runtime context). Switched to `tauri::async_runtime::spawn`
  ([4087e30](https://github.com/vdavid/cmdr/commit/4087e30))
- APFS-aware copy space check — `validate_disk_space` and `get_space_info_for_path` used `statvfs` which ignores
  purgeable space (APFS snapshots, iCloud caches); now use `get_volume_space()` on macOS to match Finder and the status
  bar ([3454656](https://github.com/vdavid/cmdr/commit/3454656))
- SMB paths round-trip correctly — three fixes: `parse_smbutil_output` now keyword-detects columns instead of
  `split_whitespace` (share names like "Time Machine" work), `manual_servers.rs` serializes concurrent writes via
  `STORE_LOCK`, and file viewer search uses UTF-16 code units for match `column`/`length` so highlights land in the
  right place after emoji / CJK ([97c0481](https://github.com/vdavid/cmdr/commit/97c0481))
- SMB port handling — virtual E2E hosts now prefer `SMB_E2E_{SVC}_PORT` over docker-compose host ports; port extracted
  from `statfs` and threaded through `upgrade_to_smb_volume`; volume display name resolves mDNS service names to human
  hosts ([c26f7e8](https://github.com/vdavid/cmdr/commit/c26f7e8),
  [017b7043](https://github.com/vdavid/cmdr/commit/017b7043))
- SMB "Connect directly" on QNAP — `smb2` bumped to tolerate servers that split compound responses across transport
  frames (spec-legal per MS-SMB2 3.3.4.1.3); the `expected 3 compound responses, got 1` warnings against QNAP / Samba
  NASes are gone ([2666db8](https://github.com/vdavid/cmdr/commit/2666db8))
- Clear-index button WCAG contrast — the disabled state with `opacity: 0.4` dropped contrast to 1.78:1 in light mode;
  now hidden entirely when there's no index ([b1915d9](https://github.com/vdavid/cmdr/commit/b1915d9))
- Network pane stuck on old host after mount success — `NetworkMountView.handleShareSelect` didn't propagate the cleared
  host up to `FilePane`, so re-entering Network re-mounted `ShareBrowser` for the stale host
  ([41c1860](https://github.com/vdavid/cmdr/commit/41c1860))
- `llama-server` session startup on Linux — new `secrets/` module keeps credential storage working even when the keyring
  is locked (real write/read/delete probe), respects `CMDR_DATA_DIR` for the encrypted-file fallback, and bypasses
  Keychain entirely in dev mode so Cmdr doesn't prompt on every HMR
  ([55ccde3](https://github.com/vdavid/cmdr/commit/55ccde3))

### Improved

- Async `Volume` trait — 20 I/O methods now return `Pin<Box<dyn Future + Send>>`, eliminating every `block_on` in the
  volume layer. `LocalPosixVolume` uses `spawn_blocking`, `MtpVolume` talks to `MtpConnectionManager` via `.await`,
  `SmbVolume` swaps to `tokio::sync::Mutex`. Copy/move/delete pipelines are plain `async fn`; conflict resolution uses
  `tokio::sync::oneshot` instead of `Condvar`. Removes the nested-runtime panics that previously broke MTP↔SMB and
  bounds per-file memory for cross-volume streaming ([531bb9b](https://github.com/vdavid/cmdr/commit/531bb9b),
  [9d4982a](https://github.com/vdavid/cmdr/commit/9d4982a), [694ddc1](https://github.com/vdavid/cmdr/commit/694ddc1))
- MTP channel-based read stream — `MtpReadStream` is now a consumer of a bounded `sync_channel(4)` filled by a
  background task, so it's safe to call `next_chunk()` inside any runtime context (previous `block_on`-based stream
  panicked when nested inside SMB's `block_on`) ([1598f8c](https://github.com/vdavid/cmdr/commit/1598f8c))
- Cancelled SMB uploads use `FileWriter::abort()` — skips the server FLUSH/fsync on a file we're about to delete, saving
  ~100 ms to ~1 s per cancel on slow NAS ([6fa0780](https://github.com/vdavid/cmdr/commit/6fa0780))

### Non-app

- Design-time WCAG contrast checker (`scripts/check-a11y-contrast`) — parses `app.css` + every Svelte `<style>` block,
  resolves CSS variables (nested `color-mix(in srgb | oklch, ...)` with premultiplied alpha), computes contrast in
  light + dark modes separately, flags pairs below 4.5:1 / 3:1. Runs in ~300 ms on 85 files. Replaces the flaky
  `color-contrast` axe rule (WebKit builds disagreed on how to resolve nested `color-mix()` chains)
  ([db25f0d](https://github.com/vdavid/cmdr/commit/db25f0d), [55af258](https://github.com/vdavid/cmdr/commit/55af258))
- Fix 18 real WCAG AA contrast failures surfaced by the new checker — add `--color-error-text` / `--color-warning-text`
  tokens for text on tinted bgs, swap warning-white to `--color-accent-fg`, keep base accent color on hover and use
  underline for affordance ([747507f](https://github.com/vdavid/cmdr/commit/747507f),
  [67d42ba](https://github.com/vdavid/cmdr/commit/67d42ba), [4a15a53](https://github.com/vdavid/cmdr/commit/4a15a53))
- Tier 3 component-level a11y tests — 61 `.a11y.test.ts` files (from 5), 146 passing, runs in ~6.3 s via Vitest/jsdom.
  New `a11y-coverage` check enforces that every tracked `.svelte` file under `apps/desktop/src/lib/` has a colocated
  test or an allowlist entry ([33300a4](https://github.com/vdavid/cmdr/commit/33300a4),
  [d56c1df](https://github.com/vdavid/cmdr/commit/d56c1df), [398bf7a](https://github.com/vdavid/cmdr/commit/398bf7a))
- Switch from Lucide components to UnoCSS pure-CSS icons — zero JS runtime, recolor via `currentColor` + CSS vars,
  `mask-image` rendering ([9354806](https://github.com/vdavid/cmdr/commit/9354806))
- File-length check + allowlist — tool flags files growing past a tracked size, 31 baseline entries. Split 20+ long
  files into sub-800-line modules (`volume_copy.rs`, `scan.rs`, `smb.rs`, `integration_test.rs`, `AiSection.svelte`,
  `+page.svelte`, `DualPaneExplorer.svelte`, and more) — pure mechanical splits, no logic changes
  ([7514cb4](https://github.com/vdavid/cmdr/commit/7514cb4), [2939bfe](https://github.com/vdavid/cmdr/commit/2939bfe),
  [4514a83](https://github.com/vdavid/cmdr/commit/4514a83), [3155609](https://github.com/vdavid/cmdr/commit/3155609))
- `OperationEventSink` + `ListingEventSink` traits decouple copy/move/listing pipelines from `tauri::AppHandle` —
  enables unit tests without a Tauri runtime; `CollectorEventSink` stores events for assertions
  ([35fea46](https://github.com/vdavid/cmdr/commit/35fea46), [0a6ae61](https://github.com/vdavid/cmdr/commit/0a6ae61))
- Vendor the smb2 consumer Docker containers under `.compose/` with `VENDORED.md` bump instructions — CI no longer needs
  to extract them via `cargo run` (which required GTK deps that aren't in the test container)
  ([d50b963](https://github.com/vdavid/cmdr/commit/d50b963))
- Linux E2E runs in Docker to match local — same image, same `./scripts/e2e-linux.sh` flow developers use. Cache volumes
  overridable via env so `actions/cache` can persist them between runs
  ([8803c3c](https://github.com/vdavid/cmdr/commit/8803c3c), [f39177c](https://github.com/vdavid/cmdr/commit/f39177c))
- Remove legacy CrabNebula/WebDriverIO macOS E2E suite — Playwright now covers all 15 macOS tests. Drops 9 npm devDeps,
  the `automation` Cargo feature, and `tauri-plugin-automation`
  ([4cecfb9](https://github.com/vdavid/cmdr/commit/4cecfb9))
- E2E file entries matched by `data-filename` attribute, not `.col-name` text — decouples tests from display format
  ([5a8f6af](https://github.com/vdavid/cmdr/commit/5a8f6af))
- Upgrade `rustls-webpki` 0.103.11 → 0.103.12 (RUSTSEC-2026-0098/0099), `bitstream-io` 4.9.0 → 4.10.0 (drops yanked
  `core2`); `cargo-audit` now parses `--json` for one-line-per-advisory output with 26 upstream ignores
  ([3734502](https://github.com/vdavid/cmdr/commit/3734502))
- HMR crash recovery — catch SvelteKit TDZ crash on root layout HMR and auto-reload via `sessionStorage` debounce, move
  `virtual:uno.css` HMR handler to a stable `hmr-recovery.ts` module
  ([700eac](https://github.com/vdavid/cmdr/commit/700eac))
- Auto-sign E2E binaries with a "Cmdr Dev" self-signed cert — no more Keychain prompts during `scripts/check.sh`
  ([23b920d](https://github.com/vdavid/cmdr/commit/23b920d))
- Error-handling contributor guide (`docs/error-handling.md`) — maps the Rust → IPC → Svelte error pipeline, lists all
  `VolumeError` variants, errno codes, providers ([a4a5fdb](https://github.com/vdavid/cmdr/commit/a4a5fdb))

## [0.11.1] - 2026-04-10

### Added

- Striped rows setting — alternating row shading in Full and Brief view modes, auto-adapts to light/dark mode
  ([faa2534](https://github.com/vdavid/cmdr/commit/faa2534))
- MTP per-file copy progress and mid-file cancellation — progress callback on every USB chunk, instant cancel via USB
  SIC abort (~300ms instead of draining the full stream) ([ac5ec4d](https://github.com/vdavid/cmdr/commit/ac5ec4d),
  [a66adf6](https://github.com/vdavid/cmdr/commit/a66adf6))

### Fixed

- View menu Full/Brief checkmarks now sync when switching panes
  ([6e36a49](https://github.com/vdavid/cmdr/commit/6e36a49))
- MTP: export files directly instead of guess-and-fallback, eliminating `ObjectNotFound` error log spam on every copy
  ([0cc675a](https://github.com/vdavid/cmdr/commit/0cc675a))
- MTP: fix mid-stream cancel corrupting USB session and making device unresponsive — bump `mtp-rs` to 0.11.0
  ([a66adf6](https://github.com/vdavid/cmdr/commit/a66adf6))
- A11y: darken `--color-accent-text` for WCAG AA compliance, fix search input placeholder opacity
  ([b7744dd](https://github.com/vdavid/cmdr/commit/b7744dd))
- Fix lint errors in `VolumeBreadcrumb`, `TransferDialog`, `+layout.svelte`
  ([90b5ea0](https://github.com/vdavid/cmdr/commit/90b5ea0))
- Fix Linux compilation — move shared SMB types to cross-platform module, add `get_smb_mount_info` for Linux
  ([00c5f18](https://github.com/vdavid/cmdr/commit/00c5f18))

## [0.11.0] - 2026-04-10

### Added

- SMB direct connections — file operations now go through the smb2 protocol directly, bypassing the OS mount (~4x
  faster). The OS mount stays for Finder/Terminal compatibility
  ([dea46ec](https://github.com/vdavid/cmdr/commit/dea46ec))
- SMB auto-upgrade — pre-existing and newly detected SMB mounts are automatically upgraded to direct connections in the
  background, controlled by `network.directSmbConnection` setting
  ([a6ab2ca](https://github.com/vdavid/cmdr/commit/a6ab2ca))
- SMB "Connect to server" — enter hostname, IP, or `smb://` URL to connect to hosts not found by Bonjour. Persisted
  across restarts. Context menu to disconnect, forget server, or forget saved password
  ([2df24ac](https://github.com/vdavid/cmdr/commit/2df24ac))
- SMB connection status indicators — green/yellow circles in volume picker and breadcrumb show whether a share uses a
  direct (fast) or OS mount (slower) connection, with one-click upgrade option
  ([0473250](https://github.com/vdavid/cmdr/commit/0473250))
- SMB real-time progress for file transfers — pipelined I/O with throttled progress events, cancellation flows through
  to smb2 ([f530355](https://github.com/vdavid/cmdr/commit/f530355))
- SMB write operations — create, delete, rename, copy, and move all work through smb2 direct connections with full
  conflict handling ([e72c082](https://github.com/vdavid/cmdr/commit/e72c082),
  [4f030d7](https://github.com/vdavid/cmdr/commit/4f030d7))
- SMB/MTP unified change notifications — `notify_directory_changed` with incremental listing cache patches, smb2
  background watcher via `CHANGE_NOTIFY` long-poll, fixes "listing doesn't update after create/delete/rename"
  ([2d0bc98](https://github.com/vdavid/cmdr/commit/2d0bc98))
- SMB native connection warning — transfer dialog warns when using OS mount (slower, cancel/rollback may be delayed)
  ([d25de48](https://github.com/vdavid/cmdr/commit/d25de48))
- MTP auto-suppress `ptpcamerad` on macOS — no more manual steps to stop the daemon from competing for USB access,
  auto-restored on disconnect and app exit ([d161f9b](https://github.com/vdavid/cmdr/commit/d161f9b))
- MTP settings — toggle to disable MTP entirely, device connection toast with "Don't show again" option, dedicated
  settings section ([2467ece](https://github.com/vdavid/cmdr/commit/2467ece),
  [70d8d40](https://github.com/vdavid/cmdr/commit/70d8d40))
- Brief mode: show real recursive directory sizes in selection info
  ([53ee5ef](https://github.com/vdavid/cmdr/commit/53ee5ef))
- Cursor jumps to newly created directories ([eff84d1](https://github.com/vdavid/cmdr/commit/eff84d1))

### Fixed

- Copy progress: per-file counter now increments per individual file during directory copies, not per top-level item.
  Stale scan events from previous operations are rejected ([d10d9cc](https://github.com/vdavid/cmdr/commit/d10d9cc))
- SMB faster deletes — skip stat round-trip, halves round-trips for bulk deletes. Rollback now includes the in-progress
  item ([0e7f072](https://github.com/vdavid/cmdr/commit/0e7f072))
- Copy cancellation — directory tree copies now check cancellation between each file instead of looping without checking
  ([a7d401a](https://github.com/vdavid/cmdr/commit/a7d401a))
- Cross-volume copy with SmbVolume — `is_local_volume()` no longer misclassifies SmbVolume as local
  ([4a86a85](https://github.com/vdavid/cmdr/commit/4a86a85))
- SMB paths with accented characters — NFC normalization fixes `STATUS_OBJECT_PATH_NOT_FOUND` on macOS
  ([baaccc8](https://github.com/vdavid/cmdr/commit/baaccc8))
- Keychain lookup for SMB — resolves IP → hostname via mDNS so credentials are found regardless of address format
  ([b1addfd](https://github.com/vdavid/cmdr/commit/b1addfd))
- Show login form on stale Keychain credentials instead of empty share list
  ([46609f1](https://github.com/vdavid/cmdr/commit/46609f1))
- Volume-boundary navigation — prevents navigating above SMB mount root into `/Volumes`, falls back to home when
  unreachable ([d25de48](https://github.com/vdavid/cmdr/commit/d25de48))
- Stale cursor index after file ops — cursor/selection adjustment now happens before `fetchEntryUnderCursor`
  ([945093b](https://github.com/vdavid/cmdr/commit/945093b))
- Drag & drop after wry upgrade — webview class discovery now uses live instance instead of hardcoded class name
  ([a816c77](https://github.com/vdavid/cmdr/commit/a816c77))
- Stale dir sizes after copy/create — `reconcile_subtree` auto-creates new root directories, pending rescans are no
  longer abandoned ([1479108](https://github.com/vdavid/cmdr/commit/1479108))
- Scan preview race in progress dialog — subscribe to events before checking completion status
  ([5d9b91b](https://github.com/vdavid/cmdr/commit/5d9b91b))
- `dir_stats` count drift on file↔dir type changes — writer now detects and corrects the old type's count
  ([364ddf1](https://github.com/vdavid/cmdr/commit/364ddf1))
- Index entry ID race — unified all ID allocation through a shared atomic counter
  ([6e173e4](https://github.com/vdavid/cmdr/commit/6e173e4))
- MTP move not refreshing UI on Linux — bump `mtp-rs` to v0.9.1 for `ObjectInfoChanged` events
  ([5b27ead](https://github.com/vdavid/cmdr/commit/5b27ead))

### Non-app

- Replace `smb`/`smb-rpc` crates with custom `smb2` crate — cleaner API, proper error types, no NDR debug-format parsing
  hacks ([2d7904f](https://github.com/vdavid/cmdr/commit/2d7904f))
- CI: upgrade actions to Node.js 24 ([e5820bb](https://github.com/vdavid/cmdr/commit/e5820bb))
- Testing: fix multiple E2E flakes — MTP cache staleness, theme race, hidden files toggle, Docker shell quoting
  ([5009971](https://github.com/vdavid/cmdr/commit/5009971), [52faf43](https://github.com/vdavid/cmdr/commit/52faf43))
- Suppress noisy `tao` and indexing dev logs ([21b041b](https://github.com/vdavid/cmdr/commit/21b041b),
  [9bf0e00](https://github.com/vdavid/cmdr/commit/9bf0e00))

## [0.10.0] - 2026-04-08

### Added

- Copy rollback is now visible — progress bars count backwards from the cancellation point, rollback button shows
  "Rolling back...", Cancel stays active to stop the rollback ([0ac5d0](https://github.com/vdavid/cmdr/commit/0ac5d0))
- Dual progress bars in transfer dialogs — size-based and file-count-based, hidden during scanning phase
  ([ced9d2](https://github.com/vdavid/cmdr/commit/ced9d2))
- MCP: `cmdr://settings` resource and `set_setting` tool — inspect and change all settings without opening the Settings
  window ([c71115](https://github.com/vdavid/cmdr/commit/c71115))
- MCP: `move_cursor` now awaits frontend confirmation, fixing race where `copy` fires before cursor has moved
  ([634125](https://github.com/vdavid/cmdr/commit/634125))

### Fixed

- MTP: move conflicts no longer silently overwrite — both cross-volume and same-volume moves now show the conflict
  dialog with Skip/Overwrite options, same as copy ([27f2ff](https://github.com/vdavid/cmdr/commit/27f2ff))
- MTP: fix watcher missing external file changes — listing cache key mismatch made every invalidation a no-op, masked on
  macOS by the 5s cache TTL but visible on Linux where inotify fires instantly
  ([266026](https://github.com/vdavid/cmdr/commit/266026))
- MTP: fix event debouncer permanently dropping events — suppressed events in the 500ms window are now scheduled for a
  trailing emit ([21b3bc](https://github.com/vdavid/cmdr/commit/21b3bc))
- MTP: fix pane falling back to local root after copy — `refresh_listing` was calling `std::fs` on MTP paths, emitting a
  spurious `directory-deleted` event ([9deba7](https://github.com/vdavid/cmdr/commit/9deba7))
- MTP: fix volumes missing from copy/move dialog, fix destination volume dropdown not updating on change
  ([cd6603](https://github.com/vdavid/cmdr/commit/cd6603))
- MTP: fix event loop lock contention — clone `MtpDevice` for event polling instead of holding mutex during
  `next_event()`, unblocking copy/move/scan operations ([0461e3](https://github.com/vdavid/cmdr/commit/0461e3),
  [547a41](https://github.com/vdavid/cmdr/commit/547a41))
- MTP: fix scan preview showing 0/0/0 in confirmation dialog, reduce USB round-trips for conflict checks
  ([4e1efa](https://github.com/vdavid/cmdr/commit/4e1efa))
- MTP: fix rename conflicts not showing dialog on non-local volumes, fix paste guard checking clipboard before MTP
  rejection ([25f2b2](https://github.com/vdavid/cmdr/commit/25f2b2))
- Copy: fix "Cancel" (keep partial files) triggering unintended rollback — `onDestroy` race condition overwrote the
  user's choice ([3042f2](https://github.com/vdavid/cmdr/commit/3042f2))
- Copy: fix cancellation hanging 30+ seconds on network mounts — use chunked copy instead of `copyfile(3)` for all
  non-APFS-clone copies ([816e9e](https://github.com/vdavid/cmdr/commit/816e9e))
- Fix UI blocking on network filesystem operations — move validation into `spawn_blocking`, emit `write-error` for
  handler errors ([bed59d](https://github.com/vdavid/cmdr/commit/bed59d))
- Indexing: fix replay progress showing "Scanning..." instead of the replay overlay with progress bar and ETA
  ([32c053](https://github.com/vdavid/cmdr/commit/32c053))
- Volume selector: push-based model replaces polling, fix race conditions on mount/unmount
  ([b09665](https://github.com/vdavid/cmdr/commit/b09665))
- Volume path resolution via `statfs` — resolves in <1ms regardless of network mount health, handles APFS firmlinks
  ([5a1f78](https://github.com/vdavid/cmdr/commit/5a1f78))
- Harden unsafe Rust code — checked main thread markers, scoped `Send` impls, `SAFETY` comments on `transmute` calls
  ([541804](https://github.com/vdavid/cmdr/commit/541804))

### Improved

- Typed write operation errors replace string parsing — 9 specific variants (`DeviceDisconnected`, `ReadOnlyDevice`,
  `FileLocked`, etc.) instead of `IoError(String)` catch-all ([c10e06](https://github.com/vdavid/cmdr/commit/c10e06))
- Typed volume errors — MTP errors stop being erased into `IoError(String)` and guessed back via string matching
  ([8f2296](https://github.com/vdavid/cmdr/commit/8f2296))
- MTP: unified backend move — frontend no longer orchestrates three-stage MTP moves, backend handles strategy
  ([547a41](https://github.com/vdavid/cmdr/commit/547a41))
- Demote noisy per-file copy/move/MTP logs from INFO to DEBUG, add `level_for` filters for third-party crates
  ([357fef](https://github.com/vdavid/cmdr/commit/357fef))

### Non-app

- Accessibility: fix all WCAG violations found by axe-core — proper ARIA roles, focus indicators, color contrast, screen
  reader landmarks ([d29a7c](https://github.com/vdavid/cmdr/commit/d29a7c),
  [438046](https://github.com/vdavid/cmdr/commit/438046), [6e6230](https://github.com/vdavid/cmdr/commit/6e6230))
- E2E: port all tests from WebDriverIO to Playwright, add 80+ new tests covering MTP operations, SMB, file conflicts,
  accessibility, and indexing
- E2E: replace all test sleeps with event-driven waits ([3b5565](https://github.com/vdavid/cmdr/commit/3b5565))
- Tooling: replace Prettier with oxfmt (10–20x faster) ([995f8c](https://github.com/vdavid/cmdr/commit/995f8c))
- Tooling: auto-invalidate Docker `node_modules` on lockfile change
  ([ac4e26](https://github.com/vdavid/cmdr/commit/ac4e26))
- Refactor: split indexing module (1951 lines → focused files), extract shared `compute_bottom_up()`, unify
  `name_folded` across platforms ([390864](https://github.com/vdavid/cmdr/commit/390864))
- Website: light/dark theme with toggle, features page, OG images, blog Like buttons
- Dashboard: color-coded charts, GitHub star tracking, improved error reporting

## [0.9.1] - 2026-03-24

### Fixed

- Fix orphaned llama-server processes — rapid AI provider switching (Local → OpenAI → Local) could leave `llama-server`
  running after app quit. Spawn + PID tracking now happen in a single lock, plus `pgrep`-based cleanup on startup
  ([b3382e](https://github.com/vdavid/cmdr/commit/b3382e))
- Fix vendor-specific MTP device detection (Kindle, USB class `0xFF` devices) via `mtp-rs` 0.4.1 upgrade, also fixes
  indefinite event polling blocking MTP operations on idle devices
  ([1a170d](https://github.com/vdavid/cmdr/commit/1a170d))

### Non-app

- API server: migrate telemetry from Analytics Engine to D1, add crash email notifications via Resend, add admin
  endpoints for downloads/active-users/crashes, rename `license-server` → `api-server`
  ([7dc0da](https://github.com/vdavid/cmdr/commit/7dc0da))
- Refactor: split `search.rs` (2361 lines) and `SearchDialog.svelte` (1552 lines) into focused modules
  ([c17c21](https://github.com/vdavid/cmdr/commit/c17c21))
- Refactor: deduplicate repeated code patterns across Rust, Svelte, TypeScript, and Go
  ([52afe3](https://github.com/vdavid/cmdr/commit/52afe3))
- Upgrade 9 Rust dependencies — `reqwest` 0.13, `rusqlite` 0.39, `notify-debouncer-full` 0.7, and more
  ([929556](https://github.com/vdavid/cmdr/commit/929556))
- Tooling: skip `pnpm install` when lockfile unchanged, saving ~20s per run
  ([8d2b39](https://github.com/vdavid/cmdr/commit/8d2b39))
- Blog: add Kindle support article ([5c9d5b](https://github.com/vdavid/cmdr/commit/5c9d5b))

## [0.9.0] - 2026-03-23

### Added

- Add whole-drive file search (⌘F) — in-memory index with rayon parallel scan, glob/regex patterns, size/date filters,
  scope filtering, keyboard-navigable dialog, AI mode via configured LLM, two-pass AI search with preflight refinement,
  case sensitivity toggle, system folder exclusion, MCP `search` and `ai_search` tools
  ([058136](https://github.com/vdavid/cmdr/commit/058136), [15110c](https://github.com/vdavid/cmdr/commit/15110c),
  [8c3546](https://github.com/vdavid/cmdr/commit/8c3546), [cf5827](https://github.com/vdavid/cmdr/commit/cf5827),
  [415db3](https://github.com/vdavid/cmdr/commit/415db3), [21d32e](https://github.com/vdavid/cmdr/commit/21d32e),
  [26d682](https://github.com/vdavid/cmdr/commit/26d682))
- Add opt-in crash reporting — panic hook + signal handler write crash files, dialog lets users inspect and send on next
  launch, crash loop protection, no PII ([016ee3](https://github.com/vdavid/cmdr/commit/016ee3),
  [be29af](https://github.com/vdavid/cmdr/commit/be29af))
- Add Shift+F4: create new file and open in default editor, Total Commander style
  ([da8ca9](https://github.com/vdavid/cmdr/commit/da8ca9))
- Add smart size display — store both logical and physical sizes, show `min(logical, physical)` by default with setting
  to switch, dual-size tooltips with colored byte triads, hardlink dedup via inode tracking, size mismatch warning icon,
  hourglass icon for stale sizes ([1d666a](https://github.com/vdavid/cmdr/commit/1d666a),
  [b302d0](https://github.com/vdavid/cmdr/commit/b302d0), [065820](https://github.com/vdavid/cmdr/commit/065820),
  [1d588f](https://github.com/vdavid/cmdr/commit/1d588f), [a93a8b](https://github.com/vdavid/cmdr/commit/a93a8b),
  [9c450c](https://github.com/vdavid/cmdr/commit/9c450c))
- Add Ext column in Full mode — sortable, between Name and Size ([e834b4](https://github.com/vdavid/cmdr/commit/e834b4))
- Add replay progress overlay — shows "Updating index..." with progress bar and ETA during cold-start replay
  ([f166b0](https://github.com/vdavid/cmdr/commit/f166b0))
- MTP: show live disk space in volume breadcrumb dropdown and status bar
  ([b155f1](https://github.com/vdavid/cmdr/commit/b155f1), [c4cc26](https://github.com/vdavid/cmdr/commit/c4cc26))
- MTP: show loading progress when opening large folders ([77ebaa](https://github.com/vdavid/cmdr/commit/77ebaa))
- Add missing focus indicators on search and command palette inputs
  ([179221](https://github.com/vdavid/cmdr/commit/179221))
- Selection summary now includes directory sizes ([392819](https://github.com/vdavid/cmdr/commit/392819))
- MCP: show directory sizes in state resource ([9cb775](https://github.com/vdavid/cmdr/commit/9cb775))

### Fixed

- Fix macOS multi-GB memory leak — add `autoreleasepool` wrappers around all ObjC API calls on background threads, 50M
  retained objects / 5 GB after 20h of runtime ([777f9e](https://github.com/vdavid/cmdr/commit/777f9e))
- Fix stack overflow crash in sync status — use dedicated OS threads with 8 MB stacks instead of rayon for NSURL/XPC
  calls ([fa28cd](https://github.com/vdavid/cmdr/commit/fa28cd))
- Fix size overcounting — hardlink dedup via inode column, cloud-only files no longer counted as local, smart size mode
  for dataless files ([fe5eff](https://github.com/vdavid/cmdr/commit/fe5eff))
- Fix file watcher: instant updates in large dirs via incremental stat-and-compare instead of full re-read, synthetic
  diffs for mkdir ([df558e](https://github.com/vdavid/cmdr/commit/df558e))
- Fix selection clearing after file operations — clear on source pane after move/copy/delete/trash, gradual deselection
  per source item ([538ec5](https://github.com/vdavid/cmdr/commit/538ec5))
- Fix selection indices drifting after external file changes — pure index adjustment on structural diffs
  ([453ec0](https://github.com/vdavid/cmdr/commit/453ec0))
- Fix cursor lost after deleting all files ([17808d](https://github.com/vdavid/cmdr/commit/17808d))
- Fix stale dir sizes on rename — writer now emits notifications after both delete+insert commit
  ([10213d](https://github.com/vdavid/cmdr/commit/10213d))
- Fix indexing won't start on fresh DB — `scanning` flag moved to correct path
  ([a61376](https://github.com/vdavid/cmdr/commit/a61376))
- Fix "Scanning..." stuck after replay — clear scanning in replay-complete handler
  ([4a44d7](https://github.com/vdavid/cmdr/commit/4a44d7), [fb796e](https://github.com/vdavid/cmdr/commit/fb796e))
- Fix verifier + replay transaction conflict — use named savepoints instead of nested transactions
  ([72ca9f](https://github.com/vdavid/cmdr/commit/72ca9f))
- Fix MTP browsing panic and show device name instead of storage name for single-storage devices
  ([d37b8a](https://github.com/vdavid/cmdr/commit/d37b8a))
- Fix MTP duplicate directory listing on connect ([17efe8](https://github.com/vdavid/cmdr/commit/17efe8))
- Fix MCP stale state after server crash, auto-probe port when configured port is in use
  ([0369d2](https://github.com/vdavid/cmdr/commit/0369d2), [d69f87](https://github.com/vdavid/cmdr/commit/d69f87))
- Fix OpenAI compatibility ([795a67](https://github.com/vdavid/cmdr/commit/795a67))
- Hide misleading rollback button for move operations ([fbdba5](https://github.com/vdavid/cmdr/commit/fbdba5))
- Raise replay and journal gap thresholds to reduce unnecessary full rescans
  ([377919](https://github.com/vdavid/cmdr/commit/377919), [af2bf7](https://github.com/vdavid/cmdr/commit/af2bf7))

### Non-app

- Analytics dashboard: full-stack metrics view with 6 data sources, rich download timelines, agent-readable report
  ([b4f740](https://github.com/vdavid/cmdr/commit/b4f740), [0766c4](https://github.com/vdavid/cmdr/commit/0766c4),
  [b97028](https://github.com/vdavid/cmdr/commit/b97028))
- Tooling: enforce CSS design tokens via Stylelint — spacing, colors, font sizes, border radius, z-index
  ([50f2b4](https://github.com/vdavid/cmdr/commit/50f2b4), [e3259b](https://github.com/vdavid/cmdr/commit/e3259b),
  [36b340](https://github.com/vdavid/cmdr/commit/36b340))
- Testing: remove desktop smoke tests (covered by Vitest + Linux E2E), speed up store tests by ~20s
  ([c6210a](https://github.com/vdavid/cmdr/commit/c6210a), [dab071](https://github.com/vdavid/cmdr/commit/dab071))
- Refactors: reduce structural code duplication across write ops, listing, events, and search dialog
  ([33ec2f](https://github.com/vdavid/cmdr/commit/33ec2f))
- Website: add story + testimonials sections, landing page polish, fix Docker healthcheck, fix Remark42 CSP
  ([d5a7f4](https://github.com/vdavid/cmdr/commit/d5a7f4), [51acd8](https://github.com/vdavid/cmdr/commit/51acd8),
  [424a80](https://github.com/vdavid/cmdr/commit/424a80), [dd5e34](https://github.com/vdavid/cmdr/commit/dd5e34))
- MTP: upgrade mtp-rs to v0.2.0 ([634255](https://github.com/vdavid/cmdr/commit/634255))

## [0.8.2] - 2026-03-15

### Fixed

- Fix crash on launch after auto-update — `fs::copy` overwrote files in-place keeping the same inode, causing macOS
  kernel code signing cache to SIGKILL the app. Now writes to a temp file then `rename()` for a fresh inode
  ([dec8457](https://github.com/vdavid/cmdr/commit/dec8457))
- Fix indexing: per-navigation verifier catches index drift via background readdir diffs with 30s debounce, excluded
  system paths (`/System`, `/dev`) no longer inserted as empty stubs, unified exclusion checks across
  scanner/reconciler/verifier ([7afe71b](https://github.com/vdavid/cmdr/commit/7afe71b),
  [9434ee6](https://github.com/vdavid/cmdr/commit/9434ee6))
- Fix dir size display during indexing — show "Scanning..." during aggregation phase, refresh panes on
  aggregation-complete instead of scan-complete ([a6a123d](https://github.com/vdavid/cmdr/commit/a6a123d))
- Fix navigation latency — fire-and-forget verification (no mutex block), parallelize 6 sequential `listen()` calls,
  remove redundant index enrichment from `get_file_range` ([8f3ce55](https://github.com/vdavid/cmdr/commit/8f3ce55))
- Fix indexing performance: replace composite index with integer-only (25 min → seconds for 5.1M entries), add
  `name_folded` column for O(log n) path resolution, deduplicate replay events (99% reduction in high-churn scenarios)
  ([b94b611](https://github.com/vdavid/cmdr/commit/b94b611), [7ac477b](https://github.com/vdavid/cmdr/commit/7ac477b),
  [4fe5bb4](https://github.com/vdavid/cmdr/commit/4fe5bb4))

### Non-app

- Tooling: separate dev and prod log dirs, fix Linux Rust test output capture, fix smoke test timeout
  ([8429123](https://github.com/vdavid/cmdr/commit/8429123), [2181ad5](https://github.com/vdavid/cmdr/commit/2181ad5),
  [de5236b](https://github.com/vdavid/cmdr/commit/de5236b))
- Docs: improve agent instructions ([71f365b](https://github.com/vdavid/cmdr/commit/71f365b))

## [0.8.1] - 2026-03-14

### Fixed

- Fix indexing: lock-free dir stats reads (bypass `INDEXING` mutex), remove redundant `PathResolver` LRU cache with
  latent staleness bug, remove broken micro-scans, fix "DB is locked" in post-scan reconciler, fix overlay race during
  index rebuild, fix lost scan metadata causing full rescan on every restart, fix dir→file replacement leaving orphaned
  children ([50bd4fa](https://github.com/vdavid/cmdr/commit/50bd4fa),
  [44abfd1](https://github.com/vdavid/cmdr/commit/44abfd1), [7319c5c](https://github.com/vdavid/cmdr/commit/7319c5c),
  [26785fc](https://github.com/vdavid/cmdr/commit/26785fc), [795e48b](https://github.com/vdavid/cmdr/commit/795e48b),
  [424eedb](https://github.com/vdavid/cmdr/commit/424eedb), [dbccec1](https://github.com/vdavid/cmdr/commit/dbccec1),
  [8f87a4f](https://github.com/vdavid/cmdr/commit/8f87a4f))
- Fix traffic light position in production builds ([7551df2](https://github.com/vdavid/cmdr/commit/7551df2))

### Non-app

- Indexing: add concurrency stress tests, event loop tests, and reconciler tests
  ([3ad3adc](https://github.com/vdavid/cmdr/commit/3ad3adc), [8a084cd](https://github.com/vdavid/cmdr/commit/8a084cd),
  [dbccec1](https://github.com/vdavid/cmdr/commit/dbccec1))
- Docs: `ReadPool` thread safety correction, release gotchas from v0.8.0
  ([a6b5c0a](https://github.com/vdavid/cmdr/commit/a6b5c0a), [4aaa53f](https://github.com/vdavid/cmdr/commit/4aaa53f))

## [0.8.0] - 2026-03-13

### Added

- Add custom macOS updater that preserves Full Disk Access permissions across updates — syncs files into existing `.app`
  bundle instead of replacing it, with privilege escalation when needed
  ([190a637](https://github.com/vdavid/cmdr/commit/190a637))
- Add MTP delete, rename, and move operations with full progress, cancellation, and dry-run support
  ([812ad07](https://github.com/vdavid/cmdr/commit/812ad07))
- Add breadcrumb improvements: path displays "/" prefix, abbreviates home directory to "~"
  ([44b7105](https://github.com/vdavid/cmdr/commit/44b7105))
- Add auto-rescan on FSEvents channel overflow with user notification toast
  ([ca7cece](https://github.com/vdavid/cmdr/commit/ca7cece))
- Add index debug dashboard with live DB stats, watcher status, event rate sparkline, and `MustScanSubDirs` log
  ([7510ec3](https://github.com/vdavid/cmdr/commit/7510ec3))

### Fixed

- Fix indexing: interrupt-safe reconciler replaces destructive `MustScanSubDirs` handling, stop micro-scans after
  cold-start replay, faster bulk inserts by dropping/recreating index, fix false FSEvents deletions, fix missing dir
  sizes after replay, eliminate enrichment lock contention, periodic DB vacuum
  ([31df59e](https://github.com/vdavid/cmdr/commit/31df59e), [981b311](https://github.com/vdavid/cmdr/commit/981b311),
  [da74290](https://github.com/vdavid/cmdr/commit/da74290), [f0c225f](https://github.com/vdavid/cmdr/commit/f0c225f),
  [bf0b47f](https://github.com/vdavid/cmdr/commit/bf0b47f), [d125a24](https://github.com/vdavid/cmdr/commit/d125a24),
  [67684bb](https://github.com/vdavid/cmdr/commit/67684bb))
- Fix drag swizzle failing on wry 0.54+ — moved install to `RunEvent::Ready` after webview creation
  ([2680bae](https://github.com/vdavid/cmdr/commit/2680bae))
- Fix MCP live start/stop UX: query backend state as ground truth, serialize operations, auto-check port availability,
  unified status messages ([f4c107a](https://github.com/vdavid/cmdr/commit/f4c107a))
- Fix MCP server not stopping on app quit ([61fe290](https://github.com/vdavid/cmdr/commit/61fe290))
- Fix traffic light position in production builds ([b74ed39](https://github.com/vdavid/cmdr/commit/b74ed39))
- Fix scan overlay showing stale state — refresh UI after full scan completes
  ([218bcb9](https://github.com/vdavid/cmdr/commit/218bcb9))

### Non-app

- Vendor `cmdr-fsevent-stream` fork into monorepo as workspace crate
  ([8b937a6](https://github.com/vdavid/cmdr/commit/8b937a6))
- Website: fix two FOUC flickers on page load (light mode flash, newsletter icon flash)
  ([8c21ac7](https://github.com/vdavid/cmdr/commit/8c21ac7))
- Tooling: self-hosted macOS GitHub Actions runner ([665f63a](https://github.com/vdavid/cmdr/commit/665f63a)), index DB
  query tool ([37f1062](https://github.com/vdavid/cmdr/commit/37f1062)), extract website deploy workflow
  ([5744636](https://github.com/vdavid/cmdr/commit/5744636)), trim Linux test output
  ([b9d0ef2](https://github.com/vdavid/cmdr/commit/b9d0ef2)), fix release script
  ([190bfe9](https://github.com/vdavid/cmdr/commit/190bfe9), [233c8dd](https://github.com/vdavid/cmdr/commit/233c8dd))
- Refactors: split indexing `mod.rs` into `enrichment.rs`, `event_loop.rs`, `events.rs`
  ([bb7d57f](https://github.com/vdavid/cmdr/commit/bb7d57f))
- Dev: pink title bar to distinguish dev from prod ([d2c9ae4](https://github.com/vdavid/cmdr/commit/d2c9ae4))

## [0.7.1] - 2026-03-12

### Fixed

- Fix scan overlay stuck at 100% after directory size aggregation
  ([2842e92](https://github.com/vdavid/cmdr/commit/2842e92))

## [0.7.0] - 2026-03-12

### Added

- Add AI settings with three providers (off / cloud API / local LLM), 15 cloud presets with per-provider key storage,
  connection check, model combobox, RAM gauge, and context size control
  ([b41365b](https://github.com/vdavid/cmdr/commit/b41365b), [abfc248](https://github.com/vdavid/cmdr/commit/abfc248),
  [423e669](https://github.com/vdavid/cmdr/commit/423e669))
- Add live MCP server start/stop in Settings — no app restart needed
  ([e0c55e7](https://github.com/vdavid/cmdr/commit/e0c55e7))
- Add stale index detection with user notification toast and automatic rescan
  ([b590a54](https://github.com/vdavid/cmdr/commit/b590a54))
- Add device tracking for license abuse detection with fair use terms in ToS
  ([cf4f913](https://github.com/vdavid/cmdr/commit/cf4f913))
- Add license section to Settings with status display, action buttons, and dynamic labels across the app
  ([39cf7b4](https://github.com/vdavid/cmdr/commit/39cf7b4))
- Improve app icon for macOS Sequoia ([cc80d28](https://github.com/vdavid/cmdr/commit/cc80d28))

### Changed

- Remove supporter license tier — legacy keys gracefully map to Personal
  ([c0a63f5](https://github.com/vdavid/cmdr/commit/c0a63f5))
- Split Settings UI horizontally 50-50% ([9493f88](https://github.com/vdavid/cmdr/commit/9493f88))
- Rename settings file from `settings-v2.json` to `settings.json`
  ([d987cc8](https://github.com/vdavid/cmdr/commit/d987cc8))

### Fixed

- Fix startup panic from `blocking_lock` in async context ([f9855ca](https://github.com/vdavid/cmdr/commit/f9855ca))
- Fix SQLite write pragmas running on read-only connections, causing panic in subtree scans
  ([a53a275](https://github.com/vdavid/cmdr/commit/a53a275))
- Fix llama-server not stopping on app quit, keeping stale PIDs alive, and using excessive memory (256k → 4k default
  context) ([eae70f1](https://github.com/vdavid/cmdr/commit/eae70f1),
  [ffcbc81](https://github.com/vdavid/cmdr/commit/ffcbc81), [e45c742](https://github.com/vdavid/cmdr/commit/e45c742))
- Fix Settings UI freezing for ~5s when stopping AI server — instant SIGKILL for stateless llama-server
  ([2af7ee8](https://github.com/vdavid/cmdr/commit/2af7ee8))
- Fix dev and prod app data clashing on same machine — dev now uses separate data directory and MCP port
  ([b8b058a](https://github.com/vdavid/cmdr/commit/b8b058a))
- Fix fallback path resolution falling to `/` instead of `~` ([8d7c644](https://github.com/vdavid/cmdr/commit/8d7c644))
- Fix indexing: 100x faster aggregation via in-memory accumulation, DB auto vacuum, truncate before full scan, live
  index size in Settings ([47a2e8e](https://github.com/vdavid/cmdr/commit/47a2e8e),
  [cad1af5](https://github.com/vdavid/cmdr/commit/cad1af5), [aff2046](https://github.com/vdavid/cmdr/commit/aff2046),
  [96323e9](https://github.com/vdavid/cmdr/commit/96323e9))
- Fix FSEvents storms causing high memory pressure — mimalloc allocator, 1s dedup window, reduced SQLite cache and
  channel buffers ([207ddee](https://github.com/vdavid/cmdr/commit/207ddee))

### Non-app

- Docs: replace 19 ADRs with colocated Decision/Why entries in 11 CLAUDE.md files, slim down AGENTS.md from 245 to 93
  lines, add `@wrap-up` and `@plan` commands ([ccf5cc7](https://github.com/vdavid/cmdr/commit/ccf5cc7),
  [d297a1a](https://github.com/vdavid/cmdr/commit/d297a1a), [0595796](https://github.com/vdavid/cmdr/commit/0595796))
- Website: show version + file size on all download buttons, fix Intel/Apple detection flicker, fix a11y warning, fix
  Umami script collision ([bd17056](https://github.com/vdavid/cmdr/commit/bd17056),
  [ec35b1f](https://github.com/vdavid/cmdr/commit/ec35b1f), [55c950e](https://github.com/vdavid/cmdr/commit/55c950e),
  [0ad03f4](https://github.com/vdavid/cmdr/commit/0ad03f4))
- Tooling: add html-validate and circular dep checks, pass kill signals in checker script, remove pnpm audit check
  ([3dbd5af](https://github.com/vdavid/cmdr/commit/3dbd5af), [4bead2b](https://github.com/vdavid/cmdr/commit/4bead2b),
  [ce3eae1](https://github.com/vdavid/cmdr/commit/ce3eae1), [2c588bf](https://github.com/vdavid/cmdr/commit/2c588bf))
- Refactors: extract volume grouping, menu platform code, viewer scroll/search; eliminate all circular deps
  ([7740fbc](https://github.com/vdavid/cmdr/commit/7740fbc), [8522e71](https://github.com/vdavid/cmdr/commit/8522e71),
  [e16bd91](https://github.com/vdavid/cmdr/commit/e16bd91), [7ed1cea](https://github.com/vdavid/cmdr/commit/7ed1cea))
- Add missing tests across multiple modules ([b53ce59](https://github.com/vdavid/cmdr/commit/b53ce59))

## [0.6.1] - 2026-03-10

### Added

- Add top menu icons ([1a2621a](https://github.com/vdavid/cmdr/commit/1a2621a))
- Add View, Copy, Move, New folder, and Delete actions to context menu
  ([a966f17](https://github.com/vdavid/cmdr/commit/a966f17))

### Fixed

- Fix OOM crash from unbounded indexing buffers — toggling Full Disk Access could replay millions of FSEvents with zero
  backpressure, consuming 500+ GB RAM. All buffers are now bounded (~350 MB peak), with a memory watchdog that stops
  indexing at 16 GB ([f1501ec](https://github.com/vdavid/cmdr/commit/f1501ec))

### Non-app

- Website: add llms.txt, Schema.org JSON-LD, and auto-generated sitemap for agent accessibility
  ([ba64c36](https://github.com/vdavid/cmdr/commit/ba64c36))
- Website: update roadmap ([5197120](https://github.com/vdavid/cmdr/commit/5197120))
- CI: simplify release pipeline, download sigs directly from release, generate `latest.json` with `jq`, validate all 3
  sigs before proceeding ([d3095cb](https://github.com/vdavid/cmdr/commit/d3095cb),
  [5b82cd0](https://github.com/vdavid/cmdr/commit/5b82cd0))
- CI: fix Backspace E2E test on WebKitGTK, fix CI failures, fix 3 flaky tests
  ([7c22951](https://github.com/vdavid/cmdr/commit/7c22951), [79f593c](https://github.com/vdavid/cmdr/commit/79f593c),
  [8f4ea82](https://github.com/vdavid/cmdr/commit/8f4ea82))
- Docs: add troubleshooting section to releasing guide ([1768b29](https://github.com/vdavid/cmdr/commit/1768b29))

## [0.6.0] - 2026-03-08

### Added

- Add Linux support (alpha): volumes via /proc/mounts, file ops with reflink support, trash via FreeDesktop spec,
  inotify file watching, MTP ungated, SMB via mDNS + smbclient fallback, GVFS-mounted shares as volumes, native file
  icons via freedesktop-icons, accent color via XDG Desktop Portal, encrypted credential fallback when no system
  keyring, distro-specific install hints, USB permission handling
  ([b6e80f6](https://github.com/vdavid/cmdr/commit/b6e80f6), [20be0c3](https://github.com/vdavid/cmdr/commit/20be0c3),
  [9c51fa9](https://github.com/vdavid/cmdr/commit/9c51fa9), [64e41f9](https://github.com/vdavid/cmdr/commit/64e41f9),
  [40cc1a9](https://github.com/vdavid/cmdr/commit/40cc1a9), [c3ad1ed](https://github.com/vdavid/cmdr/commit/c3ad1ed),
  [d40ea25](https://github.com/vdavid/cmdr/commit/d40ea25), [60063ec](https://github.com/vdavid/cmdr/commit/60063ec),
  [e65d993](https://github.com/vdavid/cmdr/commit/e65d993), [22e2ea7](https://github.com/vdavid/cmdr/commit/22e2ea7),
  [afe2609](https://github.com/vdavid/cmdr/commit/afe2609), [4bbcbb0](https://github.com/vdavid/cmdr/commit/4bbcbb0),
  [48af543](https://github.com/vdavid/cmdr/commit/48af543))
- Add per-pane tab support: ⌘T/⌘W, ⌃Tab cycling, pin/unpin, context menu, persistence with migration, per-tab sort
  ([791a29a](https://github.com/vdavid/cmdr/commit/791a29a))
- Add delete/trash feature (F8): trash by default, ⇧F8 for permanent delete, confirmation dialog with scan preview,
  batch progress with cancellation, volume trash support detection
  ([e3560a3](https://github.com/vdavid/cmdr/commit/e3560a3))
- Add clipboard for files: ⌘C/⌘V/⌘X with Finder interop, ⌥⌘V for "Move here", cut state tracking, text clipboard in all
  windows via NSPasteboard ([0dc2953](https://github.com/vdavid/cmdr/commit/0dc2953),
  [60baeba](https://github.com/vdavid/cmdr/commit/60baeba))
- Add toast notification system with centralized store, dedup, stacking, three levels, transient/persistent modes
  ([6c5c452](https://github.com/vdavid/cmdr/commit/6c5c452), [2329f2f](https://github.com/vdavid/cmdr/commit/2329f2f))
- Add per-pane disk space display: 2px usage bar, free-space text in status bar, mini bars in volume dropdown
  ([9b6d057](https://github.com/vdavid/cmdr/commit/9b6d057))
- Add custom tooltips with glass material effect, shortcut badges, smart positioning, accessibility support, replacing
  all native tooltips ([3c7f965](https://github.com/vdavid/cmdr/commit/3c7f965))
- Add drive indexing with integer-keyed DB schema (7.4x size reduction, 3.8 GB → 0.54 GB), LRU path cache,
  platform-aware collation, recursive CTE aggregation ([7c5d3ce](https://github.com/vdavid/cmdr/commit/7c5d3ce),
  [daee97b](https://github.com/vdavid/cmdr/commit/daee97b), [5e10fa9](https://github.com/vdavid/cmdr/commit/5e10fa9),
  [68be3ab](https://github.com/vdavid/cmdr/commit/68be3ab))
- Add IPC hardening: timeout-protect all filesystem commands, transparent timeout UI with retry/fallback for volumes,
  tabs, file ops, and viewer ([6a58278](https://github.com/vdavid/cmdr/commit/6a58278),
  [71de96e](https://github.com/vdavid/cmdr/commit/71de96e))
- Add accent color option in Settings: macOS theme or Cmdr gold, "Recolor to gold" for folder icons
  ([330e824](https://github.com/vdavid/cmdr/commit/330e824), [ef9de79](https://github.com/vdavid/cmdr/commit/ef9de79))
- Add directory sorting by size with toggle in Settings ([a7dd8ca](https://github.com/vdavid/cmdr/commit/a7dd8ca))
- Add "Forget saved password" UI for SMB network shares ([7d751d5](https://github.com/vdavid/cmdr/commit/7d751d5))
- Add path validation in copy/move and mkdir dialogs with platform-correct limits
  ([6b295ec](https://github.com/vdavid/cmdr/commit/6b295ec))
- Add centralized keyboard shortcut dispatch with runtime custom bindings
  ([e40bcc2](https://github.com/vdavid/cmdr/commit/e40bcc2))
- Add macOS entitlements and TCC usage descriptions for proper permission prompts
  ([ff0c27e](https://github.com/vdavid/cmdr/commit/ff0c27e))
- Add Apple code signing, notarization, and arch-specific downloads (aarch64, x86_64, universal)
  ([b03f91e](https://github.com/vdavid/cmdr/commit/b03f91e), [944085f](https://github.com/vdavid/cmdr/commit/944085f))
- Add licensing UI improvements: verify/commit split, typed errors, short code in signed payload, Paddle live setup
  ([0abc704](https://github.com/vdavid/cmdr/commit/0abc704), [1f2308b](https://github.com/vdavid/cmdr/commit/1f2308b))

### Fixed

- Fix file viewer: search progress bar with spinner and stop button, incremental match delivery, 10k match cap,
  byte-seek navigation, loading very long files ([9c0a3c3](https://github.com/vdavid/cmdr/commit/9c0a3c3),
  [a3b9d0e](https://github.com/vdavid/cmdr/commit/a3b9d0e), [31cf5fd](https://github.com/vdavid/cmdr/commit/31cf5fd),
  [d15ecde](https://github.com/vdavid/cmdr/commit/d15ecde), [86ef2a5](https://github.com/vdavid/cmdr/commit/86ef2a5),
  [0fcdb13](https://github.com/vdavid/cmdr/commit/0fcdb13), [8b57bbe](https://github.com/vdavid/cmdr/commit/8b57bbe))
- Fix 3–10s startup block from index enrichment holding the mutex
  ([267e02b](https://github.com/vdavid/cmdr/commit/267e02b))
- Fix mDNS host resolution arriving before discovery, causing SMB auth failures
  ([2dda99b](https://github.com/vdavid/cmdr/commit/2dda99b))
- Fix focus escaping panes with focus guard, removing ~50 redundant refocus calls
  ([4c9aadc](https://github.com/vdavid/cmdr/commit/4c9aadc))
- Fix clipboard shortcuts in text fields on macOS ([20f3de0](https://github.com/vdavid/cmdr/commit/20f3de0))
- Fix non-blocking navigation on slow/dead SMB shares with timeouts and optimistic UI
  ([c85c8c2](https://github.com/vdavid/cmdr/commit/c85c8c2))
- Fix copy feature: auto-rollback on panic, deadlock prevention, cancel race condition
  ([2b17ab5](https://github.com/vdavid/cmdr/commit/2b17ab5))
- Fix status bar not refreshing after file watcher diffs ([e880f9f](https://github.com/vdavid/cmdr/commit/e880f9f))
- Fix pinned tab volume change now opens new tab instead of navigating in-place
  ([ff4c8f2](https://github.com/vdavid/cmdr/commit/ff4c8f2))
- Fix cancel-loading to return to previous folder instead of home
  ([8ff2379](https://github.com/vdavid/cmdr/commit/8ff2379))
- Fix ⌘, to refocus Settings window if already open ([71b3e61](https://github.com/vdavid/cmdr/commit/71b3e61))
- Fix Settings: ⌥+key shortcuts showing "Dead" on macOS, key filter subset matching, ESC clears filter
  ([1fd540a](https://github.com/vdavid/cmdr/commit/1fd540a), [5056bb6](https://github.com/vdavid/cmdr/commit/5056bb6),
  [47050e0](https://github.com/vdavid/cmdr/commit/47050e0))
- Fix settings not initialized warning at startup ([b540fcc](https://github.com/vdavid/cmdr/commit/b540fcc))
- Fix SMB share showing 0 bytes free on network filesystems ([f791153](https://github.com/vdavid/cmdr/commit/f791153))
- Fix volumes cached to prevent timeout at startup ([024e48f](https://github.com/vdavid/cmdr/commit/024e48f))
- Fix top menu items staying enabled on non-main windows ([7572d13](https://github.com/vdavid/cmdr/commit/7572d13))
- Fix live file count during large folder loading ([7815d0f](https://github.com/vdavid/cmdr/commit/7815d0f))
- Fix window content height for production builds ([0cbd0fd](https://github.com/vdavid/cmdr/commit/0cbd0fd))
- Fix folder icons updating on OS theme change ([6b02445](https://github.com/vdavid/cmdr/commit/6b02445))
- Fix focus lost after rename cancellation ([edace18](https://github.com/vdavid/cmdr/commit/edace18))
- Fix file viewer not loading settings ([acfef93](https://github.com/vdavid/cmdr/commit/acfef93))
- Fix drive indexing: orphaned entries, missing dir sizes, background scan failures, DB transaction issues
  ([323ae86](https://github.com/vdavid/cmdr/commit/323ae86), [004f302](https://github.com/vdavid/cmdr/commit/004f302),
  [c331143](https://github.com/vdavid/cmdr/commit/c331143))
- Fix MCP protocol version mismatch warnings at startup ([2af0b90](https://github.com/vdavid/cmdr/commit/2af0b90))
- Fix arrow up/down performance in large folders ([e6f268c](https://github.com/vdavid/cmdr/commit/e6f268c))
- Fix PostHog CSP and make it cookieless ([1700d99](https://github.com/vdavid/cmdr/commit/1700d99),
  [9cea85a](https://github.com/vdavid/cmdr/commit/9cea85a))
- Fix app loading slowly due to startup optimizations: license cache, async validation
  ([3835866](https://github.com/vdavid/cmdr/commit/3835866), [87de136](https://github.com/vdavid/cmdr/commit/87de136))

### Non-app

- Overhaul native menus on macOS and Linux: build from scratch, strip macOS system-injected items, unify dispatch via
  single event, context-aware graying, full accelerator sync ([b38c552](https://github.com/vdavid/cmdr/commit/b38c552))
- Unify frontend + backend logging via tauri-plugin-log, demote noisy log levels, suppress smb/sspi noise
  ([22f4ab5](https://github.com/vdavid/cmdr/commit/22f4ab5), [dbbcc55](https://github.com/vdavid/cmdr/commit/dbbcc55),
  [1e59a56](https://github.com/vdavid/cmdr/commit/1e59a56))
- Design system: unified button styles, consistent loading states, improved text readability, redesigned network screens
  ([8dc2e33](https://github.com/vdavid/cmdr/commit/8dc2e33), [4d07ad0](https://github.com/vdavid/cmdr/commit/4d07ad0),
  [71dbe0b](https://github.com/vdavid/cmdr/commit/71dbe0b), [b5d8b28](https://github.com/vdavid/cmdr/commit/b5d8b28),
  [a018a3e](https://github.com/vdavid/cmdr/commit/a018a3e), [90e2010](https://github.com/vdavid/cmdr/commit/90e2010))
- Docs overhaul: CLAUDE.md staleness checker in CI, enriched 25 CLAUDE.md files with Decision/Why entries, cross-cutting
  patterns in architecture.md, split infrastructure.md into per-service files
  ([ff8b3be](https://github.com/vdavid/cmdr/commit/ff8b3be), [347ae9b](https://github.com/vdavid/cmdr/commit/347ae9b),
  [f961f19](https://github.com/vdavid/cmdr/commit/f961f19), [2f7bff1](https://github.com/vdavid/cmdr/commit/2f7bff1))
- Website: add blog with first post, PostHog and Umami analytics, arch-specific download buttons, Docker build check,
  newsletter improvements ([01681c1](https://github.com/vdavid/cmdr/commit/01681c1),
  [75d5228](https://github.com/vdavid/cmdr/commit/75d5228), [78de573](https://github.com/vdavid/cmdr/commit/78de573),
  [ae8f6cb](https://github.com/vdavid/cmdr/commit/ae8f6cb), [34ecc70](https://github.com/vdavid/cmdr/commit/34ecc70))
- Check runner: CSV stats logging, cfg-gate enclosing block scope detection, file length check, flag combining fix
  ([9ac4b54](https://github.com/vdavid/cmdr/commit/9ac4b54), [539db62](https://github.com/vdavid/cmdr/commit/539db62),
  [4a24562](https://github.com/vdavid/cmdr/commit/4a24562), [6fe48a9](https://github.com/vdavid/cmdr/commit/6fe48a9))
- Refactors: split DualPaneExplorer and FilePane, extract dialog state, deduplicate templates and Settings CSS, split
  tauri-commands ([337f620](https://github.com/vdavid/cmdr/commit/337f620),
  [cfae0db](https://github.com/vdavid/cmdr/commit/cfae0db), [dad8790](https://github.com/vdavid/cmdr/commit/dad8790),
  [35a4239](https://github.com/vdavid/cmdr/commit/35a4239), [ba86d87](https://github.com/vdavid/cmdr/commit/ba86d87))
- License server: download tracking via Cloudflare Analytics Engine
  ([ef0f049](https://github.com/vdavid/cmdr/commit/ef0f049))
- Add Renovate for automated dependency updates ([00880a0](https://github.com/vdavid/cmdr/commit/00880a0))
- Add macOS Playwright E2E tests and CrabNebula E2E tests ([ec900ee](https://github.com/vdavid/cmdr/commit/ec900ee),
  [a768c03](https://github.com/vdavid/cmdr/commit/a768c03))
- Infra: uptime monitoring with UptimeRobot + Pushover, hardened deploy script
  ([19baefd](https://github.com/vdavid/cmdr/commit/19baefd))
- Add cfg-gate lint check for macOS-only Rust crates ([075c1d4](https://github.com/vdavid/cmdr/commit/075c1d4))

## [0.5.0] - 2026-02-15

### Added

- Add file viewer (F3) with three-backend architecture for files of any size, virtual scrolling, search with multibyte
  support, word wrap, horizontal scrolling, and keyboard shortcuts
  ([79268a4](https://github.com/vdavid/cmdr/commit/79268a4), [9f91bce](https://github.com/vdavid/cmdr/commit/9f91bce),
  [b10002a](https://github.com/vdavid/cmdr/commit/b10002a), [2ad2521](https://github.com/vdavid/cmdr/commit/2ad2521),
  [b65c422](https://github.com/vdavid/cmdr/commit/b65c422), [43adc86](https://github.com/vdavid/cmdr/commit/43adc86))
- Add drag-and-drop into Cmdr: pane and folder-level targeting, canvas overlay with file names and icons, Alt to switch
  copy/move, smart overlay suppression for large source images
  ([1ad1493](https://github.com/vdavid/cmdr/commit/1ad1493), [6207d8e](https://github.com/vdavid/cmdr/commit/6207d8e),
  [a89f18f](https://github.com/vdavid/cmdr/commit/a89f18f), [371746b](https://github.com/vdavid/cmdr/commit/371746b),
  [a3eae1c](https://github.com/vdavid/cmdr/commit/a3eae1c), [c776eed](https://github.com/vdavid/cmdr/commit/c776eed),
  [e97d3db](https://github.com/vdavid/cmdr/commit/e97d3db))
- Add settings window (⌘,) with declarative registry, fuzzy search, persistence, keyboard shortcut customization with
  conflict detection, and cross-window sync ([db121f6](https://github.com/vdavid/cmdr/commit/db121f6),
  [418f790](https://github.com/vdavid/cmdr/commit/418f790), [8f78596](https://github.com/vdavid/cmdr/commit/8f78596),
  [218b79b](https://github.com/vdavid/cmdr/commit/218b79b), [9c39db3](https://github.com/vdavid/cmdr/commit/9c39db3),
  [4e90137](https://github.com/vdavid/cmdr/commit/4e90137))
- Add MTP (Android device) support: browsing, file operations (copy, delete, rename, new folder), USB hotplug,
  multi-storage, MTP-to-MTP transfers ([938e87c](https://github.com/vdavid/cmdr/commit/938e87c),
  [672fa6e](https://github.com/vdavid/cmdr/commit/672fa6e), [d1e9f80](https://github.com/vdavid/cmdr/commit/d1e9f80),
  [7ac1528](https://github.com/vdavid/cmdr/commit/7ac1528), [b08af36](https://github.com/vdavid/cmdr/commit/b08af36),
  [ea845a6](https://github.com/vdavid/cmdr/commit/ea845a6), [fd8dad6](https://github.com/vdavid/cmdr/commit/fd8dad6))
- Add move feature (F6) reusing the copy UI as a unified transfer abstraction
  ([682d33a](https://github.com/vdavid/cmdr/commit/682d33a), [cb9e047](https://github.com/vdavid/cmdr/commit/cb9e047))
- Add rename feature with edge-case handling ([62799c6](https://github.com/vdavid/cmdr/commit/62799c6))
- Add swap panes feature with ⌘U shortcut ([2a1b329](https://github.com/vdavid/cmdr/commit/2a1b329))
- Add local AI for folder name suggestions in New Folder dialog, optional download
  ([b9a112e](https://github.com/vdavid/cmdr/commit/b9a112e), [3dc19c0](https://github.com/vdavid/cmdr/commit/3dc19c0))
- Add chunked copy with cancellation and pause support on network drives
  ([ba5409e](https://github.com/vdavid/cmdr/commit/ba5409e))
- Add 6 copy/move safety checks: path canonicalization, writability, disk space, inode identity, name length, special
  file filtering ([9548022](https://github.com/vdavid/cmdr/commit/9548022))
- Add sync status polling so iCloud/Dropbox icons update in real time
  ([ed36158](https://github.com/vdavid/cmdr/commit/ed36158), [6296412](https://github.com/vdavid/cmdr/commit/6296412))
- Add CSP to Tauri webview for XSS protection ([68bd510](https://github.com/vdavid/cmdr/commit/68bd510))
- Add copy/move folder-into-subfolder warning with clear error message
  ([521ab5e](https://github.com/vdavid/cmdr/commit/521ab5e))

### Fixed

- Fix panes getting stale when current directory or its parents are deleted
  ([1b5ad52](https://github.com/vdavid/cmdr/commit/1b5ad52))
- Fix multi-window race conditions that could crash the app ([9a33e24](https://github.com/vdavid/cmdr/commit/9a33e24))
- Fix recovering from poisoned mutexes instead of crashing (56 lock sites)
  ([62fd685](https://github.com/vdavid/cmdr/commit/62fd685))
- Fix wrong cursor position after show/hide hidden files ([223b041](https://github.com/vdavid/cmdr/commit/223b041))
- Fix selection and cursor position breaking on sort change ([36d61d0](https://github.com/vdavid/cmdr/commit/36d61d0))
- Fix panel unresponsive after Brief/Full view change ([2b6d513](https://github.com/vdavid/cmdr/commit/2b6d513))
- Fix copy operationId capture race condition ([9b5c57c](https://github.com/vdavid/cmdr/commit/9b5c57c))
- Fix $effect listener cleanup race in FilePane ([e2c6ee1](https://github.com/vdavid/cmdr/commit/e2c6ee1))
- Fix condvar hang on unresolved conflict dialog ([2975c45](https://github.com/vdavid/cmdr/commit/2975c45))
- Fix first click on main window not changing file focus ([59c5da4](https://github.com/vdavid/cmdr/commit/59c5da4))
- Fix AppleScript injection in get_info command ([e3378c3](https://github.com/vdavid/cmdr/commit/e3378c3))
- Fix URL-encoding of SMB username in smbutil URLs ([f908a74](https://github.com/vdavid/cmdr/commit/f908a74))
- Fix mouse/keyboard interaction bug for volume picker ([8afd0de](https://github.com/vdavid/cmdr/commit/8afd0de))
- Fix drop coordinates when DevTools is docked ([a9a041f](https://github.com/vdavid/cmdr/commit/a9a041f))
- Fix MCP server always returning left pane as selected ([2f9160a](https://github.com/vdavid/cmdr/commit/2f9160a))
- Redact PII from production log statements ([fe31316](https://github.com/vdavid/cmdr/commit/fe31316))

### Non-app

- Migrate network discovery from NSNetServiceBrowser to mdns-sd: 68% code reduction, no unsafe code
  ([3d44cf1](https://github.com/vdavid/cmdr/commit/3d44cf1))
- Rewrite MCP server with fewer tools but more capabilities, auto-reconnect, and instructions field
  ([1061fad](https://github.com/vdavid/cmdr/commit/1061fad), [ede6463](https://github.com/vdavid/cmdr/commit/ede6463),
  [82345d1](https://github.com/vdavid/cmdr/commit/82345d1))
- Introduce ModalDialog component for all soft modals with drag support
  ([ffbf14a](https://github.com/vdavid/cmdr/commit/ffbf14a))
- Major refactors: split DualPaneExplorer, FilePane, volume_copy, listing/operations, connection modules
  ([04dc3de](https://github.com/vdavid/cmdr/commit/04dc3de), [e14c289](https://github.com/vdavid/cmdr/commit/e14c289),
  [2da8e6d](https://github.com/vdavid/cmdr/commit/2da8e6d), [c0bd500](https://github.com/vdavid/cmdr/commit/c0bd500),
  [707a96a](https://github.com/vdavid/cmdr/commit/707a96a))
- Security: pin GitHub Actions to commit SHAs, fix Paddle webhook timing attack, use crypto.getRandomValues for license
  codes, HTML-escape license emails, add webhook idempotency, constant-time admin auth
  ([c0d8cc3](https://github.com/vdavid/cmdr/commit/c0d8cc3), [70bc594](https://github.com/vdavid/cmdr/commit/70bc594),
  [51cd0b5](https://github.com/vdavid/cmdr/commit/51cd0b5), [bea3b2a](https://github.com/vdavid/cmdr/commit/bea3b2a),
  [9db450b](https://github.com/vdavid/cmdr/commit/9db450b), [b82f857](https://github.com/vdavid/cmdr/commit/b82f857))
- Docs overhaul: add colocated CLAUDE.md files throughout repo, architecture.md, branding guide
  ([eac9e61](https://github.com/vdavid/cmdr/commit/eac9e61), [dd91c78](https://github.com/vdavid/cmdr/commit/dd91c78))
- Website: add changelog, roadmap, newsletter signup with Listmonk + AWS SES, mobile responsiveness fixes, 512px logo
  ([643de6a](https://github.com/vdavid/cmdr/commit/643de6a), [07936d1](https://github.com/vdavid/cmdr/commit/07936d1),
  [ba4812d](https://github.com/vdavid/cmdr/commit/ba4812d), [aa661cf](https://github.com/vdavid/cmdr/commit/aa661cf))
- Add dead code check, manual CI trigger, pnpm security audit, LoC counter, summary job for branch protection
  ([9876600](https://github.com/vdavid/cmdr/commit/9876600), [3b20e66](https://github.com/vdavid/cmdr/commit/3b20e66),
  [ad22eba](https://github.com/vdavid/cmdr/commit/ad22eba))
- Tooling: extract shared Go check helpers, add VNC mode for Linux testing, fix Linux E2E environment
  ([550c353](https://github.com/vdavid/cmdr/commit/550c353), [6aa5ff7](https://github.com/vdavid/cmdr/commit/6aa5ff7),
  [fa907b6](https://github.com/vdavid/cmdr/commit/fa907b6))
- License server: add input validation, webhook idempotency, and security hardening
  ([4363a32](https://github.com/vdavid/cmdr/commit/4363a32), [9db450b](https://github.com/vdavid/cmdr/commit/9db450b),
  [7398965](https://github.com/vdavid/cmdr/commit/7398965))

## [0.4.0] - 2026-01-27

### Added

- Add file selection: Space toggles, Shift+arrows for range, Cmd+A for select all, selection info in status bar
  ([4d44cda](https://github.com/vdavid/cmdr/commit/4d44cda), [1cac4b3](https://github.com/vdavid/cmdr/commit/1cac4b3))
- Add copy feature with F5: copy dialog, destination picker with free space display, conflict handling
  ([281f45e](https://github.com/vdavid/cmdr/commit/281f45e), [fb5f027](https://github.com/vdavid/cmdr/commit/fb5f027),
  [a6d148d](https://github.com/vdavid/cmdr/commit/a6d148d), [6c661f2](https://github.com/vdavid/cmdr/commit/6c661f2))
- Add new folder feature with F7 shortcut and conflict handling
  ([80ec297](https://github.com/vdavid/cmdr/commit/80ec297))
- Add "Open in editor" feature with F4 shortcut ([7eb66ac](https://github.com/vdavid/cmdr/commit/7eb66ac))
- Add function key bar at bottom of UI for mouse-initiated actions
  ([537e040](https://github.com/vdavid/cmdr/commit/537e040))
- Add pane resizing: drag to resize between 25–75%, double-click to reset to 50%
  ([542b491](https://github.com/vdavid/cmdr/commit/542b491))
- Add multifile external drag and drop ([7426334](https://github.com/vdavid/cmdr/commit/7426334))
- Add keyboard navigation to network panes: PgUp/PgDn, Home/End, arrow keys
  ([70aa341](https://github.com/vdavid/cmdr/commit/70aa341))
- Add "Opening folder..." loading phase for network folders with distinct status messages
  ([9eb1185](https://github.com/vdavid/cmdr/commit/9eb1185))
- Add license key entry dialog with organization address and tax ID collection
  ([52480ce](https://github.com/vdavid/cmdr/commit/52480ce), [29eb6fe](https://github.com/vdavid/cmdr/commit/29eb6fe))

### Fixed

- Fix UI not updating on external file renames ([5de9346](https://github.com/vdavid/cmdr/commit/5de9346))
- Fix light mode colors ([42888c7](https://github.com/vdavid/cmdr/commit/42888c7))
- Fix cursor going out of Full view bounds ([7edcac8](https://github.com/vdavid/cmdr/commit/7edcac8))
- Fix ESC during loading navigating to wrong location ([b8c12e7](https://github.com/vdavid/cmdr/commit/b8c12e7))
- Fix focus after dragging window ([8488de6](https://github.com/vdavid/cmdr/commit/8488de6))
- Fix multiple volume selectors opening at once ([f4c4c21](https://github.com/vdavid/cmdr/commit/f4c4c21))
- Fix frontend race condition from refactor ([646c7af](https://github.com/vdavid/cmdr/commit/646c7af))

### Non-app

- Add E2E tests with tauri-driver on Linux using WebDriverIO in Docker
  ([1b0cbac](https://github.com/vdavid/cmdr/commit/1b0cbac))
- Revamp checker script: parallel execution, dependency graph, aligned output, colored durations
  ([7835b4c](https://github.com/vdavid/cmdr/commit/7835b4c))
- Add type drift detection between Rust and Svelte types ([b3ae1c3](https://github.com/vdavid/cmdr/commit/b3ae1c3))
- Add jscpd for Rust code duplication detection, CSS health checks, Go checks
  ([67e6c15](https://github.com/vdavid/cmdr/commit/67e6c15), [d177eb3](https://github.com/vdavid/cmdr/commit/d177eb3),
  [254075a](https://github.com/vdavid/cmdr/commit/254075a))
- Add Claude hooks for pre-session context and post-edit autoformat
  ([3d59dde](https://github.com/vdavid/cmdr/commit/3d59dde), [122182d](https://github.com/vdavid/cmdr/commit/122182d))
- Add LogTape logging for Svelte and debug pane for dev mode ([affa548](https://github.com/vdavid/cmdr/commit/affa548),
  [f494e15](https://github.com/vdavid/cmdr/commit/f494e15))
- Require reasoning in clippy lint exceptions ([d327cf4](https://github.com/vdavid/cmdr/commit/d327cf4))
- Website: fix hero image animation and sizing, fix broken Paddle references
  ([40faeee](https://github.com/vdavid/cmdr/commit/40faeee), [278ad4c](https://github.com/vdavid/cmdr/commit/278ad4c),
  [5eb5a52](https://github.com/vdavid/cmdr/commit/5eb5a52))
- License server: wire up Paddle checkout, fix webhook email fetching, support quantity > 1
  ([3c40929](https://github.com/vdavid/cmdr/commit/3c40929))

## [0.3.2] - 2026-01-14

### Fixed

- Fix auto-updater to download updates and restart the app after updating
  ([c0bff9a](https://github.com/vdavid/cmdr/commit/c0bff9a))

### Non-app

- Website: redesign with mustard yellow theme, view transitions, hero animation, and reduced motion support
  ([0296379](https://github.com/vdavid/cmdr/commit/0296379), [18b729f](https://github.com/vdavid/cmdr/commit/18b729f),
  [689a151](https://github.com/vdavid/cmdr/commit/689a151))
- Website: avoid aggressive caching, rearrange T&C ([8ca0539](https://github.com/vdavid/cmdr/commit/8ca0539),
  [c92dff8](https://github.com/vdavid/cmdr/commit/c92dff8))
- Tooling: turn off MCP stdio sidecar, fix Rust-Linux check, reduce CI frequency, fix latest.json formatting
  ([5dda608](https://github.com/vdavid/cmdr/commit/5dda608), [2ec3f7e](https://github.com/vdavid/cmdr/commit/2ec3f7e),
  [42d81ab](https://github.com/vdavid/cmdr/commit/42d81ab), [52980ae](https://github.com/vdavid/cmdr/commit/52980ae))
- Docs: release process and auto-updater documentation ([c7c36f6](https://github.com/vdavid/cmdr/commit/c7c36f6),
  [765f5ad](https://github.com/vdavid/cmdr/commit/765f5ad), [f3785da](https://github.com/vdavid/cmdr/commit/f3785da),
  [10e43de](https://github.com/vdavid/cmdr/commit/10e43de))

## [0.3.1] - 2026-01-14

### Added

- Add custom title bar, 4 px narrower for more content space ([33e90c8](https://github.com/vdavid/cmdr/commit/33e90c8))

### Changed

- Replace rusty icon with yellow one ([79777e3](https://github.com/vdavid/cmdr/commit/79777e3))

### Fixed

- Fix app name in task switcher: shows "Cmdr" instead of "cmdr"
  ([8117300](https://github.com/vdavid/cmdr/commit/8117300))

## [0.3.0] - 2026-01-13

### Added

- Add MCP server with file exploring tools ([f6dcf27](https://github.com/vdavid/cmdr/commit/f6dcf27))
- Add stdio MCP interface for broader client compatibility ([3b193f7](https://github.com/vdavid/cmdr/commit/3b193f7))
- Add Streamable HTTP support to MCP server ([1d0549b](https://github.com/vdavid/cmdr/commit/1d0549b))
- Stream folder contents for blazing fast experience ([1d82ec9](https://github.com/vdavid/cmdr/commit/1d82ec9))
- Add "listing complete" state showing file count ([5059e00](https://github.com/vdavid/cmdr/commit/5059e00))
- Add Linux checks to checker script ([02ab0ab](https://github.com/vdavid/cmdr/commit/02ab0ab))

### Fixed

- Fix MCP server port and tool naming ([c2ae7de](https://github.com/vdavid/cmdr/commit/c2ae7de))
- Fix race condition when loading files ([38865e6](https://github.com/vdavid/cmdr/commit/38865e6))

## [0.2.0] - 2026-01-10

Initial public release. Free forever for personal use (BSL license).

### Added

- Dual-pane file explorer with keyboard and mouse navigation ([c945f18](https://github.com/vdavid/cmdr/commit/c945f18))
- Full mode (vertical scroll with size/date columns) and Brief mode (horizontal multi-column), switchable via ⌘1/⌘2
  ([c779a6d](https://github.com/vdavid/cmdr/commit/c779a6d))
- Virtual scrolling for 100k+ files ([cf6c35d](https://github.com/vdavid/cmdr/commit/cf6c35d))
- Chunked directory loading (50k files: 350 ms to first files)
  ([869cdfb](https://github.com/vdavid/cmdr/commit/869cdfb))
- File icons from OS with caching ([b8c588e](https://github.com/vdavid/cmdr/commit/b8c588e))
- File metadata panel with size color coding and date tooltips
  ([bc3dc85](https://github.com/vdavid/cmdr/commit/bc3dc85))
- Native context menu (Open, Show in Finder, Copy path, Quick Look)
  ([7d977a1](https://github.com/vdavid/cmdr/commit/7d977a1))
- Live file watching with incremental diffs ([cf12372](https://github.com/vdavid/cmdr/commit/cf12372))
- Dropbox and iCloud sync status icons ([46f1770](https://github.com/vdavid/cmdr/commit/46f1770))
- Volume switching with keyboard navigation ([ba3e770](https://github.com/vdavid/cmdr/commit/ba3e770))
- Network drives (SMB): host discovery via Bonjour, share listing, authentication, and mounting
  ([54ee04f](https://github.com/vdavid/cmdr/commit/54ee04f))
- Sorting by name, size, date, extension with alphanumeric sort
  ([e7b7206](https://github.com/vdavid/cmdr/commit/e7b7206))
- Back/Forward navigation ([56a5bf6](https://github.com/vdavid/cmdr/commit/56a5bf6))
- Drag and drop from the app ([8e1d53b](https://github.com/vdavid/cmdr/commit/8e1d53b))
- Command palette with fuzzy search ([7b0ea13](https://github.com/vdavid/cmdr/commit/7b0ea13))
- Window state persistence (position and size remembered) ([b8d93c5](https://github.com/vdavid/cmdr/commit/b8d93c5))
- Dark mode support ([7deb986](https://github.com/vdavid/cmdr/commit/7deb986))
- Show hidden files menu item ([4af855d](https://github.com/vdavid/cmdr/commit/4af855d))
- Full disk access permission handling ([9f433d8](https://github.com/vdavid/cmdr/commit/9f433d8))
- Licensing features (validation, about screen, expiry modal) ([dc68eeb](https://github.com/vdavid/cmdr/commit/dc68eeb))
- Keyboard shortcuts: Backspace/⌘↑ (go up), ⌥↑/↓ (home/end), Fn arrows (page up/down)
  ([fc899d4](https://github.com/vdavid/cmdr/commit/fc899d4))
- getcmdr.com website ([0f9eb21](https://github.com/vdavid/cmdr/commit/0f9eb21))
- License server (Cloudflare Worker) with Ed25519-signed keys ([bff3e8a](https://github.com/vdavid/cmdr/commit/bff3e8a))

---

### Development history

<details>
<summary>Click to expand full development history</summary>

#### 2026-01-10 - Initial public release

- Add licensing features to app (validation, about screen, expiry modal)
  ([dc68eeb](https://github.com/vdavid/cmdr/commit/dc68eeb))
- Add command palette with fuzzy search ([7b0ea13](https://github.com/vdavid/cmdr/commit/7b0ea13))
- Switch to BSL license (free for individuals) ([06c49cb](https://github.com/vdavid/cmdr/commit/06c49cb))

#### 2026-01-09 - License server improvements

- Add checkout tester tool for license server ([38774fe](https://github.com/vdavid/cmdr/commit/38774fe))
- Add sandbox/live environment duality for license tests ([15b3957](https://github.com/vdavid/cmdr/commit/15b3957))
- Unify trial period to 14 days ([7e68c27](https://github.com/vdavid/cmdr/commit/7e68c27))

#### 2026-01-08 - Cmdr, website, licensing

- Rename to Cmdr ([016a3e3](https://github.com/vdavid/cmdr/commit/016a3e3))
- Restructure as monorepo with desktop app in apps/desktop ([c0e764a](https://github.com/vdavid/cmdr/commit/c0e764a))
- Add getcmdr.com website ([0f9eb21](https://github.com/vdavid/cmdr/commit/0f9eb21))
- Add license server (Cloudflare Worker) with Ed25519-signed keys
  ([bff3e8a](https://github.com/vdavid/cmdr/commit/bff3e8a))
- Add legal pages (privacy policy, terms, refund policy, pricing)
  ([4f32a29](https://github.com/vdavid/cmdr/commit/4f32a29))
- Streamline CI (website-only PRs: 22 min → 2 min) ([4894003](https://github.com/vdavid/cmdr/commit/4894003))

#### 2026-01-07 - Network fixes

- Fix network share unnecessary login prompts ([dbeebaf](https://github.com/vdavid/cmdr/commit/dbeebaf))
- Fix Back/Forward navigation across network screens ([bf462e9](https://github.com/vdavid/cmdr/commit/bf462e9))
- Sort network hosts and shares alphabetically ([9de5f2b](https://github.com/vdavid/cmdr/commit/9de5f2b))

#### 2026-01-05-06 - Network drives (SMB)

- Add network host discovery via Bonjour ([54ee04f](https://github.com/vdavid/cmdr/commit/54ee04f))
- Add SMB share listing ([693e926](https://github.com/vdavid/cmdr/commit/693e926))
- Add network share authentication ([283e5fd](https://github.com/vdavid/cmdr/commit/283e5fd))
- Add network share mounting ([308d55c](https://github.com/vdavid/cmdr/commit/308d55c))
- Add volume mount/unmount watching ([76bbf22](https://github.com/vdavid/cmdr/commit/76bbf22))

#### 2026-01-04 - Sorting

- Add sorting feature (name, size, date, extension) with alphanumeric sort
  ([e7b7206](https://github.com/vdavid/cmdr/commit/e7b7206))
- Add Stylelint for CSS quality ([a778dcc](https://github.com/vdavid/cmdr/commit/a778dcc))

#### 2026-01-02-03 - Navigation and permissions

- Add ⌘↑ shortcut to go up a folder ([848e2f1](https://github.com/vdavid/cmdr/commit/848e2f1))
- Add full disk access permission handling ([9f433d8](https://github.com/vdavid/cmdr/commit/9f433d8))
- Add Back/Forward navigation with menu items ([56a5bf6](https://github.com/vdavid/cmdr/commit/56a5bf6))
- Add keyboard navigation to volume selector ([46c3023](https://github.com/vdavid/cmdr/commit/46c3023))
- Save last directory per volume ([9886fcd](https://github.com/vdavid/cmdr/commit/9886fcd))
- Set minimum window size ([237c5a9](https://github.com/vdavid/cmdr/commit/237c5a9))
- Fix opening files ([714dc5a](https://github.com/vdavid/cmdr/commit/714dc5a))

#### 2026-01-01 - Drag and drop, volumes

- Add drag and drop FROM the app ([8e1d53b](https://github.com/vdavid/cmdr/commit/8e1d53b))
- Add volume switching feature ([ba3e770](https://github.com/vdavid/cmdr/commit/ba3e770))
- Remove Tailwind (was slowing down app startup) ([5354a48](https://github.com/vdavid/cmdr/commit/5354a48))

#### 2025-12-31 - Polish

- Add font width measuring for precise Brief mode layout ([848f68f](https://github.com/vdavid/cmdr/commit/848f68f))
- Abstract file system access for better testing ([eb9dd72](https://github.com/vdavid/cmdr/commit/eb9dd72))
- Fix Dropbox sync icon false positives ([64007f0](https://github.com/vdavid/cmdr/commit/64007f0))
- Fix file watching reliability ([aefe3e7](https://github.com/vdavid/cmdr/commit/aefe3e7))

#### 2025-12-30 - Speed optimizations

- Add keyboard shortcuts: ⌥↑/↓ for home/end, Fn arrows for page up/down
  ([6298990](https://github.com/vdavid/cmdr/commit/6298990))
- Move file cache to backend for major speed improvements ([a42eda5](https://github.com/vdavid/cmdr/commit/a42eda5))
- Optimize directory loading (phase 1 and 2) ([7efd61a](https://github.com/vdavid/cmdr/commit/7efd61a))

#### 2025-12-29 - View modes and cloud sync

- Add Full mode (vertical scroll with size/date columns) and Brief mode (horizontal multi-column)
  ([c779a6d](https://github.com/vdavid/cmdr/commit/c779a6d))
- Add Dropbox and iCloud sync status icons ([46f1770](https://github.com/vdavid/cmdr/commit/46f1770))
- Add loading screen animation ([234f0a7](https://github.com/vdavid/cmdr/commit/234f0a7))

#### 2025-12-28 - Performance and file operations

- Add chunked directory loading (50k files: 350 ms to first files)
  ([869cdfb](https://github.com/vdavid/cmdr/commit/869cdfb))
- Add file metadata panel with size color coding and date tooltips
  ([bc3dc85](https://github.com/vdavid/cmdr/commit/bc3dc85))
- Add native context menu (Open, Show in Finder, Copy path, Quick Look)
  ([7d977a1](https://github.com/vdavid/cmdr/commit/7d977a1))
- Add live file watching with incremental diffs ([cf12372](https://github.com/vdavid/cmdr/commit/cf12372))
- Add virtual scrolling for 100k+ files ([cf6c35d](https://github.com/vdavid/cmdr/commit/cf6c35d))
- Add Backspace shortcut to go up a folder ([fc899d4](https://github.com/vdavid/cmdr/commit/fc899d4))
- Scroll to last folder when navigating up ([8ccd8bd](https://github.com/vdavid/cmdr/commit/8ccd8bd))

#### 2025-12-27 - File metadata and icons

- Add file metadata display (owner, size, dates) ([d9994bc](https://github.com/vdavid/cmdr/commit/d9994bc))
- Add file icons from OS with caching ([b8c588e](https://github.com/vdavid/cmdr/commit/b8c588e))
- Add per-folder custom icons support ([210f23b](https://github.com/vdavid/cmdr/commit/210f23b))
- Add Tauri MCP server for AI tooling integration ([0a64eb3](https://github.com/vdavid/cmdr/commit/0a64eb3))
- Fix symlinked directory handling ([5a134ac](https://github.com/vdavid/cmdr/commit/5a134ac))

#### 2025-12-26 - Dual-pane explorer

- Add dual-pane file explorer with home directory listing ([c945f18](https://github.com/vdavid/cmdr/commit/c945f18))
- Add window state persistence (position and size remembered) ([b8d93c5](https://github.com/vdavid/cmdr/commit/b8d93c5))
- Add file navigation with keyboard and mouse ([20424e0](https://github.com/vdavid/cmdr/commit/20424e0))
- Add "Show hidden files" menu item ([4af855d](https://github.com/vdavid/cmdr/commit/4af855d))
- Add dark mode support ([7deb986](https://github.com/vdavid/cmdr/commit/7deb986))

#### 2025-12-25 - Project init

- Initialize Rust + Tauri 2 + Svelte 5 project ([b410bd9](https://github.com/vdavid/cmdr/commit/b410bd9))
- Add GitHub Actions workflow ([6dbf265](https://github.com/vdavid/cmdr/commit/6dbf265))

</details>
