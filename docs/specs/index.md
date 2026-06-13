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

## Later

Deferred future work. Unchecked by default; the folder name is the status.

- [ ] 2026-03-10 later/db-first-listings-plan.md - Serve directory listings from the SQLite index for sub-ms navigation
- [ ] 2026-03-10 later/dropbox-sync-status-linux.md - Detect Dropbox sync status on Linux via command socket
- [ ] 2026-03-10 later/linux-builds-plan.md - Add Linux release build target plus website download detection
- [ ] 2026-03-10 later/viewer-menu-swap-plan.md - Per-window menu bar for the viewer on macOS
- [ ] 2026-04-01 later/rust-word-wrap-heights-plan.md - Move viewer word-wrap height calculation from JS to Rust
- [ ] 2026-05-10 later/totalcmd-plugin-analysis.md - Not a spec, but Total Commander packer-plugin research backing
      future archive/plugin work
- [ ] 2026-05-29 later/disk-cleanup-advice-process.md - Not a spec, but reference notes for a future disk-cleanup advice
      feature
- [ ] 2026-06-04 later/agent-spec.md - Persistent in-app agent proposing file operations
- [ ] 2026-06-04 later/data-dir-rename-spec-draft.md - Rename data directories from bundle-id to plain names
- [ ] 2026-06-10 later/codegraph-tauri-resolver.md - Teach CodeGraph to trace Cmdr's Tauri IPC boundary
- [ ] 2026-06-11 later/viewer-horizontal-virtualization-plan.md - Horizontally virtualize long lines in the file viewer
- [ ] 2026-06-13 later/docs-single-source-sweep.md - Multi-agent sweep to de-duplicate mechanism docs (map points, one
      canonical home)
