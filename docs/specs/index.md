# Specs index

Spec docs and task lists for Cmdr developments, indexed so each stays discoverable. See [README.md](README.md) for what
this folder is and when it gets wiped. Shipped specs get wiped once their durable intent is captured in colocated
`CLAUDE.md`/`DETAILS.md` (and git history); what remains here is deferred work under [`later/`](later/).

## In progress

- [ ] 2026-07-04 [listing-loader-extraction-plan.md](listing-loader-extraction-plan.md) - Drain FilePane's last deferred
      cluster (the listing loader: `loadDirectory`/`handleListingComplete`/reset + streaming listeners + pendingLoad +
      the generation/listingId drop-foreign-listings token model) into a tested `listing-loader.svelte.ts` factory,
      behavior-preserving, `FilePaneAPI` byte-identical.
- [ ] 2026-06-28 local-reconcile-rescan-plan.md - Reclaim index DB disk: recreate-on-schema-mismatch + port the SMB/MTP
      reconcile-in-place rescan onto the local jwalk path (stale sizes stay visible, no freelist balloon)
- [ ] 2026-06-28 [location-type-nav-plan.md](location-type-nav-plan.md) - Make `(volumeId, path)` a first-class
      `Location` and kill bare-path navigation (fixes cross-volume search/⌘G navigating over the wrong volume)
- [ ] 2026-06-28 colorful-tags-plan.md - macOS Finder tags: read + show colored dots (Phase 1), context-menu assign
      (Phase 2)
- [ ] 2026-06-29 [drive-index-progress-plan.md](drive-index-progress-plan.md) - Clearer, unified drive-indexing
      progress: name the drive, count-first honest progress, one shared status model, and a per-volume step checklist.
- [ ] 2026-07-03 [write-ops-managed-plan.md](write-ops-managed-plan.md) - Route rename/mkdir/mkfile through the
      operation manager as scan-free instant ops (busy/eject guard + queue visibility, still result-returning), lift the
      event sink to the IPC edge, and sweep small write-ops debt.
- [ ] 2026-07-03 [mcp-tool-registry-plan.md](mcp-tool-registry-plan.md) - Collapse the 4-way hand-synced MCP tool
      bookkeeping (schema, dispatch, auth gate) into one authored `mcp_tools!` registry, so the bearer-token gate is
      by-construction and a destructive tool can't ship ungated. Wire output stays byte-identical.
- [ ] 2026-07-03 [archive-browsing-plan.md](archive-browsing-plan.md) - Browse + edit zip (and read-only tar/7z)
      archives as folders: `ArchiveVolume` (rc-zip sans-IO read) + batch `ArchiveEditOperation` on the existing op
      manager, transparent `/foo.zip/inner` paths, temp+rename mutation in v1 with in-place append as a fast-follow.
      Executing on the `archive-browsing` worktree; lands as one feature, no partial merges. Supersedes the research in
      `later/totalcmd-plugin-analysis.md`.

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
- [ ] 2026-06-28 later/index-vacuum-reader-pinning.md - Reclaim residual index-DB freelist that long-lived root readers
      stop the incremental vacuum from returning to the OS (deferred: the big freelist sources are now fixed)
- [ ] 2026-06-21 later/transfer-queue-v2-plan.md - Transfer queue/pause v2: per-lane budgets (FTP conns), mid-large-file
      pause, concurrent-path pause, connection keep-alive, queue reorder/persist
- [ ] 2026-06-13 later/docs-single-source-sweep.md - Multi-agent sweep to de-duplicate mechanism docs (map points, one
      canonical home)
- [ ] 2026-06-28 later/drive-index-overall-eta.md - Overall indexing ETA across remaining steps, with the backend
      per-phase calibration it needs to stay honest (the step checklist ships per-step ETA only)
- [ ] 2026-06-30 [later/media-ml-index-plan.md](later/media-ml-index-plan.md) - Searchable image index (OCR, tags,
      faces, text→image) as an ML enrichment layer on the drive index: macOS-native (Vision + Core ML + Foundation
      Models), vectors in SQLite not Postgres, on-device by default with faces/cloud as separate opt-ins. Plan
      reviewed + the Core ML/Rust path spike-verified; backed by `docs/notes/clip-coreml-rust-spike.md`.
