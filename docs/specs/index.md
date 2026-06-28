# Specs index

Spec docs and task lists for Cmdr developments, indexed so each stays discoverable. See [README.md](README.md) for what
this folder is and when it gets wiped. Shipped specs get wiped once their durable intent is captured in colocated
`CLAUDE.md`/`DETAILS.md` (and git history); what remains here is deferred work under [`later/`](later/).

## In progress

- [ ] 2026-06-28 local-reconcile-rescan-plan.md - Reclaim index DB disk: recreate-on-schema-mismatch + port the
      SMB/MTP reconcile-in-place rescan onto the local jwalk path (stale sizes stay visible, no freelist balloon)

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
