# Specs index

Spec docs and task lists for Cmdr developments, indexed so each stays discoverable. See `README.md` for what this folder
is and when it gets wiped. Shipped specs get wiped once their durable intent is captured in colocated
`CLAUDE.md`/`DETAILS.md` (and git history); what remains here is deferred work under `later/`.

## In progress

Nothing active right now. Shipped plans have been wiped (durable intent lives in the colocated `CLAUDE.md`/`DETAILS.md`
and git history); deferred work sits under `later/`.

## Later

Deferred future work. Unchecked by default; the folder name is the status. Each entry notes what shipped and what's
left, so the durable intent survives the wipe.

- [ ] 2026-07-22 `later/swap-scan-plan.md` - Build-and-swap rescan: run the fast parallel guarded walker into a separate
      `index-{vid}.building.db`, then swap it in atomically (~8.4× faster, 107 s vs 897 s), replacing the ~15-minute
      serial in-place reconcile of a completed LOCAL index. Durable `.swap` marker + idempotent open-time recovery
      guarantees exactly one complete index across any crash. NOT STARTED (only the plan + reviews exist; reconcile is
      still the sole rescan path). Foundation: `docs/notes/swap-scan-feasibility.md`,
      `docs/notes/indexing-benchmarks-2026-07-21.md`.
- [ ] 2026-07-22 `later/sealed-subtrees-plan.md` - Bound the cost of pathological high-churn directories without lying
      about folder sizes (motivated by a 7-minute, 1 GB cold-start stall from one 1.14M-file directory). M1 (two-teeth
      child-count guard in post-replay verification) SHIPPED. M2–M5 (seal a subtree to its `dir_stats` aggregate + a
      bounded head of large files, churn-rolled seal root, periodic re-anchoring, a distinct "approximate" size state)
      NOT STARTED and probably never needed: M1 alone may be the whole fix, so M2–M5 stay gated behind measured residual
      pain.
- [ ] 2026-07-21 `later/natural-language-bulk-rename-hardening-handoff.md` - Hardening continuation for the shipped
      natural-language bulk rename. All hardening landed (atomic no-overwrite, dependency-aware execution, live
      conflict/source detection, review warnings, truncation disclosure, plus a follow-up closing local rename/rollback
      safety gaps) EXCEPT finding 5: record both "agent proposed" and "user approved" provenance in the operation log.
      That's the only remaining work.
- [ ] 2026-07-07 `later/archive-browsing-polish.md` - Follow-ups to the shipped archive-browsing feature. SHIPPED:
      one-pass sequential extract, ZipCrypto + WinZip-AES + 7z-AES decrypt end to end, remote-source copy-into, remote
      temp reaping, move-out per-entry convergence, the archive folder split, and SMB push-refresh for remote archives.
      DEFERRED (each with a settled design or trigger): fast tail-add zip edits (clone+tail-rewrite design validated in
      `docs/notes/m-append-spike.md`; the SMB path needs an smb2 copychunk client API), open-with-external for inner
      files (design spiked), and MTP in-place editing (stretch).
- [ ] 2026-07-08 `later/importance-subsystem-plan.md` - Neutral, deterministic folder-importance subsystem (per-volume
      `importance.db`, a minimal lifecycle bus in `indexing/`, an explain call, offline-unmounted reads), exposed as a
      general read API. SHIPPED (M1–M4); durable intent lives in the `importance/` and `indexing/` `CLAUDE.md`/
      `DETAILS.md`. Open follow-ups: weight tuning, and the `kMDItemLastUsedDate` sampling cost.
- [ ] 2026-07-13 `later/media-ml-index-plan.md` - Searchable image index (OCR, tags, faces, text→image) as an ML
      enrichment layer on the drive index: macOS-native (Vision + Core ML + Foundation Models), vectors in SQLite,
      on-device by default. SHIPPED: M1/M1.5/M2 (backend + OCR foundation), M3 (natural-language CLIP semantic search;
      the CLIP path is gated dark until the model artifacts are uploaded), M6 (photo-search agent/MCP tool). PARKED:
      M4a/M4b faces (David wants to be closer in the loop), and M5 LLM captions (optional).
- [ ] 2026-06-28 `later/colorful-tags-plan.md` - macOS Finder tags: read + show colored dots, and context-menu assign.
      SHIPPED (M0–M3); durable intent lives in the colocated `CLAUDE.md`/`DETAILS.md`. Remaining is minor polish only:
      quiet backfill, in-place search-results refresh, a locale native-string pass, and David's visual QA.
- [ ] 2026-03-10 `later/db-first-listings-plan.md` - Serve directory listings from the SQLite index for sub-ms
      navigation.
- [ ] 2026-03-10 `later/dropbox-sync-status-linux.md` - Detect Dropbox sync status on Linux via command socket.
- [ ] 2026-03-10 `later/linux-builds-plan.md` - Add Linux release build target plus website download detection.
- [ ] 2026-05-10 `later/totalcmd-plugin-analysis.md` - Not a spec, but Total Commander packer-plugin research backing
      future archive/plugin work.
- [ ] 2026-05-29 `later/disk-cleanup-advice-process.md` - Not a spec, but reference notes for a future disk-cleanup
      advice feature.
- [ ] 2026-07-18 `later/out-of-process-indexing.md` - Deferred escalation: move drive and media indexing into a separate
      OS process for a hard "can't starve the UI" guarantee. Not needed now (thread QoS + bounded logging closed the
      levers; the resilience fix stopped the source); captures the seams, the clean per-volume-WAL data-safety split,
      the `ai/process.rs` sidecar prior art, and the effort/tradeoffs, with revisit triggers.
- [ ] 2026-06-04 `later/agent-spec.md` - Persistent in-app agent proposing file operations.
- [ ] 2026-06-04 `later/data-dir-rename-spec-draft.md` - Rename data directories from bundle-id to plain names.
- [ ] 2026-06-28 `later/index-vacuum-reader-pinning.md` - Reclaim residual index-DB freelist that long-lived root
      readers stop the incremental vacuum from returning to the OS (deferred: the big freelist sources are now fixed).
- [ ] 2026-06-21 `later/transfer-queue-v2-plan.md` - Transfer queue/pause v2: per-lane budgets (FTP conns),
      mid-large-file pause, concurrent-path pause, connection keep-alive, queue reorder/persist.
- [ ] 2026-06-13 `later/docs-single-source-sweep.md` - Multi-agent sweep to de-duplicate mechanism docs (map points, one
      canonical home).
- [ ] 2026-06-28 `later/drive-index-overall-eta.md` - Overall indexing ETA across remaining steps, with the backend
      per-phase calibration it needs to stay honest (the step checklist ships per-step ETA only).
- [ ] 2026-07-14 `later/default-file-manager-spec.md` - Reveal-in-Cmdr (`NSFileViewer` redirect) + `public.folder`
      default handler: two opt-in toggles (default OFF, onboarding step 4 + Settings), `RunEvent::Opened` plumbing with
      cold-start buffering, sanctioned `NSWorkspace` registration, and a spike checklist to run before building.
