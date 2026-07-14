# Specs index

Spec docs and task lists for Cmdr developments, indexed so each stays discoverable. See [README.md](README.md) for what
this folder is and when it gets wiped. Shipped specs get wiped once their durable intent is captured in colocated
`CLAUDE.md`/`DETAILS.md` (and git history); what remains here is deferred work under [`later/`](later/).

## In progress

- [ ] 2026-07-14 [idle-cpu-throttle-plan.md](idle-cpu-throttle-plan.md) - Cut idle CPU + disk-write thrash from live
      indexing (#37). L1 (importance folded-key subtree clear) and M1 (per-file live-upsert throttle: 60 s leading +
      trailing, 2%/512 KiB bypass, Downloads-exempt, self-write loop subsumed) shipped; M2 (search-index prealloc
      right-size) and M3 (per-hot-directory coalescing) remain. Backend-only, no schema/marker (pane file sizes are live
      `lstat`). See `indexing/DETAILS.md` § "Live per-file write throttle".
- [ ] 2026-07-13 [media-ml-index-plan.md](media-ml-index-plan.md) - Searchable image index (OCR, tags, faces,
      text→image) as an ML enrichment layer on the drive index: macOS-native (Vision + Core ML + Foundation Models),
      vectors in SQLite not Postgres, on-device by default with faces/cloud as separate opt-ins. Plan reviewed + the
      Core ML/Rust path spike-verified (`docs/notes/clip-coreml-rust-spike.md`), then re-grounded on the shipped
      `importance/`, lifecycle-bus, and `agent/` subsystems (copy the plumbing, not build it); importance-prioritized
      enrichment + a settings threshold slider added. Execution pending.
- [ ] 2026-07-12 [ask-cmdr-plan.md](ask-cmdr-plan.md) - Implementation plan for the "Ask Cmdr" chat slice: 9 milestones
      (`AgentLlm` trait + fake → `main.db` store → registry read/write gating + split → in-process tool layer →
      runtime + context assembly → rail UI + streaming → sessions/search/attachments → consent/settings/i18n/E2E → LLM
      call logging), the `main.db` DDL (conversations/messages/FTS5/cost_meter, FTS5 net-new), the `AgentLlm` typed-part
      trait sketch, the IPC surface, and resolutions to every spec §7 open question. Plan ready; execution pending
      (`/execute` next).
- [ ] 2026-07-12 [ask-cmdr-spec.md](ask-cmdr-spec.md) - "Ask Cmdr" chat slice of the agent: read-only LLM chat over the
      drive index + importance + operation log via the in-process tool registry, `AgentLlm` trait over `genai` (gated on
      the agent-spec §18.1 capability spike), `main.db` conversations/messages/FTS, right-sidebar rail UI with sessions
      and search. Spec ready; see the plan above.
- [ ] 2026-07-12 [ask-cmdr-genai-spike.md](ask-cmdr-genai-spike.md) - The completed genai capability spike (spec §3 step
      0 / agent-spec §18.1): verdict that `genai =0.6.0-beta.19` drives multi-step tool loops, streaming-with-tools, and
      stop-reason/usage normalization on all adapters, but reasoning-state round-trip is broken on the Anthropic and
      OpenAI-Responses adapters (upstream #213) and correct on Gemini. Shapes the `AgentLlm` typed-part design and the
      reasoning-off-in-v1 posture. Referenced by the plan.
- [ ] 2026-07-09 [compress-level-plan.md](compress-level-plan.md) - Extend the shipped Compress feature: a
      compression-level slider (deflate 1-9, default 6) in both the Compress dialog and Settings › Behavior › Archives,
      one FE-owned setting threaded through `route_archive_copy_into` → the mutator's `FileOptions` (governs
      copy/move-into-archive too); plus a spike-gated estimated-result-size line driven off the byte-scan by cheap
      deflate sampling, shipping only if it clears an accuracy + resource bar.
- [ ] 2026-07-09 [operation-log-plan.md](operation-log-plan.md) - Durable, cross-volume journal of file mutations with
      rollback: a new `operation-log.db` (the app's first durable DB, with a forward-migration ladder + retention),
      per-item capture at the operation-manager chokepoint, two-axis status (execution + rollback) plus initiator
      provenance (user / ai_client / agent), rollback as inverse ops through the managed pipeline, indexed name search,
      MCP query+rollback tools, retention settings, a Debug panel, and a thin alpha "Operation log" dialog.
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

## Shipped, awaiting wipe

Done and merged; each entry stays until its durable intent is confirmed captured in the colocated C+D.md, then gets
wiped.

- [x] 2026-07-09 [mcp-agent-surface-plan.md](mcp-agent-surface-plan.md) - Catch the MCP server up with ~2 months of
      features and ready it as the future in-app agent's substrate: per-volume `cmdr://indexing` + `indexing` tool, the
      `cmdr://importance` resource (offline-capable), operation-queue visibility (`operations:`) + `queue` tool +
      terminal-ops ring, `rename`/named-create/trash-mode/`tag`/`eject`/`favorites`, race-free `await` conditions
      (operation, indexing), generic soft-dialog close, uniform `volumes:`, and a tool-description pass (registry 33 →
      39 tools; new `IfRollback` gate). SHIPPED 2026-07-10 after live dogfooding (which also fixed a focus-divergence
      data-safety bug and a `/tmp` search-scope bug). Wipe once the durable intent is confirmed captured in the mcp/,
      importance/, indexing/, and write_operations/ C+D.md.

- [x] 2026-07-09 [compress-feature-plan.md](compress-feature-plan.md) - Add a Compress command (menu, palette, ⌥F5, MCP)
      that opens the Transfer dialog as a third mode (Copy/Move/Compress) and packs the cursor item or selection into a
      new zip at the other pane's path. Backend seeds a 22-byte valid empty zip at the target and routes through the
      existing `route_archive_copy_into` machinery; zip-only, LOCAL and REMOTE (SMB/MTP) destinations (a remote parent
      seeds through the parent volume). SHIPPED 2026-07-09 (all milestones, final review passed); wipe once the durable
      intent is confirmed captured in the transfer/, archive_edit, and mcp C+D.md. Future work stays in the plan's
      "Decided questions" (tar/7z creation, compression-level option).
- [x] 2026-07-07 [pane-toasts-and-rename-identity-plan.md](pane-toasts-and-rename-identity-plan.md) - Pane-scoped
      transient-toast dismissal (background navigation events stop wiping unrelated toasts app-wide) + the inline rename
      editor keyed by path instead of index (kills a latent wrong-row data-safety bug; rename follows its row through
      diffs). SHIPPED 2026-07-07; wipe once the durable intent is confirmed captured in the ui/ and rename/ C+D.md.
- [x] 2026-07-07 [paste-clipboard-as-file-plan.md](paste-clipboard-as-file-plan.md) - Cmd+V with non-file clipboard
      content (text/image/PDF) creates `pasted.*` in the pane, cursor lands on it, inline rename auto-starts
      (setting-gated), info toast with Settings deep link (issue #35). Shipped in b0de3824f.
- [x] 2026-07-08 [importance-subsystem-plan.md](importance-subsystem-plan.md) - A neutral, deterministic
      folder-importance subsystem (pure Rust scorer over listing metadata) exposed as a general read API for any
      feature: separate per-volume `importance.db`, a minimal neutral lifecycle bus in `indexing/`, an explain call, and
      offline-unmounted reads. Known consumers (agent, media-ML enrichment) wire in via their own plans. SHIPPED
      2026-07-08 (M1–M4); durable intent lives in `importance/` and `indexing/` C+D.md. Follow-ups (weight tuning, the
      `kMDItemLastUsedDate` sampling cost) survive in the plan's open-questions. Wipe once the C+D.md capture is
      confirmed.
- [x] 2026-07-03 [archive-browsing-plan.md](archive-browsing-plan.md) - Browse + edit zip (and read-only tar/7z)
      archives as folders: `ArchiveVolume` (rc-zip sans-IO read) + batch `ArchiveEditOperation` on the existing op
      manager, transparent `/foo.zip/inner` paths, temp+rename mutation. SHIPPED 2026-07-06 (merged to `main`, fully
      i18n-ized, remaining polish captured in `archive-browsing-polish.md`); wipe once the durable intent is confirmed
      captured in the colocated archive/write-ops C+D.md. Supersedes the research in
      `later/totalcmd-plugin-analysis.md`.
- [x] 2026-07-07 [archive-browsing-polish.md](archive-browsing-polish.md) - Ranked follow-ups to the shipped
      archive-browsing feature. SHIPPED 2026-07-08 (the executed batch): one-pass sequential extract (the O(n²) cliff),
      ZipCrypto password-prompt extraction (dialog + retry + remember-per-archive, 10 locales), remote-source copy-into,
      remote temp reaping, move-out per-entry convergence (+ a latent data-loss fix), the archive folder split, and the
      dev-side warn debt. SHIPPED 2026-07-09: WinZip-AES + 7z AES decrypt end to end (the `smb2` `aes` pin was relaxed
      to stable in `smb2 0.12.1`), including browse-time prompting for header-encrypted 7z; and SMB push-refresh for
      remote archives (the share watcher forwards a changed `.zip` to open inner listings; MTP stays manual by
      contract). Still deferred IN the spec, each with a settled design or trigger: fast tail-add zip edits
      (clone+tail-rewrite design spike-validated 2026-07-09, see `notes/m-append-spike.md`; SMB path needs an smb2
      copychunk client API), open-with-external for inner files (design spiked), and MTP in-place editing (stretch).
      Wipe the shipped sections once the C+D.md capture is confirmed; the deferred items then move back under `later/`.

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
- [ ] 2026-07-14 [later/default-file-manager-spec.md](later/default-file-manager-spec.md) - Reveal-in-Cmdr
      (`NSFileViewer` redirect) + `public.folder` default handler: two opt-in toggles (default OFF, onboarding step 4 +
      Settings), `RunEvent::Opened` plumbing with cold-start buffering, sanctioned `NSWorkspace` registration, and a
      spike checklist to run before building
