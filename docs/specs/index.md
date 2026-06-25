# Specs index

Spec docs and task lists for Cmdr developments, indexed so each stays discoverable. See [README.md](README.md) for what
this folder is and when it gets wiped. Checked means the work shipped; unchecked means not yet, or deferred under
[`later/`](later/). The date is each spec's creation date.

## Specs

- [x] 2026-04-03 connect-to-server-plan.md - Manual "Connect to server" for non-mDNS SMB shares (shipped v0.11.0)
- [x] 2026-05-25 component-catalog-plan.md - In-app dev-only catalog of every UI primitive
- [x] 2026-05-28 downloads-watcher-plan.md - Hotkey and toast to jump to the latest download
- [x] 2026-05-28 viewer-tail-encoding-regex-plan.md - Viewer regex search, encoding picker, and tail mode
- [x] 2026-05-29 pending-dir-size-hourglass-plan.md - Per-directory hourglass while folder sizes are still updating
- [x] 2026-05-31 per-folder-icons.md - Distinctive per-folder icons in the file list
- [x] 2026-06-03 go-to-path-plan.md - Keyboard-first "Go to path" (⌘G) jump action
- [x] 2026-06-04 explorer-architecture-plan.md - Master explorer refactor to module store and typed dispatch
- [x] 2026-06-05 explorer-store-phase1-plan.md - Un-trap explorer state into one module store
- [x] 2026-06-05 explorer-command-bus-phase2-plan.md - Route every command entry path through one typed dispatch bus
- [x] 2026-06-05 explorer-navigation-phase3-plan.md - Collapse navigation braid into one transactional navigate call
- [x] 2026-06-05 explorer-capabilities-phase4-plan.md - Capability-driven virtual-volume guards replacing volume-id
      strings
- [x] 2026-06-05 command-handler-record-plan.md - Convert command dispatch switch to a typed flat handler record
- [x] 2026-06-05 e2e-stability-quick-wins-plan.md - SMB fixture resilience plus looser Playwright Linux-lane timeouts
- [x] 2026-06-05 folder-merge-plan.md - Always-merge folder conflicts plus instant same-volume moves
- [x] 2026-06-05 smb-shared-stack-plan.md - Share the SMB Docker fixture stack across worktrees safely
- [x] 2026-06-06 drag-out-file-promises-plan.md - Drag files out of MTP and SMB panes via file promises
- [x] 2026-06-06 indexing-status-indicator-plan.md - Soften drive-indexing status into an unobtrusive hourglass
- [x] 2026-06-06 progressive-scan-sizes-plan.md - Show growing partial folder sizes during a full scan
- [x] 2026-06-06 scan-progress-eta-plan.md - Add scan progress percent and ETA to the indicator
- [x] 2026-06-06 shortcut-display-unification-plan.md - Unify and make truthful every shortcut shown in the UI
- [x] 2026-06-08 typed-events-plan.md - Make Tauri event names and payloads generated and typed
- [x] 2026-06-09 beta-analytics-plan.md - Privacy-clean beta usage analytics with a true daily-active count
- [x] 2026-06-11 query-dialogs-overhaul-plan.md - Overhaul the Search and Select dialogs end to end
- [ ] 2026-06-11 whats-new-popup-plan.md - Post-update "What's new" changelog dialog
- [ ] 2026-06-13 dropdown-uniformization-plan.md - Converge every dropdown onto two reusable macOS-y Ark primitives
- [ ] 2026-06-13 editable-favorites-plan.md - User-editable favorites (add, remove, rename, reorder) in the volume
      switcher
- [ ] 2026-06-14 viewer-media-plan.md - Render images (incl. HEIC, SVG) and PDFs inline in the File viewer via
      WKWebView, no Rust decoder
- [x] 2026-06-16 friendly-error-text-to-frontend-plan.md - Move all user-facing error prose from Rust to the frontend
      (typed reason + params over IPC), keeping classification in Rust; step 1 of i18n-readiness
- [ ] 2026-06-16 i18n-formatter-layer-plan.md - Route numbers, file sizes, and the system date through one locale-aware
      formatting layer with a single locale source; step 3 of i18n-readiness (formatters only, no plurals/catalog)
- [ ] 2026-06-16 i18n-runtime-plan.md - Custom thin i18n runtime + JSON message catalog (intl-messageformat ICU engine,
      Svelte `<Trans>`, generated `MessageKey` types, semantic scoped keys, no TMS); step 2 of i18n-readiness
- [ ] 2026-06-16 i18n-screenshots-plan.md - Auto-couple a context screenshot to every catalog key via runtime
      capture-mode + a Playwright driver; visual context for translator agents (i18n follow-on)
- [ ] 2026-06-17 i18n-translation-maintenance-plan.md - Translation-readiness + maintenance tooling (pseudolocale,
      stale-detection via source-hash, placeholder/ICU/plural/key-parity checks, translator guide); pseudolocale as the
      universal test fixture; English-only today, ready for the first real locales
- [x] 2026-06-15 doc-context-diet-plan.md - Shrank the resident agent-doc bundle (9.5k → 2k words): re-homed desktop
      content, ratcheted CLAUDE.md toward 600, mandated sibling DETAILS.md, dieted the rules to path-scoped homes,
      enforced with checks (details-sibling, resident-doc-budget, dead-links)
- [ ] 2026-06-16 settings-card-groups-plan.md - Third settings grouping level: rows grouped into `SectionCard`s per
      page, empty cards auto-hidden, search-grouping kept in sync (fixes empty-card and blank-page search bugs)
- [x] 2026-06-19 smb-mtp-indexing-plan.md - Extended drive indexing to SMB and MTP volumes with a new "admittedly stale"
      freshness model, a per-drive status badge, and per-drive last-index duration (per-volume index registry; SMB and
      MTP both index and stay live via smb2 `CHANGE_NOTIFY` / PTP events)
- [x] 2026-06-20 mtp-device-scheduler-plan.md - Foreground-priority MTP device scheduler: the background index scan
      yields the single USB pipe to user nav/copy/delete per bounded unit, and the live watch→index feed buffers the raw
      handle before any device resolve (fixes the ~30 s scan livelock)
- [x] 2026-06-21 transfer-queue-pause-plan.md - Pause/resume + a lane-based queue for copy/move/delete across all volume
      types, via a central Operation Manager wrapping all five spawn paths; Pause + Queue (F2) on progress dialogs and a
      standalone macOS queue window (multi-select, cancel selected, pause all); cancel-only (no rollback) for now
- [ ] 2026-06-22 navigate-during-transfers-plan.md - Make the phone responsive DURING an MTP transfer: the per-chunk
      `CheckpointStream` checkpoint auto-yields the PTP session to foreground nav/list ops (release +
      `background_yield_point` + reopen at offset), with debounce + a min-progress floor; reuses the release-on-pause
      primitive and the device priority gate (op stays Running, not Paused)
- [ ] 2026-06-25 honest-index-sizes-plan.md - Honest directory sizes: exact / ≥lower-bound / unknown plus fresh-vs-stale
      via a per-dir `listed_epoch` + rolled-up `min_subtree_epoch` and a per-volume epoch counter; fixes the mid-scan
      disconnect "0 bytes" lie and lays groundwork for lazy fill and offline browse
- [x] 2026-06-25 2026-06-25-error-report-system-snapshot-plan.md - Attach a richer system snapshot (Mac model, RAM
      breakdown, CPU counts, thermal state, Cmdr's RSS, drive-index size, disk headroom) to error/crash bundles;
      PII-free

## Later

Deferred future work. Unchecked by default; the folder name is the status.

- [ ] 2026-03-10 later/db-first-listings-plan.md - Serve directory listings from the SQLite index for sub-ms navigation
- [ ] 2026-03-10 later/dropbox-sync-status-linux.md - Detect Dropbox sync status on Linux via command socket
- [ ] 2026-03-10 later/linux-builds-plan.md - Add Linux release build target plus website download detection
- [ ] 2026-05-10 later/totalcmd-plugin-analysis.md - Not a spec, but Total Commander packer-plugin research backing
      future archive/plugin work
- [ ] 2026-05-29 later/disk-cleanup-advice-process.md - Not a spec, but reference notes for a future disk-cleanup advice
      feature
- [ ] 2026-06-04 later/agent-spec.md - Persistent in-app agent proposing file operations
- [ ] 2026-06-04 later/data-dir-rename-spec-draft.md - Rename data directories from bundle-id to plain names
- [ ] 2026-06-10 later/codegraph-tauri-resolver.md - Teach CodeGraph to trace Cmdr's Tauri IPC boundary
- [ ] 2026-06-21 later/transfer-queue-v2-plan.md - Transfer queue/pause v2: per-lane budgets (FTP conns), mid-large-file
      pause, concurrent-path pause, connection keep-alive, queue reorder/persist
- [ ] 2026-06-13 later/docs-single-source-sweep.md - Multi-agent sweep to de-duplicate mechanism docs (map points, one
      canonical home)
